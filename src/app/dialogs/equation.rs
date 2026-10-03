//! Ventana con la fórmula que rodea al cursor, compilada aparte.

use super::*;

type Rendered = Result<(egui::ColorImage, bool), String>;

#[derive(Default)]
pub(in crate::app) struct Equation {
    pub(in crate::app) open: bool,
    /// Huella de la fórmula que se muestra o se está compilando.
    key: u64,
    /// Fórmula bajo el cursor que aún no se ha compilado y desde cuándo espera.
    waiting: Option<(u64, Instant)>,
    job: Option<Receiver<Rendered>>,
    texture: Option<TextureHandle>,
    /// Se compiló sin el preámbulo del documento porque con él fallaba.
    plain: bool,
    pub(in crate::app) error: String,
}

impl Equation {
    #[cfg(test)]
    pub(in crate::app) fn ready(&self) -> bool {
        self.job.is_none() && self.waiting.is_none()
    }
    #[cfg(test)]
    pub(in crate::app) fn size(&self) -> Option<egui::Vec2> {
        self.texture.as_ref().map(TextureHandle::size_vec2)
    }
}

impl App {
    pub(in crate::app) fn toggle_equation(&mut self) {
        self.equation = Equation {
            open: !self.equation.open,
            ..Default::default()
        };
    }
    /// Fórmula que rodea al cursor del documento activo.
    fn math_at_cursor(&self) -> Option<&str> {
        let editor = self.editor();
        if editor.format != Format::Latex {
            return None;
        }
        let cursor = editor.cursor;
        let before: usize = editor.lines[..cursor.row].iter().map(|l| l.len() + 1).sum();
        let at = before + crate::editor::byte_col(&editor.lines[cursor.row], cursor.col);
        crate::equation::math_at(editor.source(), at)
    }
    fn start_equation(&mut self, math: String, ctx: &egui::Context) {
        let root = self.root();
        let engine = match &root {
            Some(root) => {
                let preference = if self.project_settings.engine.is_empty() {
                    &self.config.engine
                } else {
                    &self.project_settings.engine
                };
                compiler::select_engine(root, preference)
            }
            None => compiler::engines().into_iter().next().ok_or(
                "No encontré un motor LaTeX. Instala Tectonic o una distribución TeX.".into(),
            ),
        };
        let engine = match engine {
            Ok(engine) => engine,
            Err(e) => {
                self.equation.error = e;
                return;
            }
        };
        // El preámbulo sale del documento principal, con sus cambios sin guardar.
        let text = match &root {
            Some(root) => self
                .completion_sources()
                .into_iter()
                .find(|s| s.path == *root)
                .map(|s| s.text),
            None => Some(self.editor().text()),
        };
        let folder = root
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        let pixels = 1.6 * ctx.pixels_per_point();
        let invert = self.config.invert_preview;
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(|| {
                crate::equation::render(
                    &math,
                    text.as_deref(),
                    &engine,
                    folder.as_deref(),
                    pixels,
                    invert,
                )
            })
            .unwrap_or_else(|_| Err("No pude compilar la fórmula".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.equation.job = Some(rx);
    }
    pub(super) fn equation_dialog(&mut self, ctx: &egui::Context) {
        if !self.equation.open {
            return;
        }
        if let Some(result) = self.equation.job.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.equation.job = None;
            match result {
                Ok((image, plain)) => {
                    self.equation.texture =
                        Some(ctx.load_texture("equation", image, TextureOptions::LINEAR));
                    self.equation.plain = plain;
                    self.equation.error.clear();
                }
                Err(e) => self.equation.error = e,
            }
        }
        let math = self.math_at_cursor().map(str::to_owned);
        if let Some(math) = &math {
            let key = egui::util::hash((math, self.config.invert_preview));
            if key == self.equation.key {
                self.equation.waiting = None;
            } else if self
                .equation
                .waiting
                .is_none_or(|(waiting, _)| waiting != key)
            {
                self.equation.waiting = Some((key, Instant::now()));
            }
            // Se espera a que deje de cambiar y a que acabe la compilación anterior.
            if let Some((key, since)) = self.equation.waiting {
                if self.equation.job.is_none() && since.elapsed() >= Duration::from_millis(350) {
                    self.equation.key = key;
                    self.equation.waiting = None;
                    self.start_equation(math.clone(), ctx);
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
        }
        let mut open = true;
        egui::Window::new("Ecuación")
            .open(&mut open)
            .default_pos(ctx.content_rect().center_bottom() - egui::vec2(180.0, 220.0))
            .default_width(360.0)
            .show(ctx, |ui| {
                if let Some(texture) = &self.equation.texture {
                    let size = texture.size_vec2() / ctx.pixels_per_point();
                    ScrollArea::both().max_height(360.0).show(ui, |ui| {
                        ui.image(egui::load::SizedTexture::new(texture.id(), size));
                    });
                }
                if !self.equation.error.is_empty() {
                    ui.colored_label(col(self.theme.error), &self.equation.error);
                } else if self.equation.plain {
                    ui.label(
                        RichText::new("Compilada sin el preámbulo del documento, que no funciona por separado.")
                            .size(12.0)
                            .color(col(self.theme.muted())),
                    );
                }
                if self.equation.job.is_some() {
                    ui.spinner();
                } else if math.is_none() && self.equation.texture.is_none() {
                    ui.label("Pon el cursor dentro de una fórmula: $…$, \\[…\\] o un entorno como equation o align.");
                }
            });
        self.equation.open = open;
    }
}
