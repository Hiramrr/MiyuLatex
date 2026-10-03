//! Compilación, PDF y herramientas en segundo plano.

use super::*;

impl App {
    pub(super) fn compile(&mut self, automatic: bool, ctx: &egui::Context) {
        if self.editor().format != Format::Latex
            && !self
                .editor()
                .path
                .as_ref()
                .is_some_and(|p| latex::is_source(p))
        {
            self.edited_at = None;
            if !automatic {
                self.message = "La compilación está disponible para documentos LaTeX".into();
            }
            return;
        }
        if self.compile_rx.is_some() {
            return;
        }
        if self.editor().path.is_none() && (automatic || !self.save_document(self.active, false)) {
            self.edited_at = None;
            return;
        }
        let Some(root) = self.root() else {
            self.edited_at = None;
            return;
        };
        let sources = latex::sources(&root, &self.source_overlays());
        for i in 0..self.documents.len() {
            if self.documents[i]
                .editor
                .path
                .as_ref()
                .is_some_and(|path| sources.iter().any(|s| s.path == *path))
                && self.documents[i].editor.dirty()
                && !self.save_document(i, false)
            {
                self.edited_at = None;
                return;
            }
        }
        let preference = if self.project_settings.engine.is_empty() {
            &self.config.engine
        } else {
            &self.project_settings.engine
        };
        let (engine, executable) = match compiler::select_engine(&root, preference) {
            Ok(selected) => selected,
            Err(e) => {
                self.message = e;
                self.edited_at = None;
                return;
            }
        };
        self.message = format!(
            "Compilando {} con {engine}…",
            root.file_name().unwrap_or_default().to_string_lossy()
        );
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        self.compile_thread = Some(thread::spawn(move || {
            let result =
                compiler::compile(root, engine, executable, cancel).map_err(|e| e.to_string());
            let _ = tx.send(result);
            ctx.request_repaint();
        }));
        self.compile_rx = Some(rx);
        self.edited_at = None;
    }
    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.tool_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("La operación se interrumpió".into()))
            }
            _ => None,
        }) {
            self.tool_rx = None;
            match result {
                Ok(ToolResult::Message(message)) => self.message = message,
                Ok(ToolResult::Words(count)) => self.word_count = Some(count),
                Ok(ToolResult::Citation(entry)) => {
                    if let Err(e) = self.add_citation(&entry) {
                        self.message = e;
                    }
                }
                Ok(ToolResult::Forward(page, x, y)) => {
                    self.pdf_marker = Some((page, x, y));
                    self.scroll_pdf_marker = true;
                }
                Ok(ToolResult::Back(path, line)) => self.jump(&Target {
                    path,
                    row: line.saturating_sub(1),
                    col: 0,
                    label: String::new(),
                    detail: String::new(),
                }),
                Ok(ToolResult::Imported(path)) => {
                    self.project = path;
                    self.project_settings = latex::Project::load(&self.project);
                    self.files = project_files(&self.project);
                    let main = self
                        .project_settings
                        .main
                        .as_ref()
                        .map(|p| self.project.join(p))
                        .filter(|p| p.is_file())
                        .or_else(|| {
                            self.files
                                .iter()
                                .find(|p| p.file_name().is_some_and(|n| n == "main.tex"))
                                .cloned()
                        })
                        .or_else(|| {
                            self.files
                                .iter()
                                .find(|p| p.extension().is_some_and(|e| e == "tex"))
                                .cloned()
                        });
                    if let Some(main) = main {
                        if let Err(e) = self.open(&main) {
                            self.message = e;
                        }
                    } else {
                        self.refresh_sources();
                    }
                    self.message = format!("Proyecto importado en {}", self.project.display());
                }
                Err(e) => self.message = e,
            }
        }
        if self.config.autosave
            && self
                .save_at
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(2))
        {
            self.save_at = None;
            for i in 0..self.documents.len() {
                if self.documents[i].editor.dirty()
                    && self.documents[i]
                        .editor
                        .path
                        .as_ref()
                        .is_some_and(|p| latex::is_source(p))
                    && !self.save_document(i, false)
                {
                    break;
                }
            }
        }
        if let Some(result) = self.compile_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("El motor se cerró".into())),
            _ => None,
        }) {
            self.compile_rx = None;
            if let Some(worker) = self.compile_thread.take() {
                let _ = worker.join();
            }
            match result {
                Ok(result) => {
                    self.message = format!(
                        "{} · {} · {:.1} s",
                        if result.ok {
                            "PDF actualizado"
                        } else {
                            "La compilación falló"
                        },
                        result.engine,
                        result.duration
                    );
                    if let Some(path) = &result.pdf
                        && self
                            .root()
                            .is_some_and(|root| root.with_extension("pdf") == *path)
                    {
                        self.load_pdf(path);
                    } else if let Some(path) = &result.pdf {
                        for doc in &mut self.documents {
                            if doc.editor.path.as_ref() == Some(path)
                                && let Some(pdf) = &mut doc.pdf
                                && let Err(e) = pdf.load(path)
                            {
                                self.message = e;
                            }
                        }
                    }
                    self.panel = !result.ok || !result.problems.is_empty();
                    self.diagnostics = result
                        .problems
                        .iter()
                        .filter_map(|problem| {
                            let path = Self::problem_path(&result, problem);
                            Some(Diagnostic {
                                path: path.canonicalize().unwrap_or(path),
                                row: problem.line?.saturating_sub(1),
                                error: problem.severity == "error",
                                message: problem.message.clone(),
                            })
                        })
                        .collect();
                    self.result = Some(result);
                }
                Err(e) => self.message = e,
            }
        }
        self.preview.poll(ctx);
        for doc in &mut self.documents {
            if let Some(pdf) = &mut doc.pdf {
                pdf.poll(ctx);
            }
        }
        if self.config.autocompile
            && self.compile_rx.is_none()
            && self
                .edited_at
                .is_some_and(|t| t.elapsed().as_secs_f64() > self.config.autocompile_delay)
        {
            self.compile(true, ctx);
        }
        if self.compile_rx.is_some()
            || self.tool_rx.is_some()
            || self.save_at.is_some()
            || self.preview.loading
            || self.edited_at.is_some()
            || self
                .documents
                .iter()
                .any(|d| d.pdf.as_ref().is_some_and(|p| p.loading))
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    /// Archivo de un problema; el motor lo da relativo al documento principal.
    pub(super) fn problem_path(result: &CompileResult, problem: &compiler::Problem) -> PathBuf {
        match result.root.parent() {
            Some(base) if problem.file.is_relative() => base.join(&problem.file),
            _ => problem.file.clone(),
        }
    }
    /// Lleva el cursor al problema siguiente o anterior de la última
    /// compilación, primero dentro del archivo activo.
    pub(super) fn next_problem(&mut self, backwards: bool) {
        let here = self.editor().path.clone();
        let row = self.editor().cursor.row;
        let mut local: Vec<usize> = self
            .diagnostics
            .iter()
            .filter(|d| Some(&d.path) == here.as_ref())
            .map(|d| d.row)
            .collect();
        local.sort_unstable();
        local.dedup();
        let target = if backwards {
            local.iter().rev().find(|r| **r < row).or(local.last())
        } else {
            local.iter().find(|r| **r > row).or(local.first())
        };
        let (path, row) = match (target, self.diagnostics.first()) {
            (Some(row), _) => (here.unwrap(), *row),
            (None, Some(first)) => (first.path.clone(), first.row),
            (None, None) => {
                self.message = "La última compilación no dejó problemas con línea".into();
                return;
            }
        };
        match self.open(&path) {
            Ok(()) => {
                self.editor_mut().goto(row, 0);
                self.sync_cursor = true;
                self.focus_editor = true;
                self.message = self
                    .diagnostics
                    .iter()
                    .filter(|d| d.path == path && d.row == row)
                    .map(|d| d.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" · ");
            }
            Err(e) => self.message = e,
        }
    }
    pub(super) fn load_pdf(&mut self, path: &Path) {
        if let Err(e) = self.preview.load(path) {
            self.message = e;
        }
    }
    pub(super) fn open_pdf(&mut self) {
        if let Some(path) = self.pdf_path() {
            #[cfg(target_os = "macos")]
            let program = "open";
            #[cfg(target_os = "windows")]
            let program = "explorer";
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let program = "xdg-open";
            if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
                self.message = e.to_string();
            }
        }
    }
    pub(super) fn export_pdf(&mut self) {
        let Some(path) = self.pdf_path().map(Path::to_path_buf) else {
            self.message = "Compila el documento para exportar el PDF".into();
            return;
        };
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Exportar PDF")
            .set_file_name(path.file_name().unwrap_or_default().to_string_lossy())
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let destination = if destination.extension().is_none() {
            destination.with_extension("pdf")
        } else {
            destination
        };
        match fs::read(&path).and_then(|bytes| config::atomic_write(&destination, &bytes)) {
            Ok(()) => self.message = format!("PDF exportado en {}", destination.display()),
            Err(e) => self.message = e.to_string(),
        }
    }
    pub(super) fn clean_aux(&mut self) -> bool {
        if self.compile_rx.is_some() {
            self.message = "Detén la compilación antes de limpiar los archivos auxiliares".into();
            return false;
        }
        let Some(root) = self.root() else {
            self.message =
                "Abre un archivo LaTeX guardado para limpiar sus archivos auxiliares".into();
            return false;
        };
        let mut count = 0;
        for extension in compiler::AUX {
            let path = root.with_extension(extension);
            if path.is_file() {
                if let Err(e) = fs::remove_file(&path) {
                    self.message = format!("No pude eliminar {}: {e}", path.display());
                    return false;
                }
                count += 1;
            }
        }
        self.message = format!("Eliminados {count} archivos auxiliares");
        true
    }
    pub(super) fn sync_to_pdf(&mut self, ctx: &egui::Context) {
        let Some(pdf) = self.pdf().path.clone() else {
            self.message = "Compila el documento antes de sincronizar".into();
            return;
        };
        let Some(source) = self.editor().path.clone().filter(|p| latex::is_source(p)) else {
            return;
        };
        let line = self.editor().cursor.row + 1;
        self.config.show_preview = true;
        self.start_tool(ctx, move || {
            compiler::sync_forward(&pdf, &source, line)
                .map(|(page, x, y)| ToolResult::Forward(page, x, y))
        });
    }
    pub(super) fn start_tool(
        &mut self,
        ctx: &egui::Context,
        job: impl FnOnce() -> Result<ToolResult, String> + Send + 'static,
    ) {
        if self.tool_rx.is_some() {
            self.message = "Espera a que termine la operación en curso".into();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
                .unwrap_or_else(|_| Err("La operación no pudo terminar".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.tool_rx = Some(rx);
    }
    pub(super) fn count_words(&mut self, ctx: &egui::Context) {
        let Some(root) = self.root() else {
            self.message = "Guarda el documento para contar las palabras del proyecto".into();
            return;
        };
        let sources = latex::sources(&root, &self.source_overlays());
        let dirty = self.documents.iter().any(|d| {
            d.editor.dirty()
                && d.editor
                    .path
                    .as_ref()
                    .is_some_and(|p| sources.iter().any(|s| s.path == *p))
        });
        self.start_tool(ctx, move || {
            if !dirty && let Some(tool) = compiler::which("texcount") {
                let mut command = std::process::Command::new(tool);
                command.args(["-merge", "-sum", "-utf8"]).arg(&root).current_dir(root.parent().unwrap());
                compiler::utility(command).map(ToolResult::Words)
            } else {
                Ok(ToolResult::Words(format!("Estimación del texto del proyecto: {} palabras.\n\nExcluye comentarios, código literal, fórmulas y claves de referencias. Incluye títulos y pies de figura.\n\nPara el informe de TeXcount, instala texcount y guarda los cambios antes de contar.", latex::estimated_words(&sources))))
            }
        });
    }
    pub(super) fn show_history(&mut self) {
        let Some(path) = self.editor().path.clone().filter(|p| latex::is_source(p)) else {
            self.message = "Guarda el archivo LaTeX antes de ver su historial".into();
            return;
        };
        let (history, read) = dialogs::History::new(path);
        if let Err(e) = read {
            self.message = e;
        }
        self.history = Some(history);
    }
}
