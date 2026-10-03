use std::{
    collections::{BTreeMap, HashMap},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

use crate::pdftext::{self, PageText};
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use hayro::{
    RenderCache, RenderSettings,
    hayro_interpret::{InterpreterCache, InterpreterSettings},
    hayro_syntax::{Pdf, page::Page},
    render,
    vello_cpu::color::palette::css::WHITE,
};
use image::{DynamicImage, RgbaImage};

/// Lado mayor de una página rasterizada, en píxeles.
const MAX_SIDE: f32 = 4096.0;
/// Páginas rasterizadas que se conservan a la vez.
const KEPT: usize = 12;
const ZOOMS: &[f32] = &[50.0, 75.0, 100.0, 125.0, 150.0, 200.0, 300.0];
const MARGIN: f32 = 8.0;
const GAP: f32 = 10.0;

/// Píxeles RGBA de una página a `scale` píxeles por punto PDF.
fn raster(page: &Page, scale: f32, invert: bool) -> (u32, u32, Vec<u8>) {
    let settings = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
        ..Default::default()
    };
    let pixmap = render(
        page,
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &settings,
    );
    let mut data = pixmap.data_as_u8_slice().to_vec();
    if invert {
        for p in data.as_chunks_mut::<4>().0 {
            p[0] = 255 - p[0];
            p[1] = 255 - p[1];
            p[2] = 255 - p[2];
        }
    }
    (pixmap.width() as u32, pixmap.height() as u32, data)
}

fn valid(size: (f32, f32)) -> bool {
    size.0.is_finite() && size.1.is_finite() && size.0 > 0.0 && size.1 > 0.0
}

pub fn render_page(
    data: Vec<u8>,
    index: usize,
    invert: bool,
) -> Result<(DynamicImage, usize, usize), String> {
    let pdf = Pdf::new(data).map_err(|e| format!("PDF dañado o incompleto: {e:?}"))?;
    let pages = pdf.pages();
    let count = pages.len();
    if count == 0 {
        return Err("El PDF no tiene páginas".into());
    }
    let index = index.min(count - 1);
    let (width, height) = pages[index].render_dimensions();
    if !valid((width, height)) {
        return Err("El PDF tiene dimensiones de página inválidas".into());
    }
    let scale = 1.5f32.min(MAX_SIDE / width.max(height));
    let (width, height, data) = raster(&pages[index], scale, invert);
    let img = RgbaImage::from_raw(width, height, data).ok_or("No pude rasterizar el PDF")?;
    Ok((DynamicImage::ImageRgba8(img), count, index))
}

/// Primera página de un PDF, a `pixels` píxeles por punto.
pub fn render_first(data: Vec<u8>, pixels: f32, invert: bool) -> Result<ColorImage, String> {
    let pdf = Pdf::new(data).map_err(|e| format!("PDF dañado o incompleto: {e:?}"))?;
    let pages = pdf.pages();
    if pages.is_empty() {
        return Err("El PDF no tiene páginas".into());
    }
    let (width, height) = pages[0].render_dimensions();
    if !valid((width, height)) {
        return Err("El PDF tiene dimensiones de página inválidas".into());
    }
    let scale = pixels.min(MAX_SIDE / width.max(height));
    let (width, height, data) = raster(&pages[0], scale, invert);
    Ok(ColorImage::from_rgba_premultiplied(
        [width as usize, height as usize],
        &data,
    ))
}

struct Request {
    page: usize,
    scale: f32,
    invert: bool,
    generation: u64,
    ctx: egui::Context,
}
enum Reply {
    /// Tamaño de cada página en puntos PDF.
    Opened(Vec<(f32, f32)>),
    Page {
        page: usize,
        scale: f32,
        generation: u64,
        image: Option<ColorImage>,
    },
    Text {
        page: usize,
        text: PageText,
    },
    /// Ya llegó el texto de todas las páginas.
    TextDone,
    Failed(String),
}

