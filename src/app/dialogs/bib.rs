//! Revisión de la bibliografía y citas descargadas por DOI o arXiv.

use super::*;

/// Ventana para pedir una cita; el identificador se conserva entre aperturas.
#[derive(Default)]
pub(in crate::app) struct Citation {
    pub(in crate::app) open: bool,
    pub(in crate::app) identifier: String,
    /// Insertar `\cite` con la clave nueva en el cursor.
    pub(in crate::app) insert: bool,
    focus: bool,
}

impl App {
    pub(in crate::app) fn check_bibliography(&mut self) {
        self.refresh_sources();
        self.bib_report = Some(crate::bib::problems(&self.completion_sources()));
    }
    pub(super) fn bib_report_dialog(&mut self, ctx: &egui::Context) {
        let Some(report) = &self.bib_report else {
            return;
        };
        let mut open = true;
        let mut jump = None;
        egui::Window::new("Revisión de la bibliografía")
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                if report.is_empty() {
                    ui.label(
                        "No encontré claves repetidas, campos que falten ni entradas sin citar.",
                    );
                }
                ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for problem in report {
                        let file = problem
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        let text = format!("{file}:{} · {}", problem.row + 1, problem.label);
                        if ui
                            .button(text)
                            .on_hover_text("Abre la entrada en el editor.")
                            .clicked()
                        {
                            jump = Some(problem.clone());
                        }
                    }
                });
            });
        if let Some(target) = jump {
            self.jump(&target);
        }
        if !open {
            self.bib_report = None;
        }
    }
    pub(in crate::app) fn open_citation(&mut self) {
        self.citation.open = true;
        self.citation.focus = true;
        self.citation.insert = self.editor().format == Format::Latex
            && self
                .editor()
                .path
                .as_ref()
                .is_none_or(|p| p.extension().is_none_or(|e| e != "bib"));
    }
    /// Archivo `.bib` del proyecto al que se añaden las citas nuevas.
    fn bibliography(&self) -> Option<PathBuf> {
        self.source_cache
            .iter()
            .map(|s| &s.path)
            .find(|p| p.extension().is_some_and(|e| e == "bib"))
            .cloned()
    }
    pub(super) fn citation_dialog(&mut self, ctx: &egui::Context) {
        if !self.citation.open {
            return;
        }
        let mut open = true;
        let mut fetch = false;
        let bibliography = self.bibliography();
        let busy = self.tool_rx.is_some();
        egui::Window::new("Añadir cita por DOI o arXiv")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let response = ui.add(
                    TextEdit::singleline(&mut self.citation.identifier)
                        .hint_text("10.1145/359576.359579 o arXiv:1706.03762")
                        .desired_width(380.0),
                );
                if std::mem::take(&mut self.citation.focus) {
                    response.request_focus();
                }
                match &bibliography {
                    Some(path) => ui.label(format!(
                        "La entrada se añade a {}. El identificador se consulta en doi.org o arxiv.org.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )),
                    None => ui.colored_label(
                        col(self.theme.error),
                        "El proyecto no tiene un archivo .bib. Crea uno y añádelo con \\addbibresource o \\bibliography.",
                    ),
                };
                ui.checkbox(&mut self.citation.insert, "Insertar \\cite en el cursor");
                let ready = bibliography.is_some()
                    && !busy
                    && crate::bib::lookup(&self.citation.identifier).is_some();
                fetch = ready && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal_wrapped(|ui| {
                    fetch |= action(ui, "Añadir cita", ready, "Descarga la entrada BibTeX y la añade a la bibliografía. Requiere un DOI o un identificador de arXiv válido, un archivo .bib y ninguna otra operación en curso.").clicked();
                    if busy {
                        ui.spinner();
                    }
                });
            });
        self.citation.open = open;
        if fetch {
            self.fetch_citation(ctx);
        }
    }
    fn fetch_citation(&mut self, ctx: &egui::Context) {
        let Some((url, negotiate)) = crate::bib::lookup(&self.citation.identifier) else {
            return;
        };
        self.message = "Descargando la cita…".into();
        self.start_tool(ctx, move || {
            let curl = compiler::which("curl").ok_or("Hace falta curl para descargar la cita")?;
            let mut command = std::process::Command::new(curl);
            command.args(["-sSL", "--fail", "--max-time", "20"]);
            if negotiate {
                command.args(["-H", "Accept: application/x-bibtex; charset=utf-8"]);
            }
            command.arg(&url);
            let entry = compiler::utility(command)
                .map_err(|e| format!("No pude descargar la cita: {e}"))?;
            if crate::bib::key(&entry).is_none() {
                return Err(
                    "La respuesta no es una entrada BibTeX; revisa el identificador".into(),
                );
            }
            Ok(ToolResult::Citation(crate::bib::tidy(&entry)))
        });
    }
    /// Añade la entrada al `.bib` del proyecto: en el editor si está abierto,
    /// y si no en el disco.
    pub(in crate::app) fn add_citation(&mut self, entry: &str) -> Result<(), String> {
        let key = crate::bib::key(entry).ok_or("La entrada no tiene clave")?;
        let path = self
            .bibliography()
            .ok_or("El proyecto no tiene un archivo .bib")?;
        let exists = latex::citations(&self.completion_sources())
            .iter()
            .any(|t| t.label == key);
        if !exists {
            let join = |text: &str| {
                let gap = if text.is_empty() || text.ends_with("\n\n") {
                    ""
                } else if text.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                format!("{gap}{entry}\n")
            };
            if let Some(doc) = self
                .documents
                .iter_mut()
                .find(|d| d.editor.path.as_ref() == Some(&path))
            {
                let (cursor, end) = (doc.editor.cursor, doc.editor.end());
                let addition = join(doc.editor.source());
                doc.editor.replace(end, end, &addition);
                doc.editor.goto(cursor.row, cursor.col);
            } else {
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let new = format!("{text}{}", join(&text));
                latex::checkpoint(&path, &text)
                    .and_then(|()| config::atomic_write(&path, new.as_bytes()))
                    .map_err(|e| format!("No pude escribir {}: {e}", path.display()))?;
            }
        }
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if self.citation.insert
            && self.editor().format == Format::Latex
            && self.editor().path.as_ref() != Some(&path)
        {
            self.insert_snippet(&format!("\\cite{{{key}}}"));
        }
        self.refresh_sources();
        self.citation.open = false;
        self.citation.identifier.clear();
        self.message = if exists {
            format!("«{key}» ya estaba en la bibliografía")
        } else {
            format!("Añadida «{key}» a {file}")
        };
        Ok(())
    }
}
