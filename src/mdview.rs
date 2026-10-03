//! Vista previa de Markdown virtualizada: el documento se parte en trozos de
//! bloques completos y solo se dibujan los que caen en pantalla. Con
//! `CommonMarkViewer::show` sobre todo el texto, cada cuadro maquetaba miles
//! de widgets (60 ms con 15 000 líneas).

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use eframe::egui::{self, Rect, ScrollArea, Ui, UiBuilder, vec2};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Options, Parser, Tag};

/// Líneas mínimas por trozo: menos trozos son menos llamadas al visor, más
/// líneas son más trabajo por trozo visible.
const CHUNK_LINES: usize = 40;
/// Distancia fuera de la pantalla que también se dibuja, para que al
/// desplazarse los trozos ya estén medidos.
const OVERSCAN: f32 = 400.0;
/// Separación entre trozos, la misma que entre párrafos.
const GAP: f32 = 10.0;
/// Margen alrededor del texto.
const MARGIN: f32 = 16.0;

struct Chunk {
    /// Texto del trozo más las definiciones de enlaces de todo el documento.
    text: String,
    key: u64,
    lines: usize,
    headings: Vec<String>,
}

#[derive(Default)]
pub struct MarkdownView {
    /// Documento y revisión de los que salen `chunks`.
    revision: Option<(egui::Id, u64)>,
    chunks: Vec<Chunk>,
    /// Alturas medidas por contenido, válidas para `width`.
    heights: HashMap<u64, f32>,
    width: f32,
    /// Destinos de los enlaces relativos (a otros archivos) del documento.
    pub links: Vec<String>,
    /// Encabezado al que saltar (enlaces `#id`).
    target: Option<String>,
    /// Corrección del desplazamiento cuando cambia la altura de un trozo
    /// que está por encima de la vista.
    shift: f32,
    offset: f32,
}

