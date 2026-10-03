//! Tema, preferencias y fondo animado.

use super::*;

impl App {
    pub(super) fn apply_theme(&mut self, ctx: &egui::Context) {
        self.theme = theme::builtin()
            .into_iter()
            .find(|t| t.name == self.config.theme)
            .unwrap_or_else(|| theme::builtin().remove(0));
        if self.config.background_palette
            && let Some(tone) = self.backdrop.tone
        {
            self.theme = theme::photo_theme(tone, self.theme.dark);
        }
        crate::custom::apply_colors(&mut self.theme, &self.config);
        if let Err(e) = crate::custom::install_fonts(ctx, &self.config) {
            self.message = format!("No pude cargar la fuente: {e}");
        }
        let mut style = egui::Style {
            visuals: if self.theme.dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            },
            ..Default::default()
        };
        style.visuals.override_text_color = Some(col(self.theme.fg));
        style.visuals.panel_fill = self.panel_fill();
        style.visuals.window_fill = col(self.theme.surface);
        style.visuals.extreme_bg_color = col(self.theme.bg);
        style.visuals.faint_bg_color = col(self.theme.panel);
        style.visuals.selection.bg_fill = col(self.theme.selection());
        style.visuals.selection.stroke = Stroke::new(1.0, col(self.theme.fg));
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, col(self.theme.border));
        style.visuals.widgets.inactive.bg_fill = col(self.theme.panel);
        style.visuals.widgets.hovered.bg_fill = col(self.theme.highlight());
        style.visuals.widgets.active.bg_fill = col(self.theme.selection());
        style.visuals.window_shadow = egui::epaint::Shadow::NONE;
        style.visuals.popup_shadow = egui::epaint::Shadow::NONE;
        let text_size = self.config.ui_font_size as f32;
        for font in style.text_styles.values_mut() {
            *font = FontId::proportional(text_size);
        }
        style.text_styles.insert(
            egui::TextStyle::Heading,
            FontId::proportional(text_size + 3.0),
        );
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, FontId::monospace(text_size));
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        let radius = egui::CornerRadius::same(self.config.corner_radius.round() as u8);
        style.visuals.window_corner_radius = radius;
        style.visuals.menu_corner_radius = radius;
        for widget in [
            &mut style.visuals.widgets.noninteractive,
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.open,
        ] {
            widget.corner_radius = radius;
            widget.bg_stroke = Stroke::new(1.0, col(self.theme.border));
        }
        ctx.set_global_style(style);
        self.background_key.clear();
    }
    pub(super) fn preferences_changed(&mut self, ctx: &egui::Context) {
        self.apply_theme(ctx);
        if let Err(e) = self.config.save() {
            self.message = format!("No pude guardar las preferencias: {e}");
        }
    }
    pub(super) fn panel_fill(&self) -> Color32 {
        if self.backdrop.image.is_some() {
            col(self.theme.bg).gamma_multiply(0.78)
        } else {
            col(self.theme.surface)
        }
    }
    pub(super) fn choose_background(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Elegir fondo")
            .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
            .pick_file()
        {
            match self.backdrop.import(&path) {
                Ok(path) => {
                    self.config.background = path;
                    self.preferences_changed(ctx);
                }
                Err(e) => self.message = format!("No pude cargar la imagen: {e}"),
            }
        }
    }
    pub(super) fn paint_background(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        ui.painter().rect_filled(rect, 0.0, col(self.theme.bg));
        let Some(image) = self.backdrop.image.clone() else {
            self.background_job = None;
            return;
        };
        let scale = ui.ctx().pixels_per_point();
        let width = (rect.width() * scale).round().clamp(1.0, 4096.0) as u32;
        let height = (rect.height() * scale).round().clamp(1.0, 4096.0) as u32;
        let key = format!(
            "{width}:{height}:{}:{}:{}:{:?}:{}",
            self.config.background_style,
            self.config.background_intensity,
            self.config.background_dot,
            self.theme.bg,
            self.config.background
        );
        match self.background_job.as_ref().map(Receiver::try_recv) {
            Some(Ok(frame)) => {
                self.background_job = None;
                self.show_background(ui.ctx(), frame);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.background_job = None,
            _ => {}
        }
        if key != self.background_key && self.background_job.is_none() {
            // CELL=2 en píxeles CSS por defecto. En Retina cada punto ocupa 2× la escala física.
            let dot = (self.config.background_dot as f32 * scale).round().max(1.0) as u32;
            let base = self.theme.bg;
            let intensity = self.config.background_intensity;
            let plain = self.config.background_style == "plain";
            let size = rect.size();
            let render = move || {
                let cells =
                    backdrop::render_cells(&image, width, height, dot, base, intensity, plain);
                let color = egui::ColorImage::from_rgb(
                    [cells.width() as usize, cells.height() as usize],
                    cells.as_raw(),
                );
                (key, color, dot as f32 / scale, size)
            };
            if self.background_texture.is_none() {
                self.show_background(ui.ctx(), render());
            } else {
                // Al redimensionar la ventana cambia en cada cuadro: el tramado
                // se rehace fuera del hilo de la interfaz y mientras se ve el anterior.
                let (tx, rx) = mpsc::channel();
                let ctx = ui.ctx().clone();
                thread::spawn(move || {
                    let _ = tx.send(render());
                    ctx.request_repaint();
                });
                self.background_job = Some(rx);
            }
        }
        if let Some(texture) = &self.background_texture {
            let (cell, size) = self.background_layout;
            // La foto va centrada (y al 95 % de alto), así que un tramado de otro
            // tamaño se ancla igual hasta que llega el nuevo.
            let offset = ((rect.size() - size) * egui::vec2(0.5, 0.475) * scale).round() / scale;
            ui.painter().with_clip_rect(rect).image(
                texture.id(),
                egui::Rect::from_min_size(rect.min + offset, texture.size_vec2() * cell),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    pub(super) fn show_background(&mut self, ctx: &egui::Context, frame: BackgroundFrame) {
        let (key, color, cell, size) = frame;
        self.background_texture =
            Some(ctx.load_texture("background", color, TextureOptions::NEAREST));
        self.background_key = key;
        self.background_layout = (cell, size);
    }
}
