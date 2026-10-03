//! Búsqueda, historial y ajustes del proyecto.

use super::*;

/// Búsqueda en todo el proyecto; la consulta se conserva entre aperturas.
#[derive(Default)]
pub(in crate::app) struct ProjectSearch {
    pub(in crate::app) open: bool,
    pub(in crate::app) query: String,
    /// Hasta 500 líneas que coinciden con la consulta.
    pub(in crate::app) results: Vec<Target>,
}

/// Versiones anteriores de un archivo; solo existe con la ventana abierta.
pub(in crate::app) struct History {
    file: PathBuf,
    versions: Vec<PathBuf>,
    index: usize,
    /// Texto de la versión elegida, si se pudo leer.
    text: Option<String>,
}

impl History {
    pub(in crate::app) fn new(file: PathBuf) -> (Self, Result<(), String>) {
        let mut history = Self { versions: latex::versions(&file), file, index: 0, text: None };
        let read = if history.versions.is_empty() { Ok(()) } else { history.select(0) };
        (history, read)
    }

    fn select(&mut self, index: usize) -> Result<(), String> {
        self.index = index;
        self.text = None;
        let text = fs::read_to_string(&self.versions[index])
            .map_err(|e| format!("No pude leer la versión: {e}"))?;
        self.text = Some(text);
        Ok(())
    }
}

impl App {
    pub(super) fn project_search_dialog(&mut self, ctx: &egui::Context) {
        if self.search.open {
            let mut open = true;
            let mut jump = None;
            egui::Window::new("Buscar en el proyecto")
                .open(&mut open)
                .default_size([700.0, 440.0])
                .show(ctx, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.search.query)
                            .hint_text("Texto que buscar")
                            .desired_width(f32::INFINITY),
                    );
                    if response.changed() {
                        self.search_project();
                    }
                    ui.label(format!(
                        "{} líneas encontradas. Se muestran hasta 500.",
                        self.search.results.len()
                    ));
                    ScrollArea::vertical().show(ui, |ui| {
                        for target in &self.search.results {
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
            self.search.open = open;
            if let Some(target) = jump {
                self.jump(&target);
                self.search.open = false;
            }
        }
    }
    pub(super) fn history_dialog(&mut self, ctx: &egui::Context) {
        // Se saca mientras se dibuja para leer los documentos a la vez.
        let Some(mut history) = self.history.take() else {
            return;
        };
        let mut open = true;
        let mut restore = false;
        egui::Window::new("Historial del archivo").open(&mut open).default_size([900.0, 560.0]).show(ctx, |ui| {
            ui.label("Últimas 100 versiones guardadas. Restaurar cambia el editor y permite deshacer antes de guardar.");
            if history.versions.is_empty() { ui.label("Todavía no hay versiones anteriores. Se conservan al guardar cambios."); return; }
            let mut index = history.index;
            egui::ComboBox::from_id_salt("history_version").selected_text(format!("Versión {} de {}", index + 1, history.versions.len())).show_ui(ui, |ui| {
                for (i, path) in history.versions.iter().enumerate() {
                    let age = path.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map_or(0, |d| d.as_secs());
                    ui.selectable_value(&mut index, i, format!("Versión {} · hace {} min", i + 1, age / 60));
                }
            });
            if index != history.index
                && let Err(e) = history.select(index)
            {
                self.message = e;
            }
            restore = action(ui, "Restaurar versión en el editor", history.text.is_some(), "Reemplaza el texto del editor con esta versión. Puedes deshacerlo antes de guardar. Requiere una versión que se pueda leer.").clicked();
            let current = self.documents.iter().find(|d| d.editor.path.as_ref() == Some(&history.file)).map(|d| d.editor.text()).unwrap_or_default();
            ui.columns(2, |columns| {
                columns[0].label("Versión anterior");
                ScrollArea::both().id_salt("history_old").max_height(420.0).show(&mut columns[0], |ui| {
                    let mut text = history.text.as_deref().unwrap_or_default();
                    ui.add(TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
                });
                columns[1].label("Texto actual");
                ScrollArea::both().id_salt("history_current").max_height(420.0).show(&mut columns[1], |ui| {
                    let mut text = current.as_str();
                    ui.add(TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
                });
            });
        });
        if restore && let Some(text) = &history.text {
            match self.open(&history.file) {
                Ok(()) => {
                    let text = text.replace("\r\n", "\n");
                    self.editor_mut().set_text(&text);
                    self.changed_editor();
                    return;
                }
                Err(e) => self.message = e,
            }
        }
        if open {
            self.history = Some(history);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_reads_the_newest_version_first() {
        let folder = std::env::temp_dir().join(format!("miyu-history-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("main.tex");
        fs::write(&path, "uno").unwrap();
        let (empty, read) = History::new(path.clone());
        assert!(read.is_ok() && empty.versions.is_empty() && empty.text.is_none());
        latex::checkpoint(&path, "uno").unwrap();
        latex::checkpoint(&path, "dos").unwrap();
        let (mut history, read) = History::new(path);
        assert!(read.is_ok());
        assert_eq!(history.versions.len(), 2);
        assert_eq!(history.text.as_deref(), Some("dos"));
        history.select(1).unwrap();
        assert_eq!((history.index, history.text.as_deref()), (1, Some("uno")));
        fs::remove_file(&history.versions[0]).unwrap();
        assert!(history.select(0).is_err());
        assert!(history.text.is_none());
        fs::remove_dir_all(folder).unwrap();
    }
}
