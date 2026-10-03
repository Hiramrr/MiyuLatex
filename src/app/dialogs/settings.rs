//! Ventana de preferencias.

use super::*;

impl App {
    pub(super) fn settings_dialog(&mut self, ctx: &egui::Context) {
        if self.settings {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("Preferencias")
                .open(&mut open)
                .resizable(false)
                .vscroll(true)
                .default_height(640.0)
                .default_width(420.0)
                .show(ctx, |ui| {
                    if action(ui, "Cerrar preferencias", true, "Cierra esta ventana. Los cambios ya están guardados.").clicked() { ui.close_kind(egui::UiKind::Window); }
                    ui.label("Estos ajustes se aplican a todos los proyectos y se guardan al cambiarlos.");
                    ui.separator();
                    ui.label("Tema");
                    egui::ComboBox::from_id_salt("theme")
                        .selected_text(&self.config.theme)
                        .show_ui(ui, |ui| {
                            for theme in theme::builtin() {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.config.theme,
                                        theme.name.clone(),
                                        &theme.name,
                                    )
                                    .changed();
                            }
                        });
                    ui.separator();
                    ui.label("Fondo de la interfaz");
                    ui.horizontal(|ui| {
                        if action(ui, "Elegir fondo…", true, "Elige una imagen para el fondo de la interfaz.").clicked() {
                            self.choose_background(ctx);
                        }
                        if action(ui, "Quitar fondo", self.backdrop.image.is_some(), "Elimina la imagen de fondo de la interfaz. Requiere una imagen de fondo.").clicked()
                        {
                            self.config.background.clear();
                            self.backdrop = Backdrop::default();
                            self.background_texture = None;
                            changed = true;
                        }
                    });
                    if self.backdrop.image.is_some() {
                        ui.label(
                            Path::new(&self.config.background)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy(),
                        );
                        changed |= ui
                            .checkbox(
                                &mut self.config.background_palette,
                                "Usar colores de la foto",
                            )
                            .changed();
                        ui.horizontal(|ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.config.background_style,
                                    "dither".into(),
                                    "Tramado",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.config.background_style,
                                    "plain".into(),
                                    "Liso",
                                )
                                .changed();
                        });
                        changed |= ui
                            .add(
                                egui::Slider::new(
                                    &mut self.config.background_intensity,
                                    0.25..=1.0,
                                )
                                .text("Intensidad"),
                            )
                            .changed();
                    }
                    ui.separator();
                    ui.label("Paneles");
                    changed |= ui.checkbox(&mut self.config.show_sidebar, "Mostrar panel de archivos, esquema y referencias").changed();
                    ui.horizontal(|ui| {
                        ui.label("Panel lateral:");
                        changed |= ui.radio_value(&mut self.config.sidebar_right, false, "Izquierda").changed();
                        changed |= ui.radio_value(&mut self.config.sidebar_right, true, "Derecha").changed();
                    });
                    ui.horizontal(|ui| {
                        ui.label("Vista previa:");
                        changed |= ui.radio_value(&mut self.config.preview_left, true, "Izquierda").changed();
                        changed |= ui.radio_value(&mut self.config.preview_left, false, "Derecha").changed();
                    });
                    ui.separator();
                    ui.label("Motor LaTeX general");
                    let available = compiler::engines();
                    egui::ComboBox::from_id_salt("engine")
                        .selected_text(&self.config.engine)
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.config.engine,
                                    "auto".into(),
                                    "Automático",
                                )
                                .changed();
                            for (engine, _) in available {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.config.engine,
                                        engine.clone(),
                                        &engine,
                                    )
                                    .changed();
                            }
                        });
                    changed |= ui
                        .checkbox(
                            &mut self.config.autocompile,
                            "Compilar al dejar de escribir",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.config.autosave,
                            "Guardar LaTeX y bibliografía tras 2 s sin escribir",
                        )
                        .changed();
                    changed |= ui
                        .checkbox(
                            &mut self.config.soft_wrap,
                            "Ajustar líneas al ancho del editor",
                        )
                        .changed();
                    changed |= crate::spell::preferences(ui, &mut self.config);
                    changed |= ui
                        .checkbox(
                            &mut self.config.restore_session,
                            "Volver a la última sesión al abrir sin argumentos",
                        )
                        .changed();
                    if ui
                        .checkbox(&mut self.config.invert_preview, "Invertir colores del PDF")
                        .changed()
                    {
                        self.preview.invert = self.config.invert_preview;
                        self.preview.request();
                        for doc in &mut self.documents {
                            if let Some(pdf) = &mut doc.pdf {
                                pdf.invert = self.config.invert_preview;
                                pdf.request();
                            }
                        }
                        changed = true;
                    }
                    ui.separator();
                    changed |= crate::custom::preferences(
                        ui,
                        &mut self.config,
                        &self.theme,
                        &mut self.message,
                    );
                    ui.separator();

                });
            self.settings = open;
            if changed {
                self.preferences_changed(ctx);
            }
        }
    }
}