/// Lee el texto de todas las páginas en su propio hilo, para no retrasar
/// el rasterizado de las que se ven.
fn read_text(data: Vec<u8>, replies: Sender<Reply>) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Ok(pdf) = Pdf::new(data) else { return };
        let cache = InterpreterCache::new();
        let pages = pdf.pages();
        for page in 0..pages.len() {
            let text = catch_unwind(AssertUnwindSafe(|| pdftext::extract(&pages[page], &cache)))
                .unwrap_or_default();
            if replies.send(Reply::Text { page, text }).is_err() {
                return;
            }
        }
    }));
    let _ = replies.send(Reply::TextDone);
}

/// El hilo conserva el PDF abierto: rasterizar otra página no vuelve a leerlo.
fn work(data: Vec<u8>, requests: Receiver<Request>, replies: Sender<Reply>) {
    let unreadable = || Reply::Failed("El rasterizador no pudo leer el PDF".into());
    let pdf = match catch_unwind(AssertUnwindSafe(|| Pdf::new(data))) {
        Ok(Ok(pdf)) => pdf,
        Ok(Err(e)) => {
            let _ = replies.send(Reply::Failed(format!("PDF dañado o incompleto: {e:?}")));
            return;
        }
        Err(_) => {
            let _ = replies.send(unreadable());
            return;
        }
    };
    let sizes = catch_unwind(AssertUnwindSafe(|| {
        let pages = pdf.pages();
        (0..pages.len())
            .map(|i| pages[i].render_dimensions())
            .collect::<Vec<_>>()
    }));
    let reply = match sizes {
        Ok(sizes) if sizes.is_empty() => Reply::Failed("El PDF no tiene páginas".into()),
        Ok(sizes) if !sizes.iter().all(|s| valid(*s)) => {
            Reply::Failed("El PDF tiene dimensiones de página inválidas".into())
        }
        Ok(sizes) => Reply::Opened(sizes),
        Err(_) => unreadable(),
    };
    let failed = matches!(reply, Reply::Failed(_));
    if replies.send(reply).is_err() || failed {
        return;
    }
    for request in requests {
        let image = catch_unwind(AssertUnwindSafe(|| {
            let (width, height, data) =
                raster(&pdf.pages()[request.page], request.scale, request.invert);
            // El fondo es opaco: premultiplicado y sin premultiplicar coinciden.
            ColorImage::from_rgba_premultiplied([width as usize, height as usize], &data)
        }))
        .ok();
        let sent = replies.send(Reply::Page {
            page: request.page,
            scale: request.scale,
            generation: request.generation,
            image,
        });
        request.ctx.request_repaint();
        if sent.is_err() {
            return;
        }
    }
}

struct Raster {
    texture: TextureHandle,
    /// Píxeles por punto PDF.
    scale: f32,
    generation: u64,
}

pub struct Preview {
    pub path: Option<PathBuf>,
    /// Página que ocupa el centro de la vista, desde 0.
    pub page: usize,
    pub count: usize,
    /// Porcentaje del ancho disponible que ocupa la página.
    pub zoom: f32,
    pub invert: bool,
    pub error: String,
    pub loading: bool,
    sizes: Vec<(f32, f32)>,
    rasters: HashMap<usize, Raster>,
    /// Páginas que no se pudieron rasterizar, con su generación.
    broken: HashMap<usize, u64>,
    /// Cambia cuando lo rasterizado deja de valer: otro PDF u otros colores.
    generation: u64,
    requests: Option<Sender<Request>>,
    replies: Option<Receiver<Reply>>,
    opening: bool,
    in_flight: Option<usize>,
    /// Página a la que desplazar la vista en el próximo cuadro.
    target: Option<usize>,
    /// Resolución pedida y desde cuándo; no se rehace mientras cambia.
    wanted: (f32, Instant),
    /// Texto de las páginas que ya se leyeron.
    texts: BTreeMap<usize, PageText>,
    /// Aún falta leer el texto de alguna página.
    reading: bool,
    /// Texto que se busca en el PDF.
    query: String,
    /// Coincidencias de `query`: página y tramo de su texto, en orden.
    matches: Vec<(usize, usize, usize)>,
    current: Option<usize>,
    /// Llevar la vista a la coincidencia actual en el próximo cuadro.
    reveal: bool,
    /// Texto seleccionado: página, carácter donde empezó y donde acaba.
    selection: Option<(usize, usize, usize)>,
}

