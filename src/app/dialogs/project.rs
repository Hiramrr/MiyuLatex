//! Búsqueda, historial y ajustes del proyecto.

use super::*;

impl App {
    pub(super) fn project_search_dialog(&mut self, ctx: &egui::Context) {
        if self.project_search {
            let mut open = true;
            let mut jump = None;
            egui::Window::new("Buscar en el proyecto")
                .open(&mut open)
                .default_size([700.0, 440.0])
                .show(ctx, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.project_query)
                            .hint_text("Texto que buscar")
                            .desired_width(f32::INFINITY),
                    );
                    if response.changed() {
                        self.search_project();
                    }
                    ui.label(format!(
                        "{} líneas encontradas. Se muestran hasta 500.",
                        self.search_results.len()
                    ));
                    ScrollArea::vertical().show(ui, |ui| {
                        for target in &self.search_results {
                            let path = target
                                .path
                                .strip_prefix(&self.project)
                                .unwrap_or(&target.path);
                            if ui
                                .button(format!(
                                    "{}:{}  {}",
                                    path.display(),
                                    target.row + 1,
                                    target.label
                                ))
                                .clicked()
                            {
                                jump = Some(target.clone());
                            }
                        }
                    });
                });
            self.project_search = open;
            if let Some(target) = jump {
                self.jump(&target);
                self.project_search = false;
            }
        }
    }
    pub(super) fn history_dialog(&mut self, ctx: &egui::Context) {
        if self.history {
            let mut open = true;
            let mut restore = false;
            egui::Window::new("Historial del archivo").open(&mut open).default_size([900.0, 560.0]).show(ctx, |ui| {
                ui.label("Últimas 100 versiones guardadas. Restaurar cambia el editor y permite deshacer antes de guardar.");
                if self.history_versions.is_empty() { ui.label("Todavía no hay versiones anteriores. Se conservan al guardar cambios."); return; }
                let mut index = self.history_index;
                egui::ComboBox::from_id_salt("history_version").selected_text(format!("Versión {} de {}", index + 1, self.history_versions.len())).show_ui(ui, |ui| {
                    for (i, path) in self.history_versions.iter().enumerate() {
                        let age = path.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(0, |d| d.as_secs());
                        ui.selectable_value(&mut index, i, format!("Versión {} · hace {} min", i + 1, age / 60));
                    }
                });
                if index != self.history_index {
                    self.history_index = index;
                    match fs::read_to_string(&self.history_versions[index]) {
                        Ok(text) => self.history_text = Some(text),
                        Err(e) => { self.history_text = None; self.message = e.to_string(); }
                    }
                }
                restore = action(ui, "Restaurar versión en el editor", self.history_text.is_some(), "Reemplaza el texto del editor con esta versión. Puedes deshacerlo antes de guardar. Requiere una versión que se pueda leer.").clicked();
                let current = self.documents.iter().find(|d| d.editor.path == self.history_file).map(|d| d.editor.text()).unwrap_or_default();
                ui.columns(2, |columns| {
                    columns[0].label("Versión anterior");
                    ScrollArea::both().id_salt("history_old").max_height(420.0).show(&mut columns[0], |ui| {
                        let mut text = self.history_text.as_deref().unwrap_or_default();
                        ui.add(TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
                    });
                    columns[1].label("Texto actual");
                    ScrollArea::both().id_salt("history_current").max_height(420.0).show(&mut columns[1], |ui| {
                        let mut text = current.as_str();
                        ui.add(TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
                    });
                });
            });
            self.history = open;
            if restore
                && let Some(path) = self.history_file.clone()
                && let Some(text) = self.history_text.clone()
            {
                match self.open(&path) {
                    Ok(()) => {
                        let text = text.replace("\r\n", "\n");
                        self.editor_mut().set_text(&text);
                        self.changed_editor();
                        self.history = false;
                    }
                    Err(e) => self.message = e,
                }
            }
        }
    }
    pub(super) fn project_options_dialog(&mut self, ctx: &egui::Context) {
        if self.project_options {
            let mut open = true;
            let mut changed = false;
            egui::Window::new("Configurar proyecto LaTeX")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(self.project.display().to_string());
                    ui.label("Estos ajustes se guardan en este proyecto al cambiarlos.");
                    ui.separator();
                    ui.label("Archivo principal del proyecto");
                    let main = self
                        .project_settings
                        .main
                        .as_ref()
                        .map_or("Automático".into(), |p| p.display().to_string());
                    egui::ComboBox::from_id_salt("main_document")
                        .selected_text(main)
                        .width(350.0)
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.project_settings.main,
                                    None,
                                    "Automático",
                                )
                                .changed();
                            for file in &self.files {
                                if file.extension().is_some_and(|e| e == "tex")
                                    && let Ok(path) = file.strip_prefix(&self.project)
                                {
                                    changed |= ui
                                        .selectable_value(
                                            &mut self.project_settings.main,
                                            Some(path.into()),
                                            path.display().to_string(),
                                        )
                                        .changed();
                                }
                            }
                        });
                    ui.label("Motor de este proyecto");
                    egui::ComboBox::from_id_salt("project_engine")
                        .selected_text(if self.project_settings.engine.is_empty() {
                            "Usar preferencia general"
                        } else {
                            &self.project_settings.engine
                        })
                        .show_ui(ui, |ui| {
                            changed |= ui
                                .selectable_value(
                                    &mut self.project_settings.engine,
                                    String::new(),
                                    "Usar preferencia general",
                                )
                                .changed();
                            changed |= ui
                                .selectable_value(
                                    &mut self.project_settings.engine,
                                    "auto".into(),
                                    "Automático y directiva !TEX program",
                                )
                                .changed();
                            for engine in compiler::ENGINES {
                                changed |= ui
                                    .selectable_value(
                                        &mut self.project_settings.engine,
                                        (*engine).into(),
                                        *engine,
                                    )
                                    .changed();
                            }
                        });
                    ui.separator();
                    if action(
                        ui,
                        "Cerrar configuración",
                        true,
                        "Cierra esta ventana. Los cambios ya están guardados en el proyecto.",
                    )
                    .clicked()
                    {
                        ui.close_kind(egui::UiKind::Window);
                    }
                });
            self.project_options = open;
            if changed {
                self.project_changed();
            }
        }
    }
}
