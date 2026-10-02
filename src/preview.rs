use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

use hayro::{
    RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax::Pdf, render,
    vello_cpu::color::palette::css::WHITE,
};
use image::{DynamicImage, RgbaImage};

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
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("El PDF tiene dimensiones de página inválidas".into());
    }
    let scale = 1.5f32.min(4096.0 / width.max(height));
    let settings = RenderSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: WHITE,
        ..Default::default()
    };
    let pixmap = render(
        &pages[index],
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &settings,
    );
    let mut img = RgbaImage::from_raw(
        pixmap.width() as u32,
        pixmap.height() as u32,
        pixmap.data_as_u8_slice().to_vec(),
    )
    .ok_or("No pude rasterizar el PDF")?;
    if invert {
        for p in img.pixels_mut() {
            p.0[0] = 255 - p.0[0];
            p.0[1] = 255 - p.0[1];
            p.0[2] = 255 - p.0[2];
        }
    }
    Ok((DynamicImage::ImageRgba8(img), count, index))
}

type RenderResult = Result<(DynamicImage, usize, usize), String>;
pub struct Preview {
    pub path: Option<PathBuf>,
    pub page: usize,
    pub count: usize,
    pub zoom: u16,
    pub invert: bool,
    pub error: String,
    pub loading: bool,
    data: Vec<u8>,
    pub image: Option<DynamicImage>,
    worker: Option<Receiver<RenderResult>>,
    pending: bool,
}
impl Preview {
    pub fn page_size(&self) -> Option<(f32, f32)> {
        let pdf = Pdf::new(self.data.clone()).ok()?;
        Some(pdf.pages().get(self.page)?.render_dimensions())
    }
    pub fn new(invert: bool) -> Self {
        Self {
            path: None,
            page: 0,
            count: 0,
            zoom: 100,
            invert,
            error: String::new(),
            loading: false,
            data: Vec::new(),
            image: None,
            worker: None,
            pending: false,
        }
    }
    pub fn load(&mut self, path: &Path) -> Result<(), String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        if data.is_empty() {
            return Err("El PDF está vacío".into());
        }
        if self.path.as_deref() != Some(path) {
            self.page = 0;
            self.count = 0;
            self.image = None;
        }
        self.data = data;
        self.path = Some(path.into());
        self.request();
        Ok(())
    }
    pub fn request(&mut self) {
        if self.data.is_empty() {
            return;
        }
        if self.worker.is_some() {
            self.pending = true;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let data = self.data.clone();
        let page = self.page;
        let invert = self.invert;
        thread::spawn(move || {
            let result = std::panic::catch_unwind(|| render_page(data, page, invert))
                .unwrap_or_else(|_| Err("El rasterizador no pudo leer esta página".into()));
            let _ = tx.send(result);
        });
        self.worker = Some(rx);
        self.loading = true;
        self.error.clear();
    }
    pub fn poll(&mut self) -> bool {
        let result = match self.worker.as_ref().map(Receiver::try_recv) {
            Some(Ok(r)) => r,
            Some(Err(mpsc::TryRecvError::Disconnected)) => Err("El rasterizador se cerró".into()),
            _ => return false,
        };
        self.worker = None;
        if self.pending {
            self.pending = false;
            self.request();
            return true;
        }
        self.loading = false;
        match result {
            Ok((image, count, page)) => {
                self.image = Some(image);
                self.count = count;
                self.page = page;
            }
            Err(e) => {
                self.error = e;
                self.image = None;
                self.count = 0;
            }
        }
        true
    }
    pub fn change_page(&mut self, delta: isize) {
        let page = self
            .page
            .saturating_add_signed(delta)
            .min(self.count.saturating_sub(1));
        if page != self.page {
            self.page = page;
            self.request();
        }
    }
    pub fn change_zoom(&mut self, delta: isize) {
        const Z: &[u16] = &[50, 75, 100, 125, 150, 200, 300];
        let i = Z.iter().position(|v| *v == self.zoom).unwrap_or(2);
        self.zoom = Z[i.saturating_add_signed(delta).min(Z.len() - 1)];
    }
}