impl Preview {
    pub fn new(invert: bool) -> Self {
        Self {
            path: None,
            page: 0,
            count: 0,
            zoom: 100.0,
            invert,
            error: String::new(),
            loading: false,
            sizes: Vec::new(),
            rasters: HashMap::new(),
            broken: HashMap::new(),
            generation: 0,
            requests: None,
            replies: None,
            opening: false,
            in_flight: None,
            target: None,
            wanted: (0.0, Instant::now()),
            texts: BTreeMap::new(),
            reading: false,
            query: String::new(),
            matches: Vec::new(),
            current: None,
            reveal: false,
            selection: None,
        }
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    /// Busca `query` en el texto leído y va a la primera coincidencia desde
    /// la página que se ve.
    pub fn search(&mut self, query: &str) {
        self.query = query.into();
        self.matches = self
            .texts
            .iter()
            .flat_map(|(page, text)| {
                text.find(query)
                    .into_iter()
                    .map(|(start, end)| (*page, start, end))
            })
            .collect();
        self.current = self
            .matches
            .iter()
            .position(|found| found.0 >= self.page)
            .or((!self.matches.is_empty()).then_some(0));
        self.reveal = self.current.is_some();
    }
    pub fn find_next(&mut self, backwards: bool) {
        let count = self.matches.len();
        if count == 0 {
            return;
        }
        self.current = Some(match self.current {
            Some(current) if backwards => (current + count - 1) % count,
            Some(current) => (current + 1) % count,
            None => 0,
        });
        self.reveal = true;
    }
    /// Posición de la coincidencia actual, desde 1, y cuántas hay.
    pub fn found(&self) -> (Option<usize>, usize) {
        (self.current.map(|i| i + 1), self.matches.len())
    }
    /// Falta leer el texto de alguna página: la búsqueda aún puede crecer.
    pub fn reading(&self) -> bool {
        self.reading
    }
    pub fn selected_text(&self) -> Option<String> {
        let (page, anchor, head) = self.selection?;
        let text = self.texts.get(&page)?;
        Some(text.text(anchor.min(head), anchor.max(head) + 1))
    }
    /// Páginas con imagen lista.
    #[cfg(test)]
    pub fn rendered(&self) -> usize {
        self.rasters.len()
    }
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        if data.is_empty() {
            return Err("El PDF está vacío".into());
        }
        if self.path.as_deref() != Some(path) {
            self.page = 0;
            self.count = 0;
            self.sizes.clear();
            self.rasters.clear();
            self.target = None;
        }
        // Las coincidencias vuelven a aparecer según se lee el texto nuevo.
        self.texts.clear();
        self.matches.clear();
        self.current = None;
        self.selection = None;
        self.reading = true;
        // Al recompilar se ven las páginas anteriores hasta que llegan las nuevas.
        self.generation += 1;
        self.broken.clear();
        self.path = Some(path.into());
        let (request_tx, request_rx) = mpsc::channel();
        let (reply_tx, reply_rx) = mpsc::channel();
        let (text_data, text_tx) = (data.clone(), reply_tx.clone());
        thread::spawn(move || work(data, request_rx, reply_tx));
        thread::spawn(move || read_text(text_data, text_tx));
        // Soltar el canal anterior termina el hilo del PDF anterior.
        self.requests = Some(request_tx);
        self.replies = Some(reply_rx);
        self.opening = true;
        self.in_flight = None;
        self.loading = true;
        self.error.clear();
        Ok(())
    }
    /// Descarta lo rasterizado, por ejemplo al invertir los colores.
    pub fn request(&mut self) {
        self.generation += 1;
        self.broken.clear();
    }
    pub fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        loop {
            let reply = match self.replies.as_ref().map(Receiver::try_recv) {
                Some(Ok(reply)) => reply,
                Some(Err(mpsc::TryRecvError::Disconnected))
                    if self.opening || self.in_flight.is_some() =>
                {
                    Reply::Failed("El rasterizador se cerró".into())
                }
                _ => break,
            };
            changed = true;
            match reply {
                Reply::Opened(sizes) => {
                    self.opening = false;
                    self.count = sizes.len();
                    self.page = self.page.min(self.count - 1);
                    self.rasters.retain(|page, _| *page < sizes.len());
                    self.sizes = sizes;
                }
                Reply::Page {
                    page,
                    scale,
                    generation,
                    image,
                } => {
                    self.in_flight = None;
                    match image {
                        Some(image) if generation == self.generation => {
                            let texture = ctx.load_texture(
                                format!("pdf-{page}"),
                                image,
                                TextureOptions::LINEAR,
                            );
                            self.rasters.insert(
                                page,
                                Raster {
                                    texture,
                                    scale,
                                    generation,
                                },
                            );
                            self.evict();
                        }
                        Some(_) => {}
                        None => {
                            self.broken.insert(page, generation);
                            self.error = format!("No pude rasterizar la página {}", page + 1);
                        }
                    }
                }
                Reply::Text { page, text } => {
                    if !self.query.is_empty() {
                        let found = text.find(&self.query);
                        self.matches
                            .extend(found.into_iter().map(|(start, end)| (page, start, end)));
                        if self.current.is_none() && !self.matches.is_empty() {
                            self.current = Some(0);
                        }
                    }
                    self.texts.insert(page, text);
                }
                Reply::TextDone => self.reading = false,
                Reply::Failed(error) => {
                    self.reading = false;
                    self.texts.clear();
                    self.matches.clear();
                    self.current = None;
                    self.selection = None;
                    self.error = error;
                    self.opening = false;
                    self.in_flight = None;
                    self.requests = None;
                    self.replies = None;
                    self.rasters.clear();
                    self.sizes.clear();
                    self.count = 0;
                    self.page = 0;
                }
            }
        }
        self.loading = self.opening || self.in_flight.is_some() || self.reading;
        changed
    }
    fn evict(&mut self) {
        while self.rasters.len() > KEPT {
            let current = self.page;
            let Some(far) = self
                .rasters
                .keys()
                .copied()
                .max_by_key(|p| p.abs_diff(current))
            else {
                break;
            };
            self.rasters.remove(&far);
        }
    }
    pub fn go_to(&mut self, page: usize) {
        let page = page.min(self.count.saturating_sub(1));
        self.page = page;
        self.target = Some(page);
    }
    pub fn change_page(&mut self, delta: isize) {
        self.go_to(self.page.saturating_add_signed(delta));
    }
    pub fn change_zoom(&mut self, delta: isize) {
        let next = if delta > 0 {
            ZOOMS.iter().copied().find(|z| *z > self.zoom + 0.5)
        } else {
            ZOOMS.iter().rev().copied().find(|z| *z < self.zoom - 0.5)
        };
        if let Some(zoom) = next {
            self.zoom = zoom;
        }
    }
    fn fresh(&self, page: usize) -> bool {
        self.rasters
            .get(&page)
            .is_some_and(|r| r.generation == self.generation)
            || self.broken.get(&page) == Some(&self.generation)
    }
    fn ask(&mut self, page: usize, scale: f32, ctx: &egui::Context) {
        let request = Request {
            page,
            scale,
            invert: self.invert,
            generation: self.generation,
            ctx: ctx.clone(),
        };
        if self
            .requests
            .as_ref()
            .is_some_and(|tx| tx.send(request).is_ok())
        {
            self.in_flight = Some(page);
            self.loading = true;
        }
    }
    /// Dibuja todas las páginas en una columna desplazable. `marker` resalta
    /// una posición (página y puntos PDF) y `reveal` lleva la vista hasta ella.
    /// Devuelve la página y el punto PDF de un doble clic o de Cmd+clic.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        id: egui::Id,
        marker: Option<(usize, f32, f32)>,
        reveal: &mut bool,
        accent: Color32,
    ) -> Option<(usize, f32, f32)> {
        if self.sizes.is_empty() {
            if !self.loading && self.error.is_empty() {
                ui.label("Compila el documento para ver el PDF.");
            }
            return None;
        }
        let ctx = ui.ctx().clone();
        let available = ui.available_width();
        let widest = self.sizes.iter().map(|s| s.0).fold(1.0, f32::max);
        // Puntos de pantalla por punto PDF.
        let scale = (available - 2.0 * MARGIN).max(50.0) * self.zoom / 100.0 / widest;
        let mut tops = Vec::with_capacity(self.sizes.len());
        let mut bottom = MARGIN;
        for size in &self.sizes {
            tops.push(bottom);
            bottom += size.1 * scale + GAP;
        }
        let total = egui::vec2(
            (widest * scale + 2.0 * MARGIN).max(available),
            bottom - GAP + MARGIN,
        );
        let mut clicked = None;
        let mut zoom = self.zoom;
        egui::ScrollArea::both()
            .id_salt(id)
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (area, response) = ui.allocate_exact_size(total, egui::Sense::click_and_drag());
                let sizes = &self.sizes;
                let page_rect = |page: usize| {
                    let size = egui::vec2(sizes[page].0, sizes[page].1) * scale;
                    egui::Rect::from_min_size(
                        area.min + egui::vec2((total.x - size.x) / 2.0, tops[page]),
                        size,
                    )
                };
                let first = tops
                    .partition_point(|top| *top <= viewport.top())
                    .saturating_sub(1);
                let visible: Vec<usize> = (first..sizes.len())
                    .take_while(|page| tops[*page] < viewport.bottom())
                    .collect();
                let middle = viewport.center().y;
                let distance = |page: usize| {
                    let (top, bottom) = (tops[page], tops[page] + sizes[page].1 * scale);
                    (top - middle).max(middle - bottom).max(0.0)
                };
                let mut current = visible
                    .iter()
                    .copied()
                    .min_by(|a, b| distance(*a).total_cmp(&distance(*b)))
                    .unwrap_or(self.page);
                let painter = ui.painter();
                let paper = if self.invert {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                };
                for &page in &visible {
                    let rect = page_rect(page);
                    painter.rect_filled(rect, 0.0, paper);
                    if let Some(raster) = self.rasters.get(&page) {
                        painter.image(
                            raster.texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    painter.rect_stroke(
                        rect,
                        0.0,
                        ui.visuals().widgets.noninteractive.bg_stroke,
                        egui::StrokeKind::Outside,
                    );
                }
                // Caja del texto de una página, en la pantalla.
                let on_screen = |page: usize, b: pdftext::Bounds| {
                    let origin = page_rect(page).min;
                    egui::Rect::from_min_max(
                        origin + egui::vec2(b[0], b[1]) * scale,
                        origin + egui::vec2(b[2], b[3]) * scale,
                    )
                };
                let found = Color32::from_rgba_unmultiplied(255, 196, 0, 90);
                let first_match = self.matches.partition_point(|m| m.0 < first);
                for (i, &(page, start, end)) in self.matches.iter().enumerate().skip(first_match) {
                    if visible.last().is_none_or(|last| page > *last) {
                        break;
                    }
                    let Some(text) = self.texts.get(&page) else {
                        continue;
                    };
                    for bounds in text.rects(start, end) {
                        let rect = on_screen(page, bounds);
                        painter.rect_filled(rect, 2.0, found);
                        if self.current == Some(i) {
                            painter.rect_stroke(
                                rect,
                                2.0,
                                egui::Stroke::new(1.5, accent),
                                egui::StrokeKind::Outside,
                            );
                        }
                    }
                }
                if let Some((page, anchor, head)) = self.selection
                    && let Some(text) = self.texts.get(&page)
                    && page < sizes.len()
                {
                    for bounds in text.rects(anchor.min(head), anchor.max(head) + 1) {
                        painter.rect_filled(
                            on_screen(page, bounds),
                            0.0,
                            accent.gamma_multiply(0.35),
                        );
                    }
                }
                // Carácter de una página más cercano a un punto de la pantalla.
                let texts = &self.texts;
                let character = |page: usize, position: egui::Pos2| {
                    let point = (position - page_rect(page).min) / scale;
                    texts.get(&page)?.nearest(point.x, point.y)
                };
                if response.drag_started_by(egui::PointerButton::Primary) {
                    self.selection = ui
                        .input(|i| i.pointer.press_origin())
                        .and_then(|origin| {
                            let page = visible
                                .iter()
                                .copied()
                                .find(|page| page_rect(*page).contains(origin))?;
                            Some((page, character(page, origin)?))
                        })
                        .map(|(page, at)| (page, at, at));
                    response.request_focus();
                } else if response.dragged_by(egui::PointerButton::Primary)
                    && let Some((page, anchor, _)) = self.selection
                    && let Some(position) = response.interact_pointer_pos()
                    && let Some(head) = character(page, position)
                {
                    self.selection = Some((page, anchor, head));
                } else if response.clicked() {
                    self.selection = None;
                    response.request_focus();
                }
                if response.hovered()
                    && let Some(position) = response.hover_pos()
                    && visible.iter().any(|page| {
                        page_rect(*page).contains(position)
                            && texts.get(page).is_some_and(|text| !text.is_empty())
                    })
                {
                    ctx.set_cursor_icon(egui::CursorIcon::Text);
                }
                let selected = self.selected_text();
                if response.has_focus()
                    && let Some(text) = &selected
                    && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
                {
                    ctx.copy_text(text.clone());
                }
                response.context_menu(|ui| {
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Copiar"))
                        .clicked()
                    {
                        ctx.copy_text(selected.clone().unwrap_or_default());
                        ui.close();
                    }
                });
                let mut target = self.target.take().filter(|p| *p < sizes.len());
                if std::mem::take(&mut self.reveal)
                    && let Some(&(page, start, end)) =
                        self.current.and_then(|i| self.matches.get(i))
                    && let Some(bounds) = self
                        .texts
                        .get(&page)
                        .and_then(|text| text.rects(start, end).into_iter().next())
                    && page < sizes.len()
                {
                    ui.scroll_to_rect(on_screen(page, bounds), Some(egui::Align::Center));
                    target = None;
                    current = page;
                }
                if let Some((page, _, y)) = marker
                    && page < sizes.len()
                {
                    let rect = page_rect(page);
                    // `y` es la línea base: la banda cubre la altura de la letra.
                    let band = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.top() + (y - 10.0) * scale),
                        egui::pos2(rect.right(), rect.top() + (y + 4.0) * scale),
                    );
                    painter.rect_filled(band, 0.0, accent.gamma_multiply(0.22));
                    painter.rect_stroke(
                        band,
                        0.0,
                        egui::Stroke::new(1.0, accent),
                        egui::StrokeKind::Inside,
                    );
                    if *reveal {
                        ui.scroll_to_rect(band, Some(egui::Align::Center));
                        *reveal = false;
                        target = None;
                        current = page;
                    }
                }
                if let Some(page) = target {
                    let rect = page_rect(page);
                    ui.scroll_to_rect(
                        egui::Rect::from_min_max(
                            rect.min - egui::vec2(0.0, MARGIN),
                            egui::pos2(rect.right(), rect.top()),
                        ),
                        Some(egui::Align::TOP),
                    );
                    current = page;
                }
                if (response.double_clicked()
                    || (response.clicked() && ui.input(|i| i.modifiers.command)))
                    && let Some(position) = response.interact_pointer_pos()
                    && let Some(page) = visible
                        .iter()
                        .copied()
                        .find(|page| page_rect(*page).contains(position))
                {
                    let point = (position - page_rect(page).min) / scale;
                    clicked = Some((page, point.x, point.y));
                }
                if response.contains_pointer() {
                    let factor = ui.input(|i| i.zoom_delta());
                    if factor != 1.0 {
                        zoom = (zoom * factor).clamp(25.0, 400.0);
                    }
                }
                // Se pide de una en una, empezando por la más cercana al centro.
                let pixels = scale * ctx.pixels_per_point();
                let resolution = |size: (f32, f32)| pixels.min(MAX_SIDE / size.0.max(size.1));
                let mut order = visible.clone();
                order.sort_by_key(|page| page.abs_diff(current));
                let stale = order.iter().copied().find(|page| !self.fresh(*page));
                let blurry = order.iter().copied().find(|page| {
                    self.rasters
                        .get(page)
                        .is_some_and(|r| (r.scale / resolution(sizes[*page]) - 1.0).abs() > 0.12)
                });
                let after = visible.last().map(|p| p + 1);
                let before = visible.first().and_then(|p| p.checked_sub(1));
                let nearby = [after, before]
                    .into_iter()
                    .flatten()
                    .find(|page| *page < sizes.len() && !self.fresh(*page));
                let count = sizes.len();
                self.page = current.min(count - 1);
                if (pixels - self.wanted.0).abs() > f32::EPSILON {
                    self.wanted = (pixels, Instant::now());
                }
                let settled = self.wanted.1.elapsed() >= Duration::from_millis(150);
                if self.in_flight.is_none() && !self.opening {
                    let next = stale.or(blurry.filter(|_| settled)).or(nearby);
                    if let Some(page) = next {
                        self.ask(page, resolution(self.sizes[page]), &ctx);
                    } else if blurry.is_some() {
                        ctx.request_repaint_after(Duration::from_millis(160));
                    }
                }
            });
        self.zoom = zoom;
        clicked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_and_selects_pdf_text() {
        let ctx = egui::Context::default();
        let mut preview = Preview::new(false);
        preview.search("ecuación");
        preview.load(Path::new("examples/articulo.pdf")).unwrap();
        let started = Instant::now();
        while preview.reading() && started.elapsed() < Duration::from_secs(20) {
            preview.poll(&ctx);
            thread::sleep(Duration::from_millis(5));
        }
        // La búsqueda pendiente se resuelve al llegar el texto.
        assert_eq!(preview.found(), (Some(1), 1));
        preview.search("zzz");
        assert_eq!(preview.found(), (None, 0));
        preview.search("RESUMEN");
        assert_eq!(preview.found(), (Some(1), 2));
        preview.find_next(false);
        assert_eq!(preview.found(), (Some(2), 2));
        preview.find_next(false);
        assert_eq!(preview.found(), (Some(1), 2));
        preview.find_next(true);
        assert_eq!(preview.found(), (Some(2), 2));
        let (page, start, end) = preview.matches[0];
        preview.selection = Some((page, end - 1, start));
        assert_eq!(preview.selected_text().as_deref(), Some("Resumen"));
    }

    #[test]
    fn zoom_steps_and_page_limits() {
        let mut preview = Preview::new(false);
        preview.change_zoom(1);
        assert_eq!(preview.zoom, 125.0);
        preview.zoom = 110.0;
        preview.change_zoom(1);
        assert_eq!(preview.zoom, 125.0);
        preview.zoom = 110.0;
        preview.change_zoom(-1);
        assert_eq!(preview.zoom, 100.0);
        preview.zoom = 300.0;
        preview.change_zoom(1);
        assert_eq!(preview.zoom, 300.0);
        preview.zoom = 40.0;
        preview.change_zoom(-1);
        assert_eq!(preview.zoom, 40.0);
        preview.count = 3;
        preview.change_page(9);
        assert_eq!(preview.page, 2);
        preview.change_page(-9);
        assert_eq!(preview.page, 0);
        assert!(render_page(b"no es un PDF".to_vec(), 0, false).is_err());
    }
}
