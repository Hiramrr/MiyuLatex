//! Abrir, guardar, activar y cerrar documentos.

use super::*;

impl App {
    pub(super) fn add_document(&mut self, editor: Editor) {
        self.next_id += 1;
        self.documents.push(Document {
            id: Id::new(("document", self.next_id)),
            editor,
            layout: Layout::default(),
            pdf: None,
            git: None,
            changes: (0, Vec::new()),
        });
        self.activate(self.documents.len() - 1);
    }
    pub(super) fn activate(&mut self, index: usize) {
        self.active = index;
        self.focus_editor = true;
        self.sync_cursor = true;
        self.edited_at = None;
        self.find = false;
        self.goto = false;
        self.symbols = false;
        // La rama o el último commit pueden haber cambiado fuera de la aplicación.
        let doc = &mut self.documents[index];
        doc.git = doc
            .editor
            .path
            .as_deref()
            .filter(|_| doc.editor.format.editable())
            .and_then(crate::git::info);
        doc.changes = (u64::MAX, Vec::new());
        self.refresh_sources();
        if self.editor().format != Format::Latex
            && !self
                .editor()
                .path
                .as_ref()
                .is_some_and(|p| latex::is_source(p))
        {
            return;
        }
        let pdf = self
            .editor()
            .path
            .as_ref()
            .and_then(|_| self.root().map(|root| root.with_extension("pdf")));
        if let Some(path) = pdf.filter(|p| p.is_file()) {
            if self.preview.path.as_ref() != Some(&path) {
                self.load_pdf(&path);
            }
        } else {
            self.preview = Preview::new(self.config.invert_preview);
        }
        self.pdf_marker = None;
    }
    pub(super) fn new_document(&mut self, template: usize) {
        self.add_document(Editor::untitled(
            catalog().templates[template].text.clone(),
            &catalog().templates[template].filename,
        ));
        // Las plantillas aún no son archivos guardados.
        self.editor_mut().saved.clear();
        self.message = "Documento nuevo. Elige un nombre al guardar.".into();
    }
    pub(super) fn open(&mut self, path: &Path) -> Result<(), String> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        if let Some(index) = self
            .documents
            .iter()
            .position(|d| d.editor.path.as_ref() == Some(&path))
        {
            self.activate(index);
            return Ok(());
        }
        let format = Format::detect(&path);
        let mut pdf = None;
        let text = if format == Format::Pdf {
            let mut preview = Preview::new(self.config.invert_preview);
            preview.load(&path)?;
            pdf = Some(preview);
            String::new()
        } else if format == Format::Image {
            image::ImageReader::open(&path)
                .map_err(|e| e.to_string())?
                .with_guessed_format()
                .map_err(|e| e.to_string())?
                .into_dimensions()
                .map_err(|e| format!("No pude abrir la imagen: {e}"))?;
            String::new()
        } else {
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("No pude abrir el archivo como texto UTF-8: {e}"))?;
            if text.contains('\0') {
                return Err(
                    "Este archivo contiene datos binarios. Abre texto UTF-8, PDF o imágenes."
                        .into(),
                );
            }
            text
        };
        self.add_document(Editor::new(text, Some(path.clone())));
        self.documents[self.active].pdf = pdf;
        self.message = format!("Abierto {}", path.display());
        Ok(())
    }
    pub(super) fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Abrir documento")
            .set_directory(&self.project)
            .add_filter(
                "Documentos y código",
                &[
                    "tex", "bib", "sty", "cls", "ltx", "tikz", "md", "markdown", "txt", "rs", "py",
                    "js", "ts", "tsx", "jsx", "html", "css", "c", "cpp", "h", "java", "go",
                    "swift", "sh", "json", "yaml", "yml", "toml", "xml", "csv",
                ],
            )
            .add_filter("PDF", &["pdf"])
            .add_filter("Imágenes", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
            .add_filter("Todos los archivos", &["*"])
            .pick_file()
            && let Err(e) = self.open(&path)
        {
            self.message = e;
        }
    }
    pub(super) fn folder_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Abrir proyecto")
            .set_directory(&self.project)
            .pick_folder()
        {
            self.open_project(path);
        }
    }
    pub(super) fn save_document(&mut self, index: usize, save_as: bool) -> bool {
        let editor = &mut self.documents[index].editor;
        if !editor.format.editable() {
            self.message = "Este archivo se abre en modo de lectura".into();
            return false;
        }
        let destination = if save_as || editor.path.is_none() {
            let filename = editor.title();
            let extension = Path::new(&filename)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("txt");
            let Some(path) = rfd::FileDialog::new()
                .set_title("Guardar documento")
                .set_directory(
                    editor
                        .path
                        .as_ref()
                        .and_then(|p| p.parent())
                        .unwrap_or(&self.project),
                )
                .set_file_name(&filename)
                .add_filter(editor.format.label(), &[extension])
                .add_filter("Todos los archivos", &["*"])
                .save_file()
            else {
                return false;
            };
            Some(if path.extension().is_none() {
                path.with_extension(extension)
            } else {
                path
            })
        } else {
            None
        };
        match editor.save(destination) {
            Ok(()) => {
                self.message = format!("Guardado {}", editor.path.as_ref().unwrap().display());
                // Recorrer el proyecto es caro: solo si el archivo es nuevo.
                if !self.files.contains(editor.path.as_ref().unwrap()) {
                    self.files = project_files(&self.project);
                }
                self.refresh_sources();
                true
            }
            Err(e) => {
                self.message = format!("No pude guardar: {e}");
                false
            }
        }
    }
    pub(super) fn save_all(&mut self) -> bool {
        for i in 0..self.documents.len() {
            if self.documents[i].editor.format.editable()
                && (self.documents[i].editor.dirty() || self.documents[i].editor.path.is_none())
                && !self.save_document(i, false)
            {
                return false;
            }
        }
        true
    }
    pub(super) fn changed_editor(&mut self) {
        self.sync_cursor = true;
        self.focus_editor = true;
        self.edited_at = (self.editor().format == Format::Latex
            || self
                .editor()
                .path
                .as_ref()
                .is_some_and(|p| latex::is_source(p)))
        .then(Instant::now);
        self.save_at = self.edited_at;
    }
    pub(super) fn request_close(&mut self, pending: Pending, ctx: &egui::Context) {
        let dirty = match pending {
            Pending::Quit => self.documents.iter().any(|d| d.editor.dirty()),
            Pending::Close(i) => self.documents[i].editor.dirty(),
        };
        if dirty {
            self.pending = Some(pending);
        } else {
            self.finish_close(pending, ctx);
        }
    }
    pub(super) fn finish_close(&mut self, pending: Pending, ctx: &egui::Context) {
        self.pending = None;
        match pending {
            Pending::Quit => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Pending::Close(i) => {
                self.documents.remove(i);
                self.active = self
                    .active
                    .saturating_sub(usize::from(i < self.active))
                    .min(self.documents.len().saturating_sub(1));
                if self.documents.is_empty() {
                    self.add_document(Editor::new(String::new(), None));
                } else {
                    self.activate(self.active);
                }
                self.focus_editor = true;
            }
        }
    }
}