impl MarkdownView {
    fn split(&mut self, text: &str) {
        let parser = Parser::new_ext(text, options());
        let mut ends = Vec::new();
        let mut headings: Vec<Vec<String>> = vec![Vec::new()];
        let mut depth = 0usize;
        let mut start = 0;
        self.links.clear();
        let mut iter = parser.into_offset_iter();
        for (event, range) in iter.by_ref() {
            match event {
                Event::Start(tag) => {
                    match &tag {
                        Tag::Heading { id: Some(id), .. } => {
                            headings.last_mut().unwrap().push(id.to_string());
                        }
                        Tag::Link { dest_url, .. }
                            if !dest_url.contains(':')
                                && !dest_url.starts_with('#')
                                && !dest_url.is_empty() =>
                        {
                            self.links.push(dest_url.to_string());
                        }
                        _ => {}
                    }
                    depth += 1;
                }
                Event::End(_) => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth == 0 && range.end > start {
                let lines = text[start..range.end].matches('\n').count();
                if lines >= CHUNK_LINES {
                    ends.push(start..range.end);
                    headings.push(Vec::new());
                    start = range.end;
                }
            }
        }
        // Las definiciones `[x]: url` se añaden a cada trozo para que los
        // enlaces por referencia sigan resolviéndose; no se dibujan.
        let mut spans: Vec<Range<usize>> = iter
            .reference_definitions()
            .iter()
            .map(|(_, def)| def.span.clone())
            .collect();
        spans.sort_by_key(|span| span.start);
        let definitions: String = spans
            .into_iter()
            .map(|span| format!("\n\n{}", text[span].trim_end()))
            .collect();
        if start < text.len() || ends.is_empty() {
            ends.push(start..text.len());
        } else {
            headings.pop();
        }
        self.chunks = ends
            .into_iter()
            .zip(headings)
            .map(|(range, headings)| {
                let body = &text[range];
                let text = format!("{body}{definitions}");
                let mut hasher = DefaultHasher::new();
                text.hash(&mut hasher);
                Chunk {
                    key: hasher.finish(),
                    lines: body.lines().count().max(1),
                    headings,
                    text,
                }
            })
            .collect();
    }

    /// Vuelve a partir el documento si cambió. `id` distingue documentos.
    pub fn update(&mut self, id: egui::Id, text: &str, revision: u64) {
        if self.revision != Some((id, revision)) {
            self.split(text);
            self.revision = Some((id, revision));
            let keys: HashSet<u64> = self.chunks.iter().map(|c| c.key).collect();
            self.heights.retain(|key, _| keys.contains(key));
        }
    }

    /// Dibuja `text` dentro de un `ScrollArea`. `viewer` crea el visor con
    /// las opciones de la aplicación.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        id: egui::Id,
        cache: &mut CommonMarkCache,
        text: &str,
        revision: u64,
        width: f32,
        viewer: impl Fn(&Ui) -> CommonMarkViewer<'static>,
    ) {
        self.update(id, text, revision);
        if (self.width - width).abs() > 0.5 {
            self.heights.clear();
            self.width = width;
        }
        let row = ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y;
        let estimate = |chunk: &Chunk| chunk.lines as f32 * row * 0.75;
        let height_of = |heights: &HashMap<u64, f32>, chunk: &Chunk| {
            heights
                .get(&chunk.key)
                .copied()
                .unwrap_or_else(|| estimate(chunk))
        };

        // Salto a un encabezado (enlace `#id`): primero se lleva la vista a
        // su trozo y, cuando ese trozo se dibuja, el visor lo coloca exacto.
        let mut target_chunk = None;
        let mut jump = None;
        if let Some(target) = &self.target {
            let mut y = 0.0;
            for (i, chunk) in self.chunks.iter().enumerate() {
                if chunk.headings.iter().any(|h| h == target) {
                    target_chunk = Some(i);
                    jump = Some(y);
                    break;
                }
                y += height_of(&self.heights, chunk) + GAP;
            }
            if target_chunk.is_none() {
                self.target = None;
            }
        }
        let mut area = ScrollArea::both().id_salt(id).auto_shrink([false, false]);
        let shift = std::mem::take(&mut self.shift);
        if let Some(y) = jump.filter(|y| (y - self.offset).abs() > 1.0) {
            area = area.vertical_scroll_offset(y);
        } else if shift != 0.0 {
            area = area.vertical_scroll_offset(self.offset + shift);
        }
        let output = area.show_viewport(ui, |ui, viewport| {
            let origin = ui.min_rect().min + vec2(MARGIN, MARGIN);
            let visible = viewport.expand2(vec2(0.0, OVERSCAN));
            let mut y = 0.0;
            let mut widest = width;
            for (i, chunk) in self.chunks.iter().enumerate() {
                let known = self.heights.get(&chunk.key).copied();
                let height = known.unwrap_or_else(|| estimate(chunk));
                if y + height < visible.min.y || y > visible.max.y {
                    y += height + GAP;
                    continue;
                }
                if target_chunk == Some(i) {
                    *cache.scroll_to_id_target_mut() = self.target.take();
                }
                let rect = Rect::from_min_size(origin + vec2(0.0, y), vec2(width, f32::INFINITY));
                let drawn = ui
                    .scope_builder(
                        UiBuilder::new()
                            .max_rect(rect)
                            .id_salt(("markdown_chunk", i)),
                        |ui| {
                            ui.set_max_width(width);
                            viewer(ui).show(ui, cache, &chunk.text);
                        },
                    )
                    .response
                    .rect;
                // El visor deja aquí el destino de un enlace `#id` pulsado.
                if let Some(target) = cache.scroll_to_id_target_mut().take() {
                    self.target = Some(target);
                }
                widest = widest.max(drawn.width());
                let measured = drawn.height();
                if known.is_none_or(|h| (h - measured).abs() > 0.5) {
                    self.heights.insert(chunk.key, measured);
                    // Un trozo por encima de la vista que cambia de alto
                    // desplazaría el contenido: se compensa.
                    if y + height <= viewport.min.y {
                        self.shift += measured - height;
                    }
                    ui.ctx().request_repaint();
                }
                y += measured + GAP;
            }
            ui.allocate_rect(
                Rect::from_min_size(origin, vec2(widest + MARGIN, y + MARGIN)),
                egui::Sense::hover(),
            );
        });
        self.offset = output.state.offset.y;
        if self.target.is_some() {
            ui.ctx().request_repaint();
        }
    }
}

/// Las mismas opciones que usa el visor, para cortar por los mismos bloques.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_HEADING_ATTRIBUTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_the_document_with_links_and_headings() {
        let mut source: String = (0..300)
            .map(|i| format!("## Sección {i} {{#s{i}}}\n\nVer [nota](nota{i}.md), [web](https://x.org) y [ref][r].\n\n- uno\n- dos\n\n"))
            .collect();
        source.push_str("[r]: otra.md\n");
        let mut view = MarkdownView::default();
        view.split(&source);
        assert!(view.chunks.len() > 10);
        // Cada trozo empieza donde acaba el anterior y lleva las definiciones.
        let bodies: String = view
            .chunks
            .iter()
            .map(|c| c.text.strip_suffix("\n\n[r]: otra.md").unwrap())
            .collect();
        assert_eq!(bodies, source);
        assert_eq!(view.links.len(), 600);
        assert_eq!(view.links[1], "otra.md");
        assert_eq!(view.links[0], "nota0.md");
        let headings: Vec<_> = view
            .chunks
            .iter()
            .flat_map(|c| c.headings.clone())
            .collect();
        assert_eq!(headings.len(), 300);
        assert_eq!(headings[299], "s299");
        // Lo mismo de nuevo da las mismas huellas: las alturas medidas siguen valiendo.
        let keys: Vec<_> = view.chunks.iter().map(|c| c.key).collect();
        view.split(&source);
        assert_eq!(keys, view.chunks.iter().map(|c| c.key).collect::<Vec<_>>());
    }
}
