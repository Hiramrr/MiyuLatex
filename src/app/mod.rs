use crate::{
    backdrop::{self, Backdrop},
    compiler::{self, CompileResult},
    config::{self, Config},
    editor::{Editor, catalog, code::Indent},
    format::Format,
    latex::{self, Source, Target},
    layout::{self, Layout, Look},
    marks::{self, Marks},
    preview::Preview,
    theme::{self, Theme, col},
};
use eframe::egui::{
    self, Color32, FontId, Id, Key, KeyboardShortcut, Modifiers, RichText, ScrollArea, Stroke,
    TextEdit, TextureHandle, TextureOptions, ViewportCommand,
    text::{CCursor, CCursorRange, LayoutJob},
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

mod workspace;
mod preview_panel;
mod sidebar;
mod toolbar;
mod appearance;

/// Buscar el archivo principal lee disco: se recuerda mientras no cambie
/// aquello del texto de lo que depende.
struct RootCache {
    document: Id,
    revision: u64,
    signature: (Option<String>, bool),
    root: PathBuf,
}
struct Document {
    id: Id,
    editor: Editor,
    layout: Layout,
    pdf: Option<Preview>,
}
/// Clave, celdas del tramado, puntos por celda y tamaño de ventana.
type BackgroundFrame = (String, egui::ColorImage, f32, egui::Vec2);
#[derive(Clone, Copy)]
enum Pending {
    Quit,
    Close(usize),
}
enum ToolResult {
    Message(String),
    Imported(PathBuf),
    Words(String),
    Forward(usize, f32, f32),
    Back(PathBuf, usize),
}
pub struct App {
    config: Config,
    documents: Vec<Document>,
    active: usize,
    next_id: u64,
    project: PathBuf,
    files: Vec<PathBuf>,
    file_query: String,
    outline: bool,
    references: bool,
    reference_query: String,
    project_settings: latex::Project,
    source_cache: Vec<Source>,
    source_outline: Vec<(Target, usize)>,
    /// Archivo principal del documento activo; ver `root`.
    root_cache: RefCell<Option<RootCache>>,
    tool_rx: Option<Receiver<Result<ToolResult, String>>>,
    pdf_marker: Option<(usize, f32, f32)>,
    scroll_pdf_marker: bool,
    save_at: Option<Instant>,
    project_search: bool,
    project_query: String,
    search_results: Vec<Target>,
    history: bool,
    history_file: Option<PathBuf>,
    history_versions: Vec<PathBuf>,
    history_index: usize,
    history_text: Option<String>,
    table: bool,
    table_rows: usize,
    table_columns: usize,
    table_alignment: char,
    word_count: Option<String>,
    theme: Theme,
    backdrop: Backdrop,
    background_texture: Option<TextureHandle>,
    background_key: String,
    /// Puntos por celda y tamaño de ventana con que se generó la textura.
    background_layout: (f32, egui::Vec2),
    background_job: Option<Receiver<BackgroundFrame>>,
    preview: Preview,
    markdown_cache: CommonMarkCache,
    markdown_view: crate::mdview::MarkdownView,
    compile_rx: Option<Receiver<Result<CompileResult, String>>>,
    cancel: Arc<AtomicBool>,
    compile_thread: Option<thread::JoinHandle<()>>,
    result: Option<CompileResult>,
    panel: bool,
    log: bool,
    settings: bool,
    project_options: bool,
    templates: bool,
    symbols: bool,
    symbol_query: String,
    help: bool,
    find: bool,
    focus_find: bool,
    query: String,
    replacement: String,
    goto: bool,
    line: usize,
    pending: Option<Pending>,
    allow_quit: bool,
    focus_editor: bool,
    sync_cursor: bool,
    message: String,
    edited_at: Option<Instant>,
    workspace: workspace::State,
    spell: crate::spell::Speller,
    mascot: crate::mascot::Mascot,
}

pub fn project_files(root: &Path) -> Vec<PathBuf> {
    fn walk(path: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 12 || out.len() >= 3000 {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || ["target", "node_modules", "__pycache__", "build", "dist"]
                    .contains(&name.as_ref())
            {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&entry.path(), depth + 1, out);
            } else if kind.is_file() && Format::listed(&entry.path()) {
                out.push(entry.path());
            }
            if out.len() >= 3000 {
                break;
            }
        }
    }
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    files
}

impl App {
    pub fn new(target: Option<PathBuf>, ctx: &egui::Context) -> Result<Self, String> {
        egui_extras::install_image_loaders(ctx);
        let config = Config::load();
        // Sin argumentos se vuelve a la última sesión; `miyu .` abre la carpeta actual.
        let session = target.is_none()
            && config.restore_session
            && Path::new(&config.session_project).is_dir();
        let target = if session {
            Some(PathBuf::from(&config.session_project))
        } else {
            target
        };
        let target = target.unwrap_or_else(|| {
            let bundled = std::env::current_exe()
                .is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/"));
            if bundled {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        });
        let target = if target.is_absolute() {
            target
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(target)
        };
        let project = if target.is_dir() {
            target.clone()
        } else {
            target.parent().unwrap_or(Path::new(".")).into()
        };
        let project = project.canonicalize().map_err(|e| e.to_string())?;
        let mut backdrop = Backdrop::default();
        let mut message = String::new();
        if !config.background.is_empty()
            && let Err(e) = backdrop.load(&config::clean_path(&config.background))
        {
            message = format!("No pude cargar el fondo: {e}");
        }
        let mut app = Self {
            preview: Preview::new(config.invert_preview),
            config,
            documents: Vec::new(),
            active: 0,
            next_id: 0,
            files: project_files(&project),
            project_settings: latex::Project::load(&project),
            project,
            file_query: String::new(),
            outline: false,
            references: false,
            reference_query: String::new(),
            source_cache: Vec::new(),
            source_outline: Vec::new(),
            root_cache: RefCell::new(None),
            tool_rx: None,
            pdf_marker: None,
            scroll_pdf_marker: false,
            save_at: None,
            project_search: false,
            project_query: String::new(),
            search_results: Vec::new(),
            history: false,
            history_file: None,
            history_versions: Vec::new(),
            history_index: 0,
            history_text: None,
            table: false,
            table_rows: 3,
            table_columns: 3,
            table_alignment: 'l',
            word_count: None,
            theme: theme::builtin().remove(0),
            backdrop,
            background_texture: None,
            background_key: String::new(),
            background_layout: (1.0, egui::Vec2::ZERO),
            background_job: None,
            markdown_cache: CommonMarkCache::default(),
            markdown_view: Default::default(),
            compile_rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            compile_thread: None,
            result: None,
            panel: false,
            log: false,
            settings: false,
            project_options: false,
            templates: false,
            symbols: false,
            symbol_query: String::new(),
            help: false,
            find: false,
            focus_find: false,
            query: String::new(),
            replacement: String::new(),
            goto: false,
            line: 1,
            pending: None,
            allow_quit: false,
            focus_editor: true,
            sync_cursor: true,
            message,
            edited_at: None,
            workspace: workspace::State::default(),
            spell: crate::spell::Speller::default(),
            mascot: Default::default(),
        };
        if session && app.restore_session() {
            // Las pestañas de la última vez ya están abiertas.
        } else if target.is_file() {
            app.open(&target)?;
        } else {
            if let Some(path) = app.project_entry() {
                app.open(&path)?;
            } else {
                app.new_document(0);
            }
        }
        app.apply_theme(ctx);
        Ok(app)
    }
    fn editor(&self) -> &Editor {
        &self.documents[self.active].editor
    }
    fn editor_mut(&mut self) -> &mut Editor {
        &mut self.documents[self.active].editor
    }
    fn source_overlays(&self) -> Vec<Source> {
        self.documents
            .iter()
            .filter_map(|d| {
                d.editor
                    .path
                    .as_ref()
                    .filter(|path| latex::is_source(path))
                    .map(|path| Source {
                        path: path.clone(),
                        text: d.editor.text(),
                    })
            })
            .collect()
    }
    fn root(&self) -> Option<PathBuf> {
        let path = self.editor().path.as_ref()?;
        if !latex::is_source(path) {
            return None;
        }
        if path.starts_with(&self.project)
            && let Some(main) = &self.project_settings.main
        {
            return Some(self.project.join(main));
        }
        // Se consulta en cada cuadro desde la barra de herramientas.
        let editor = self.editor();
        let document = self.documents[self.active].id;
        let mut cache = self.root_cache.borrow_mut();
        let cached = cache.as_ref().filter(|c| c.document == document);
        if let Some(cached) = cached.filter(|c| c.revision == editor.revision) {
            return Some(cached.root.clone());
        }
        let signature = compiler::root_signature(editor.source());
        let root = match cached.filter(|c| c.signature == signature) {
            Some(cached) => cached.root.clone(),
            None => compiler::find_root(path, editor.source()),
        };
        *cache = Some(RootCache {
            document,
            revision: editor.revision,
            signature,
            root: root.clone(),
        });
        Some(root)
    }
    fn refresh_sources(&mut self) {
        // Los archivos del proyecto pueden haber cambiado en disco.
        self.root_cache.take();
        self.source_cache.clear();
        self.source_outline.clear();
        let Some(root) = self.root() else { return };
        self.source_cache = latex::sources(&root, &self.source_overlays());
        for file in &self.files {
            if !self.source_cache.iter().any(|s| s.path == *file) {
                let text = if file.extension().is_some_and(|e| e == "bib") {
                    fs::read_to_string(file).unwrap_or_default()
                } else {
                    String::new()
                };
                self.source_cache.push(Source {
                    path: file.clone(),
                    text,
                });
            }
        }
        for source in &self.source_cache {
            if source.text.is_empty() || !latex::is_source(&source.path) {
                continue;
            }
            let editor = Editor::new(source.text.clone(), Some(source.path.clone()));
            for (row, level, title) in editor.outline().iter().cloned() {
                self.source_outline.push((
                    Target {
                        path: source.path.clone(),
                        row,
                        col: 0,
                        label: title,
                        detail: String::new(),
                    },
                    level,
                ));
            }
        }
    }
    fn completion_sources(&self) -> Vec<Source> {
        let mut sources = self.source_cache.clone();
        for source in self.source_overlays() {
            if let Some(cached) = sources.iter_mut().find(|s| s.path == source.path) {
                cached.text = source.text;
            } else {
                sources.push(source);
            }
        }
        sources
    }
    fn update_completion(&mut self) {
        if !self.editor().wants_completion() {
            self.editor_mut().completions.clear();
            return;
        }
        let sources = if self.editor().format == Format::Latex {
            self.completion_sources()
        } else {
            Vec::new()
        };
        self.editor_mut().update_completion_from(&sources);
    }
    fn project_changed(&mut self) {
        if let Err(e) = self.project_settings.save(&self.project) {
            self.message = e.to_string();
        }
        self.refresh_sources();
        let active = self.active;
        self.activate(active);
    }
    fn jump(&mut self, target: &Target) {
        match self.open(&target.path) {
            Ok(()) => {
                self.editor_mut().goto(target.row, target.col);
                self.sync_cursor = true;
                self.focus_editor = true;
            }
            Err(e) => self.message = e,
        }
    }
    fn start_tool(
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
    fn search_project(&mut self) {
        self.search_results.clear();
        if self.project_query.is_empty() {
            return;
        }
        let pattern = if self.project_query.chars().any(char::is_uppercase) {
            regex::escape(&self.project_query)
        } else {
            format!("(?i){}", regex::escape(&self.project_query))
        };
        let re = regex::Regex::new(&pattern).unwrap();
        for path in &self.files {
            if !Format::detect(path).editable()
                || fs::metadata(path).is_ok_and(|m| m.len() > 2 * 1024 * 1024)
            {
                continue;
            }
            let text = self
                .documents
                .iter()
                .find(|d| d.editor.path.as_ref() == Some(path))
                .map(|d| d.editor.text())
                .or_else(|| fs::read_to_string(path).ok())
                .unwrap_or_default();
            for (row, line) in text.lines().enumerate() {
                if let Some(found) = re.find(line) {
                    self.search_results.push(Target {
                        path: path.clone(),
                        row,
                        col: line[..found.start()].chars().count(),
                        label: line.trim().into(),
                        detail: String::new(),
                    });
                    if self.search_results.len() >= 500 {
                        return;
                    }
                }
            }
        }
    }
    fn show_history(&mut self) {
        let Some(path) = self.editor().path.clone().filter(|p| latex::is_source(p)) else {
            self.message = "Guarda el archivo LaTeX antes de ver su historial".into();
            return;
        };
        self.history_versions = latex::versions(&path);
        self.history_index = 0;
        self.history_text = match self.history_versions.first().map(fs::read_to_string) {
            Some(Ok(text)) => Some(text),
            Some(Err(e)) => {
                self.message = format!("No pude leer la versión: {e}");
                None
            }
            None => None,
        };
        self.history_file = Some(path);
        self.history = true;
    }
    fn sync_to_pdf(&mut self, ctx: &egui::Context) {
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
    fn count_words(&mut self, ctx: &egui::Context) {
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
    fn export_project(&mut self, ctx: &egui::Context) {
        if !self.save_all() {
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Exportar proyecto ZIP")
            .set_file_name(format!(
                "{}.zip",
                self.project
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .add_filter("Proyecto ZIP", &["zip"])
            .save_file()
        else {
            return;
        };
        let destination = if destination.extension().is_none() {
            destination.with_extension("zip")
        } else {
            destination
        };
        let project = self.project.clone();
        self.message = "Exportando el proyecto…".into();
        self.start_tool(ctx, move || {
            latex::export_zip(&project, &destination)
                .map(|_| {
                    ToolResult::Message(format!("Proyecto exportado en {}", destination.display()))
                })
                .map_err(|e| e.to_string())
        });
    }
    fn import_project(&mut self, ctx: &egui::Context) {
        let Some(archive) = rfd::FileDialog::new()
            .set_title("Importar proyecto de Overleaf u otro ZIP")
            .add_filter("Proyecto ZIP", &["zip"])
            .pick_file()
        else {
            return;
        };
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Elegir carpeta donde crear el proyecto importado")
            .pick_folder()
        else {
            return;
        };
        let name = archive.file_stem().unwrap_or_default().to_string_lossy();
        let mut destination = parent.join(name.as_ref());
        let mut suffix = 1;
        while destination.exists() {
            destination = parent.join(format!("{name}-{suffix}"));
            suffix += 1;
        }
        self.message = "Importando el proyecto…".into();
        self.start_tool(ctx, move || {
            latex::import_zip(&archive, &destination)
                .map(|_| ToolResult::Imported(destination))
                .map_err(|e| e.to_string())
        });
    }
    fn export_pdf(&mut self) {
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
    fn add_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .set_title("Añadir archivos al proyecto")
            .pick_files()
        else {
            return;
        };
        for file in files {
            let destination = self.project.join(file.file_name().unwrap_or_default());
            if file == destination {
                continue;
            }
            let result = (|| {
                let mut input = fs::File::open(&file)?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                if let Err(e) = std::io::copy(&mut input, &mut output) {
                    let _ = fs::remove_file(&destination);
                    return Err(e);
                }
                Ok::<_, std::io::Error>(())
            })();
            if let Err(e) = result {
                self.message = format!("No pude añadir {}: {e}", destination.display());
                break;
            }
            self.message = format!("Añadido {}", destination.display());
        }
        self.files = project_files(&self.project);
        self.refresh_sources();
    }
    fn insert_snippet(&mut self, text: &str) {
        let (a, b) = self.editor().selection();
        let selected = self.editor().selected();
        let text = if selected.is_empty() {
            text.to_owned()
        } else {
            text.replacen("$0", &selected, 1)
        };
        self.editor_mut().snippet(&text, a, b);
        self.changed_editor();
    }
    fn insert_figure(&mut self) {
        let Some(root) = self.root() else {
            self.message = "Guarda el documento antes de insertar una figura".into();
            return;
        };
        let Some(file) = rfd::FileDialog::new()
            .set_title("Insertar figura")
            .set_directory(root.parent().unwrap())
            .add_filter("Figuras", &["png", "jpg", "jpeg", "pdf"])
            .pick_file()
        else {
            return;
        };
        let base = root.parent().unwrap();
        let mut relative = file.strip_prefix(base).ok().map(PathBuf::from);
        if relative.as_ref().is_none_or(|p| {
            p.to_string_lossy()
                .contains(['{', '}', '%', '#', '$', '\\'])
        }) {
            let images = base.join("images");
            let result = (|| {
                fs::create_dir_all(&images)?;
                let extension = file.extension().unwrap_or_default().to_string_lossy();
                let mut index = 1;
                loop {
                    let destination = images.join(format!("figura-{index}.{extension}"));
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)
                    {
                        Ok(mut output) => {
                            if let Err(e) = fs::File::open(&file)
                                .and_then(|mut input| std::io::copy(&mut input, &mut output))
                            {
                                let _ = fs::remove_file(&destination);
                                return Err(e);
                            }
                            return Ok(destination);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => index += 1,
                        Err(e) => return Err(e),
                    }
                }
            })();
            match result {
                Ok(path) => relative = path.strip_prefix(base).ok().map(PathBuf::from),
                Err(e) => {
                    self.message = e.to_string();
                    return;
                }
            }
        }
        let path = relative.unwrap().to_string_lossy().replace('\\', "/");
        self.insert_snippet(&format!("\\begin{{figure}}[htbp]\n    \\centering\n    \\includegraphics[width=0.8\\linewidth]{{{path}}}\n    \\caption{{$0}}\n    \\label{{fig:}}\n\\end{{figure}}"));
        let text = self.editor().text();
        if !crate::editor::regex(r"\\usepackage(?:\[[^\]]*\])?\s*\{[^}]*\bgraphicx\b[^}]*\}")
            .is_match(&latex::code(&text))
        {
            self.message =
                "Figura insertada. El documento principal necesita \\usepackage{graphicx}.".into();
        }
        self.files = project_files(&self.project);
        self.refresh_sources();
    }
    fn add_document(&mut self, editor: Editor) {
        self.next_id += 1;
        self.documents.push(Document {
            id: Id::new(("document", self.next_id)),
            editor,
            layout: Layout::default(),
            pdf: None,
        });
        self.activate(self.documents.len() - 1);
    }
    fn activate(&mut self, index: usize) {
        self.active = index;
        self.focus_editor = true;
        self.sync_cursor = true;
        self.edited_at = None;
        self.find = false;
        self.goto = false;
        self.symbols = false;
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
    fn new_document(&mut self, template: usize) {
        self.add_document(Editor::untitled(
            catalog().templates[template].text.clone(),
            &catalog().templates[template].filename,
        ));
        // Las plantillas aún no son archivos guardados.
        self.editor_mut().saved.clear();
        self.message = "Documento nuevo. Elige un nombre al guardar.".into();
    }
    fn open(&mut self, path: &Path) -> Result<(), String> {
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
    fn open_dialog(&mut self) {
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
    fn folder_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Abrir proyecto")
            .set_directory(&self.project)
            .pick_folder()
        {
            self.open_project(path);
        }
    }
    fn save_document(&mut self, index: usize, save_as: bool) -> bool {
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
    fn save_all(&mut self) -> bool {
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
    fn load_pdf(&mut self, path: &Path) {
        if let Err(e) = self.preview.load(path) {
            self.message = e;
        }
    }
    fn compile(&mut self, automatic: bool, ctx: &egui::Context) {
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
    fn poll(&mut self, ctx: &egui::Context) {
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
    fn changed_editor(&mut self) {
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
    fn request_close(&mut self, pending: Pending, ctx: &egui::Context) {
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
    fn finish_close(&mut self, pending: Pending, ctx: &egui::Context) {
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
    fn shortcut(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
        ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        // Los atajos con Mayús van antes: sin ella coinciden también los simples.
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::F) {
            self.project_search = true;
            self.search_project();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::O) {
            self.open_quick();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::J) {
            self.sync_to_pdf(ctx);
        }
        if Self::shortcut(ctx, cmd | Modifiers::SHIFT, Key::N) {
            self.new_project();
        } else if Self::shortcut(ctx, cmd, Key::N) {
            self.templates = true;
        }
        if Self::shortcut(ctx, cmd, Key::O) {
            self.open_dialog();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save_document(self.active, true);
        } else if Self::shortcut(ctx, cmd, Key::S) {
            self.save_document(self.active, false);
        }
        if Self::shortcut(ctx, cmd, Key::W) {
            self.request_close(Pending::Close(self.active), ctx);
        }
        if Self::shortcut(ctx, cmd, Key::Q) {
            self.request_close(Pending::Quit, ctx);
        }
        if Self::shortcut(ctx, cmd, Key::R) || Self::shortcut(ctx, Modifiers::NONE, Key::F5) {
            self.compile(false, ctx);
        }
        if Self::shortcut(ctx, cmd, Key::F) {
            self.start_find();
        }
        if Self::shortcut(ctx, Modifiers::CTRL, Key::Space) && self.config.completions {
            self.editor_mut().request_completion();
            self.update_completion();
        }
        if Self::shortcut(ctx, cmd, Key::G) {
            self.start_goto();
        }
        if Self::shortcut(ctx, cmd, Key::P) || Self::shortcut(ctx, cmd, Key::Comma) {
            self.settings = true;
        }
        if Self::shortcut(ctx, cmd, Key::T) && self.editor().format == Format::Latex {
            self.symbols = true;
        }
        if Self::shortcut(ctx, Modifiers::CTRL, Key::Tab) {
            self.activate((self.active + 1) % self.documents.len());
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F1) {
            self.help = !self.help;
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F2) {
            self.config.show_sidebar = !self.config.show_sidebar;
            self.preferences_changed(ctx);
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F3) {
            self.config.show_preview = !self.config.show_preview;
            self.preferences_changed(ctx);
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F4) {
            self.panel = !self.panel;
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F6) {
            self.open_pdf();
        }
    }
    fn open_pdf(&mut self) {
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
    fn clean_aux(&mut self) -> bool {
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
    fn start_find(&mut self) {
        if !self.editor().format.editable() {
            return;
        }
        let selected = self.editor().selected();
        if !selected.is_empty() && !selected.contains('\n') {
            self.query = selected;
        }
        self.find = true;
        self.focus_find = true;
        self.editor_mut().completions.clear();
    }
    fn start_goto(&mut self) {
        if !self.editor().format.editable() {
            return;
        }
        self.goto = true;
        self.line = self.editor().cursor.row + 1;
    }
    fn editor_keys(&mut self, ctx: &egui::Context) {
        let id = self.documents[self.active].id;
        if !self.editor().format.editable()
            || !ctx.memory(|m| m.has_focus(id))
            || self.pending.is_some()
        {
            return;
        }
        let mut changed = false;
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || Self::shortcut(ctx, Modifiers::COMMAND, Key::Y)
        {
            self.editor_mut().undo(true);
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::Z) {
            self.editor_mut().undo(false);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::B) {
            self.editor_mut().emphasize(true);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::I) {
            self.editor_mut().emphasize(false);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::Slash) {
            self.editor_mut().rewrite_lines(true, false);
            changed = true;
        }
        // Mueven el cursor o la selección sin editar el texto.
        let mut moved = false;
        let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
        if Self::shortcut(ctx, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowDown)
            || Self::shortcut(ctx, shifted, Key::D)
        {
            self.editor_mut().duplicate_lines();
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::D) {
            moved |= self.editor_mut().select_next();
        }
        if Self::shortcut(ctx, Modifiers::ALT, Key::ArrowUp) {
            changed |= self.editor_mut().move_lines(true);
        }
        if Self::shortcut(ctx, Modifiers::ALT, Key::ArrowDown) {
            changed |= self.editor_mut().move_lines(false);
        }
        if Self::shortcut(ctx, shifted, Key::K) {
            self.editor_mut().delete_lines();
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::L) {
            self.editor_mut().select_line();
            moved = true;
        }
        if Self::shortcut(ctx, shifted, Key::Enter) {
            self.editor_mut().open_line(true);
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::Enter) {
            self.editor_mut().open_line(false);
            changed = true;
        }
        if Self::shortcut(ctx, shifted, Key::Backslash)
            || Self::shortcut(ctx, Modifiers::CTRL, Key::M)
        {
            moved |= self.editor_mut().jump_bracket();
        }
        if matches!(self.editor().format, Format::Code(_)) {
            let select = ctx.input(|i| i.modifiers.shift);
            if Self::shortcut(ctx, Modifiers::NONE, Key::Home)
                || (cfg!(target_os = "macos")
                    && Self::shortcut(ctx, Modifiers::MAC_CMD, Key::ArrowLeft))
            {
                self.editor_mut().smart_home(select);
                moved = true;
            }
        }
        if moved {
            self.sync_cursor = true;
        }
        // ponytail: egui procesa los lotes en orden; el emparejado usa eventos individuales.
        let edits_in_frame = ctx.input(|input| {
            input
                .events
                .iter()
                .filter(|event| match event {
                    egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut => true,
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => matches!(
                        key,
                        Key::Enter
                            | Key::Backspace
                            | Key::Delete
                            | Key::Tab
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::Home
                            | Key::End
                    ),
                    _ => false,
                })
                .count()
        });
        if edits_in_frame > 1 {
            if changed {
                self.changed_editor();
            }
            return;
        }
        let mut edits = Vec::new();
        let completion = !self.editor().completions.is_empty();
        let pairs = self.config.auto_pairs;
        let latex = self.editor().format == Format::Latex;
        let code = matches!(self.editor().format, Format::Code(_));
        // Sin selección, copiar y cortar actúan sobre la línea entera.
        let whole_line = self
            .editor()
            .anchor
            .is_none_or(|a| a == self.editor().cursor);
        self.editor_mut().tab = self.config.tab_width;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let special = match event {
                    egui::Event::Copy | egui::Event::Cut => whole_line,
                    egui::Event::Key {
                        key: Key::Tab,
                        pressed: true,
                        modifiers,
                        ..
                    } if *modifiers == Modifiers::SHIFT => true,
                    egui::Event::Text(text) => {
                        pairs
                            && text.chars().count() == 1
                            && text.chars().next().is_some_and(|c| {
                                "{}[]()".contains(c)
                                    || (latex && c == '$')
                                    || (code && "\"'".contains(c))
                            })
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if *modifiers == Modifiers::NONE => {
                        matches!(key, Key::Enter | Key::Tab)
                            || (pairs && *key == Key::Backspace)
                            || (completion
                                && matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Escape))
                    }
                    _ => false,
                };
                if special {
                    edits.push(event.clone());
                }
                !special
            })
        });
        for event in edits {
            match event {
                egui::Event::Text(s) => {
                    self.editor_mut().smart_char(s.chars().next().unwrap());
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Enter, ..
                } => {
                    self.editor_mut().newline();
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Backspace,
                    ..
                } => {
                    self.editor_mut().backspace();
                    changed = true;
                }
                egui::Event::Copy => ctx.copy_text(self.editor().line_text()),
                egui::Event::Cut => {
                    ctx.copy_text(self.editor().line_text());
                    self.editor_mut().delete_lines();
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Tab,
                    modifiers,
                    ..
                } => {
                    if modifiers.shift {
                        self.editor_mut().rewrite_lines(false, true);
                    } else if !self.editor().completions.is_empty() {
                        self.editor_mut().accept_completion();
                    } else if self.editor().selection().0 != self.editor().selection().1 {
                        self.editor_mut().rewrite_lines(false, false);
                    } else {
                        self.editor_mut().indent_cursor();
                    }
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::ArrowUp, ..
                } => {
                    self.editor_mut().completion_index =
                        self.editor().completion_index.saturating_sub(1);
                }
                egui::Event::Key {
                    key: Key::ArrowDown,
                    ..
                } => {
                    self.editor_mut().completion_index = (self.editor().completion_index + 1)
                        .min(self.editor().completions.len().saturating_sub(1));
                }
                egui::Event::Key {
                    key: Key::Escape, ..
                } => self.editor_mut().completions.clear(),
                _ => {}
            }
        }
        if changed {
            if self.config.completions {
                self.update_completion();
            }
            self.changed_editor();
        }
    }
    fn editor_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let mut close = None;
                egui::Frame::new()
                    .fill(col(self.theme.surface))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut activate = None;
                                for (i, doc) in self.documents.iter().enumerate() {
                                    let label = format!(
                                        "{}{}",
                                        doc.editor.title(),
                                        if doc.editor.dirty() { " *" } else { "" }
                                    );
                                    ui.push_id(doc.id, |ui| {
                                        let tab = ui.selectable_label(i == self.active, label)
                                            .on_hover_text(doc.editor.path.as_ref().map_or_else(|| "Documento sin guardar".into(), |p| p.display().to_string()));
                                        if i == self.active {
                                            ui.painter().line_segment([tab.rect.left_bottom(), tab.rect.right_bottom()], Stroke::new(2.0, col(self.theme.primary)));
                                        }
                                        if tab.clicked() { activate = Some(i); }
                                        let name = format!("Cerrar {}", doc.editor.title());
                                        let response = ui.add(egui::Button::new("×").frame_when_inactive(false)).on_hover_text(&name);
                                        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
                                        if response.clicked() { close = Some(i); }
                                    });
                                }
                                if let Some(index) = activate {
                                    self.activate(index);
                                }
                            });
                        });
                    });
                if let Some(i) = close {
                    self.request_close(Pending::Close(i), &ctx);
                }
                if self.editor().format == Format::Pdf {
                    egui::Frame::new()
                        .inner_margin(16.0)
                        .show(ui, |ui| self.pdf_view(ui));
                    return;
                }
                if self.editor().format == Format::Image {
                    let uri = format!("file://{}", self.editor().path.as_ref().unwrap().display());
                    egui::Frame::new().inner_margin(16.0).show(ui, |ui| {
                        ui.label("Imagen · solo lectura");
                        if action(ui, "Recargar imagen", true, "Vuelve a leer esta imagen del disco.").clicked() {
                            ui.ctx().forget_image(&uri);
                        }
                        ui.separator();
                        ScrollArea::both()
                            .id_salt(self.documents[self.active].id.with("image_scroll"))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Image::new(&uri)
                                        .max_size(ui.available_size())
                                        .show_loading_spinner(true),
                                );
                            });
                    });
                    return;
                }
                if self.find {
                    if self.editor().query != self.query {
                        let query = self.query.clone();
                        self.editor_mut().search(&query);
                    }
                    // Mayús indica hacia atrás.
                    let mut step = None;
                    let mut close = ui.input(|i| i.key_pressed(Key::Escape))
                        && self.editor().completions.is_empty();
                    egui::Frame::new()
                        .fill(self.panel_fill())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Buscar");
                                let response = ui.add(
                                    TextEdit::singleline(&mut self.query)
                                        .id_salt("find_query")
                                        .return_key(None)
                                        .desired_width(180.0),
                                );
                                if self.focus_find {
                                    response.request_focus();
                                    self.focus_find = false;
                                    self.focus_editor = false;
                                }
                                if response.changed() {
                                    let query = self.query.clone();
                                    self.editor_mut().search(&query);
                                }
                                // Enter sigue buscando sin salir del campo.
                                if response.has_focus()
                                    && Self::shortcut(&ctx, Modifiers::NONE, Key::Enter)
                                {
                                    step = Some(ui.input(|i| i.modifiers.shift));
                                }
                                let editor = &mut self.documents[self.active].editor;
                                let mut options = false;
                                for (flag, label, help) in [
                                    (&mut editor.search_case, "Aa", "Distinguir mayúsculas"),
                                    (&mut editor.search_word, "ab", "Solo palabras completas"),
                                    (&mut editor.search_regex, ".*", "Expresión regular"),
                                ] {
                                    options |=
                                        ui.toggle_value(flag, label).on_hover_text(help).changed();
                                }
                                if options {
                                    let query = self.query.clone();
                                    self.editor_mut().search(&query);
                                }
                                let found = !self.editor().matches.is_empty();
                                if action(ui, "Anterior", found, "Selecciona la coincidencia anterior. Mayús+Enter. Requiere coincidencias.").clicked() {
                                    step = Some(true);
                                    self.focus_editor = true;
                                }
                                if action(ui, "Siguiente", found, "Selecciona la coincidencia siguiente. Enter. Requiere coincidencias.").clicked() {
                                    step = Some(false);
                                    self.focus_editor = true;
                                }
                                let count = self.editor().matches.len();
                                if count == 0 && !self.query.is_empty() {
                                    ui.label(
                                        RichText::new("Sin coincidencias")
                                            .color(col(self.theme.error)),
                                    );
                                } else if let Some(current) = marks::current_match(self.editor()) {
                                    ui.label(format!("{current} de {count}"));
                                } else {
                                    ui.label(format!("{count} coincidencias"));
                                }
                                close |= action(ui, "Cerrar búsqueda", true, "Cierra la búsqueda y vuelve al editor. Esc.").clicked();
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Reemplazar por");
                                ui.add(
                                    TextEdit::singleline(&mut self.replacement)
                                        .desired_width(180.0),
                                );
                                let selected = self.editor().matches.contains(&self.editor().selection());
                                if action(ui, "Reemplazar coincidencia", selected, "Cambia solo la coincidencia seleccionada y pasa a la siguiente. Selecciona una coincidencia con Anterior o Siguiente.").clicked() {
                                    let replacement = self.replacement.clone();
                                    self.editor_mut().insert(&replacement);
                                    self.changed_editor();
                                    self.message = "Coincidencia reemplazada. Puedes deshacer el cambio.".into();
                                    step = Some(false);
                                    self.focus_editor = true;
                                }
                                if action(ui, "Reemplazar todas", !self.editor().matches.is_empty(), "Reemplaza todas las coincidencias del documento activo. Puedes deshacerlo en un paso. Requiere coincidencias.").clicked() {
                                    let count = self.editor().matches.len();
                                    let replacement = self.replacement.clone();
                                    self.editor_mut().replace_all(&replacement);
                                    self.changed_editor();
                                    self.message = format!("{count} coincidencias reemplazadas. Puedes deshacer el cambio.");
                                    self.focus_editor = true;
                                }
                            });
                        });
                    if let Some(backwards) = step {
                        self.editor_mut().find_next(backwards);
                        self.sync_cursor = true;
                    }
                    if close {
                        self.find = false;
                        self.focus_editor = true;
                    }
                }
                let rect = ui.available_rect_before_wrap();
                self.editor_keys(&ctx);
                let follow_cursor = self.sync_cursor;
                let doc = &mut self.documents[self.active];
                if self.sync_cursor {
                    let mut state =
                        egui::text_edit::TextEditState::load(&ctx, doc.id).unwrap_or_default();
                    let range = CCursorRange::two(
                        CCursor::new(
                            doc.editor
                                .index(doc.editor.anchor.unwrap_or(doc.editor.cursor)),
                        ),
                        CCursor::new(doc.editor.index(doc.editor.cursor)),
                    );
                    state.cursor.set_char_range(Some(range));
                    state.store(&ctx, doc.id);
                    self.sync_cursor = false;
                }
                let format = doc.editor.format.clone();
                let theme = self.theme.clone();
                let size = self.config.font_size as f32;
                let wrap = self.config.soft_wrap;
                let spacing = self.config.line_height as f32;
                let numbers = self.config.line_numbers;
                let highlight_line = self.config.highlight_line;
                let guides = self.config.indent_guides;
                let find = self.find;
                let completions = self.config.completions;
                let shadow = if self.backdrop.image.is_some() {
                    (self.config.text_shadow * 255.0).round() as u8
                } else {
                    0
                };
                let line_height = (spacing > 1.0)
                    .then(|| ui.fonts_mut(|f| f.row_height(&FontId::monospace(size))) * spacing);
                doc.editor.syntax.set_dark(theme.dark);
                if doc.editor.highlight() {
                    // Queda resaltado pendiente para el próximo cuadro.
                    ctx.request_repaint();
                }
                let look = Look {
                    theme: &theme,
                    size,
                    line_height,
                    fonts: layout::fonts_epoch(ui),
                };
                // El editor es el búfer del widget, así que se saca su maquetado.
                let mut text_layout = std::mem::take(&mut doc.layout);
                let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
                    let width = if wrap { width } else { f32::INFINITY };
                    match layout::editor(buffer) {
                        Some(editor) => text_layout.galley(ui, editor, &look, width),
                        None => ui.fonts_mut(|f| {
                            f.layout_job(LayoutJob::simple(
                                buffer.as_str().into(),
                                FontId::monospace(size),
                                col(theme.fg),
                                width,
                            ))
                        }),
                    }
                };
                let mut cursor_position = None;
                let mut changed_document = false;
                let spelling = self.spell.begin(&ctx, &doc.editor, doc.id, &self.config);
                let mut corrected = false;
                ScrollArea::both()
                    .id_salt(doc.id.with("scroll"))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_size(rect.size());
                        ui.horizontal_top(|ui| {
                            let gutter = if numbers {
                                let digits = doc.editor.lines.len().to_string().len().max(3);
                                digits as f32 * size * 0.62 + 16.0
                            } else {
                                8.0
                            };
                            let text_width = (rect.width() - gutter - 16.0).max(100.0);
                            let origin = ui.cursor().min;
                            // Reservado para pintar bajo el texto la sombra y la línea actual.
                            let under_text = ui.painter().add(egui::Shape::Noop);
                            let mut under = Vec::new();
                            ui.add_space(gutter);
                            let output = TextEdit::multiline(&mut doc.editor)
                                .id(doc.id)
                                .font(FontId::monospace(size))
                                .frame(egui::Frame::NONE)
                                .margin(egui::vec2(8.0, 10.0))
                                .desired_width(text_width)
                                .desired_rows(1)
                                // Ocupa todo el alto: un clic bajo el texto lleva el cursor al final.
                                .min_size(egui::vec2(0.0, rect.height()))
                                .code_editor()
                                .event_filter(egui::EventFilter {
                                    tab: true,
                                    horizontal_arrows: true,
                                    vertical_arrows: true,
                                    escape: true,
                                })
                                .layouter(&mut layouter)
                                .show(ui);
                            if self.focus_editor {
                                // Pedir el foco otra vez borra el filtro de Tab y flechas.
                                if !output.response.has_focus() {
                                    output.response.request_focus();
                                    // Sin el foco egui reduce la selección al cursor:
                                    // se vuelve a llevar al widget cuando ya lo tiene.
                                    self.sync_cursor = true;
                                }
                                self.focus_editor = false;
                            }
                            let changed = doc.editor.take_touched();
                            if changed {
                                self.edited_at = (format == Format::Latex
                                    || doc
                                        .editor
                                        .path
                                        .as_ref()
                                        .is_some_and(|p| latex::is_source(p)))
                                .then(Instant::now);
                                self.save_at = self.edited_at;
                                changed_document = true;
                            }
                            if let Some(range) = output.cursor_range {
                                doc.editor
                                    .select(range.primary.index.0, range.secondary.index.0);
                                let cursor_rect = output.galley.pos_from_cursor(range.primary);
                                let cursor_rect =
                                    cursor_rect.translate(output.galley_pos.to_vec2());
                                cursor_position = Some(cursor_rect.left_bottom());
                                if highlight_line && range.is_empty() {
                                    let color = col(theme.highlight()).gamma_multiply(0.6);
                                    under.push(egui::Shape::rect_filled(
                                        egui::Rect::from_x_y_ranges(
                                            ui.clip_rect().x_range(),
                                            cursor_rect.y_range(),
                                        ),
                                        0.0,
                                        color,
                                    ));
                                }
                                if output.response.has_focus() && (changed || follow_cursor) {
                                    ui.scroll_to_rect(cursor_rect, None);
                                }
                            } else if follow_cursor {
                                // Sin el foco (se busca desde la barra) el widget no
                                // informa del cursor: se lleva la vista hasta él.
                                let cursor = CCursor::new(doc.editor.index(doc.editor.cursor));
                                let cursor_rect = output
                                    .galley
                                    .pos_from_cursor(cursor)
                                    .translate(output.galley_pos.to_vec2());
                                ui.scroll_to_rect(cursor_rect, Some(egui::Align::Center));
                            }
                            let column_width =
                                ui.fonts_mut(|f| f.glyph_width(&FontId::monospace(size), ' '));
                            let marks = Marks::new(
                                &doc.editor,
                                &theme,
                                output.response.has_focus(),
                                find,
                                guides,
                                column_width,
                            );
                            let visible = ui.clip_rect().y_range();
                            let scrim = col(theme.bg).gamma_multiply_u8(shadow);
                            let mut shadows = Vec::new();
                            let mut line = 1;
                            let mut starts_line = true;
                            // Columna de la línea con que empieza cada fila visual.
                            let mut column = 0;
                            let mut misspelled = Vec::new();
                            for row in &output.galley.rows {
                                let row_rect = row.rect().translate(output.galley_pos.to_vec2());
                                if visible.intersects(row_rect.y_range()) {
                                    marks.row(
                                        &doc.editor,
                                        line - 1,
                                        column,
                                        &row.glyphs,
                                        row_rect,
                                        starts_line,
                                        &mut under,
                                    );
                                    if spelling && !row.glyphs.is_empty() {
                                        self.spell.underline(
                                            &doc.editor,
                                            line - 1,
                                            column,
                                            &row.glyphs,
                                            row_rect.left(),
                                            row_rect.bottom(),
                                            col(theme.error),
                                            &mut misspelled,
                                        );
                                    }
                                    // La foto queda detrás de una sombra del color del fondo.
                                    let left = if starts_line && numbers {
                                        origin.x
                                    } else {
                                        row_rect.left() - 4.0
                                    };
                                    if shadow > 0 && (!row.glyphs.is_empty() || left == origin.x) {
                                        let right = if row.glyphs.is_empty() {
                                            origin.x + gutter - 2.0
                                        } else {
                                            row_rect.right() + 6.0
                                        };
                                        shadows.push(egui::Shape::rect_filled(
                                            egui::Rect::from_x_y_ranges(
                                                left..=right,
                                                row_rect.y_range(),
                                            ),
                                            0.0,
                                            scrim,
                                        ));
                                    }
                                    if starts_line && numbers {
                                        let mut format = egui::TextFormat::simple(
                                            FontId::monospace(size),
                                            col(theme.muted()),
                                        );
                                        format.line_height = line_height;
                                        let number = ui.painter().layout_job(
                                            LayoutJob::single_section(line.to_string(), format),
                                        );
                                        ui.painter().galley(
                                            egui::pos2(
                                                origin.x + gutter - 6.0 - number.size().x,
                                                row_rect.top(),
                                            ),
                                            number,
                                            col(theme.muted()),
                                        );
                                    }
                                }
                                starts_line = row.ends_with_newline;
                                if starts_line {
                                    line += 1;
                                    column = 0;
                                } else {
                                    column += row.glyphs.len();
                                }
                            }
                            shadows.append(&mut under);
                            ui.painter().set(under_text, egui::Shape::Vec(shadows));
                            ui.painter().extend(misspelled);
                            if spelling
                                && output.response.secondary_clicked()
                                && let Some(pointer) = output.response.interact_pointer_pos()
                            {
                                let cursor =
                                    output.galley.cursor_from_pos(pointer - output.galley_pos);
                                self.spell.target(&doc.editor, cursor.index.0);
                            }
                            if spelling && self.spell.has_menu() {
                                output.response.context_menu(|ui| {
                                    corrected |= self.spell.menu_ui(ui, &mut doc.editor);
                                });
                            }
                        });
                    });
                self.documents[self.active].layout = text_layout;
                if corrected {
                    self.changed_editor();
                }
                if changed_document && completions {
                    self.update_completion();
                }
                if let Some(pos) = cursor_position
                    && !self.editor().completions.is_empty()
                    && ctx.memory(|m| m.has_focus(self.documents[self.active].id))
                {
                    let completions = self.editor().completions.clone();
                    let selected = self.editor().completion_index;
                    let mut chosen = None;
                    egui::Window::new("Sugerencias")
                        .id(Id::new("completions"))
                        .title_bar(false)
                        .resizable(false)
                        .fixed_pos(pos)
                        .default_width(340.0)
                        .show(&ctx, |ui| {
                            ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                for (i, completion) in completions.iter().enumerate() {
                                    // En el código, al lado va qué es cada sugerencia.
                                    let text: egui::WidgetText =
                                        if matches!(completion.kind.as_str(), "word" | "snippet") {
                                            let mut job = LayoutJob::default();
                                            job.append(
                                                &completion.label,
                                                0.0,
                                                egui::TextFormat::simple(
                                                    egui::TextStyle::Button.resolve(ui.style()),
                                                    ui.visuals().text_color(),
                                                ),
                                            );
                                            job.append(
                                                &completion.detail,
                                                12.0,
                                                egui::TextFormat::simple(
                                                    FontId::proportional(12.0),
                                                    col(self.theme.muted()),
                                                ),
                                            );
                                            job.into()
                                        } else {
                                            (&completion.label).into()
                                        };
                                    let response = ui
                                        .selectable_label(i == selected, text)
                                        .on_hover_text(&completion.detail);
                                    if i == selected {
                                        response.scroll_to_me(None);
                                    }
                                    if response.clicked() {
                                        chosen = Some(i);
                                    }
                                }
                            });
                            ui.label(
                                RichText::new("Tab para insertar")
                                    .size(12.0)
                                    .color(col(self.theme.muted())),
                            );
                        });
                    if let Some(i) = chosen {
                        self.editor_mut().completion_index = i;
                        self.editor_mut().accept_completion();
                        self.update_completion();
                        self.changed_editor();
                    }
                }
            });
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
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
        if self.table {
            let mut open = true;
            let mut insert = false;
            egui::Window::new("Insertar tabla")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Filas");
                        ui.add(egui::DragValue::new(&mut self.table_rows).range(1..=100));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Columnas");
                        ui.add(egui::DragValue::new(&mut self.table_columns).range(1..=20));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Alineación");
                        for (alignment, label) in
                            [('l', "Izquierda"), ('c', "Centro"), ('r', "Derecha")]
                        {
                            ui.selectable_value(&mut self.table_alignment, alignment, label);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        insert = action(
                            ui,
                            "Insertar tabla",
                            self.editor().format == Format::Latex,
                            "Inserta la tabla en el documento LaTeX activo.",
                        )
                        .clicked();
                        if action(ui, "Cancelar", true, "Cierra sin insertar una tabla.").clicked()
                        {
                            ui.close_kind(egui::UiKind::Window);
                        }
                    });
                });
            self.table = open;
            if insert && self.editor().format == Format::Latex {
                self.insert_snippet(&latex::table(
                    self.table_rows,
                    self.table_columns,
                    self.table_alignment,
                ));
                self.table = false;
            }
        }
        if self.word_count.is_some() {
            let mut open = true;
            egui::Window::new("Conteo de palabras")
                .open(&mut open)
                .default_width(500.0)
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.label(self.word_count.as_deref().unwrap_or_default());
                });
            if !open {
                self.word_count = None;
            }
        }
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
        if self.templates {
            let mut open = true;
            let mut chosen = None;
            let mut blank = None;
            egui::Window::new("Nuevo documento")
                .open(&mut open)
                .resizable(false)
                .vscroll(true)
                .default_height(560.0)
                .default_width(420.0)
                .show(ctx, |ui| {
                    if action(ui, "Cancelar", true, "Cierra sin crear un documento.").clicked() { ui.close_kind(egui::UiKind::Window); }
                    ui.label("Documento vacío");
                    ui.horizontal_wrapped(|ui| {
                        for (label, name) in [
                            ("LaTeX", "sin-titulo.tex"),
                            ("Bibliografía", "referencias.bib"),
                            ("Markdown", "sin-titulo.md"),
                            ("Texto", "sin-titulo.txt"),
                        ] {
                            if action(ui, label, true, "Abre un documento vacío de este formato. Elige su ubicación al guardar.").clicked() { blank = Some(name); }
                        }
                        for (language, name) in workspace::CODE_FILES {
                            if action(ui, language, true, "Abre un archivo de código vacío de este lenguaje.").clicked() { blank = Some(*name); }
                        }
                    });
                    ui.separator();
                    ui.label("Plantillas LaTeX");
                    for (i, template) in catalog().templates.iter().enumerate() {
                        if ui
                            .button(format!("Crear {}", template.title))
                            .on_hover_text(&template.description)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                        ui.label(
                            RichText::new(&template.description)
                                .size(13.0)
                                .color(col(self.theme.muted())),
                        );
                    }
                    ui.separator();

                });
            self.templates = open;
            if let Some(name) = blank {
                self.add_document(Editor::untitled(String::new(), name));
                self.templates = false;
            }
            if let Some(i) = chosen {
                self.new_document(i);
                self.templates = false;
            }
        }
        if self.symbols {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Símbolos LaTeX")
                .open(&mut open)
                .default_size([420.0, 440.0])
                .show(ctx, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.symbol_query)
                            .hint_text("Buscar símbolo o comando")
                            .desired_width(f32::INFINITY),
                    );
                    if action(
                        ui,
                        "Cerrar símbolos",
                        true,
                        "Cierra sin insertar un símbolo.",
                    )
                    .clicked()
                    {
                        ui.close_kind(egui::UiKind::Window);
                    }
                    let query = self.symbol_query.to_lowercase();
                    ScrollArea::vertical().show(ui, |ui| {
                        for symbol in &catalog().symbols {
                            if !format!("{} {} {}", symbol.name, symbol.latex, symbol.group)
                                .to_lowercase()
                                .contains(&query)
                            {
                                continue;
                            }
                            if ui
                                .add_enabled(
                                    self.editor().format == Format::Latex,
                                    egui::Button::new(format!(
                                        "Insertar {}  {}  {}",
                                        symbol.char, symbol.latex, symbol.name
                                    )),
                                )
                                .on_hover_text(
                                    "Inserta este comando en el cursor del documento LaTeX activo.",
                                )
                                .clicked()
                            {
                                chosen = Some(symbol.latex.clone());
                            }
                        }
                    });
                });
            self.symbols = open;
            if let Some(text) = chosen.filter(|_| self.editor().format == Format::Latex) {
                self.editor_mut().insert(&text);
                self.changed_editor();
                self.symbols = false;
            }
        }
        if self.goto {
            let mut open = true;
            let mut chosen = false;
            egui::Window::new("Ir a línea")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    let count = self.editor().lines.len().max(1);
                    ui.add(egui::DragValue::new(&mut self.line).range(1..=count));
                    ui.label("Número de línea");
                    ui.horizontal_wrapped(|ui| {
                        chosen = action(
                            ui,
                            "Ir a línea",
                            self.editor().format.editable(),
                            "Mueve el cursor a esta línea del documento activo.",
                        )
                        .clicked();
                        if action(ui, "Cancelar", true, "Cierra sin mover el cursor.").clicked() {
                            ui.close_kind(egui::UiKind::Window);
                        }
                    });
                });
            self.goto = open;
            if chosen {
                let line = self.line;
                self.editor_mut().goto(line.saturating_sub(1), 0);
                self.sync_cursor = true;
                self.focus_editor = true;
                self.goto = false;
            }
        }
        if self.help {
            let mut open = true;
            egui::Window::new("Ayuda")
                .open(&mut open)
                .resizable(false)
                .vscroll(true)
                .default_height(560.0)
                .show(ctx, |ui| {
                    ui.label("MiyuLaTeX · LaTeX, Markdown, código, PDF e imágenes");
                    ui.separator();
                    let modifier = if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "Ctrl"
                    };
                    for (key, action) in [
                        ("N", "Nuevo"),
                        ("Shift+N", "Nuevo proyecto"),
                        ("O", "Abrir"),
                        ("S", "Guardar"),
                        ("Shift+S", "Guardar como"),
                        ("R", "Compilar"),
                        ("F", "Buscar y reemplazar"),
                        ("G", "Ir a línea"),
                        ("Shift+O", "Abrir rápido un archivo del proyecto"),
                        ("Shift+F", "Buscar en el proyecto"),
                        ("Shift+J", "Mostrar la línea en el PDF"),
                        ("T", "Insertar símbolo"),
                        ("B", "Negrita"),
                        ("I", "Cursiva"),
                        ("/", "Comentar"),
                        ("D", "Seleccionar la palabra o su siguiente aparición"),
                        ("L", "Seleccionar la línea"),
                        ("Shift+D", "Duplicar la línea"),
                        ("Shift+K", "Borrar la línea"),
                        ("Enter", "Línea nueva debajo (con Shift, encima)"),
                        ("Shift+\\", "Ir al corchete emparejado (o Ctrl+M)"),
                        ("Z", "Deshacer"),
                        ("Shift+Z", "Rehacer"),
                        (",", "Preferencias"),
                        ("W", "Cerrar documento"),
                        ("Q", "Salir"),
                    ] {
                        ui.label(format!("{modifier}+{key}   {action}"));
                    }
                    ui.separator();
                    ui.label("Alt+↑ y Alt+↓ mueven la línea. Tab y Mayús+Tab cambian la sangría.");
                    ui.label("Copiar o cortar sin selección toman la línea entera.");
                    ui.label("F5 compila. Tab acepta una sugerencia.");
                    ui.label("F2, F3 y F4 muestran u ocultan paneles.");
                    ui.label("Markdown tiene vista previa y esquema de títulos.");
                    ui.label("PDF e imágenes se abren en pestañas de solo lectura.");
                    if action(
                        ui,
                        "Cerrar ayuda",
                        true,
                        "Cierra la ayuda y vuelve al documento.",
                    )
                    .clicked()
                    {
                        ui.close_kind(egui::UiKind::Window);
                    }
                });
            self.help = open;
        }
        if let Some(pending) = self.pending {
            let mut choice = 0;
            let (save_label, discard_label) = match pending {
                Pending::Quit => ("Guardar y salir", "Salir sin guardar"),
                Pending::Close(_) => ("Guardar y cerrar", "Cerrar sin guardar"),
            };
            egui::Modal::new(Id::new("unsaved")).show(ctx, |ui| {
                ui.heading("Hay cambios sin guardar");
                match pending {
                    Pending::Close(i) => { ui.label(format!("Se cerrará {}. Cerrar sin guardar pierde sus cambios.", self.documents[i].editor.title())); }
                    Pending::Quit => { ui.label("Se cerrará la aplicación. Salir sin guardar pierde los cambios de los documentos abiertos."); }
                }
                ui.horizontal_wrapped(|ui| {
                    if action(ui, save_label, true, "Guarda los cambios y completa el cierre. Si el guardado falla o se cancela, el documento sigue abierto.").clicked() { choice = 1; }
                    if action(ui, discard_label, true, "Cierra y descarta los cambios sin guardar.").clicked() { choice = 2; }
                    if action(ui, "Cancelar", true, "Cancela el cierre y conserva los documentos abiertos.").clicked() { choice = 3; }
                });
            });
            match choice {
                1 => {
                    let saved = match pending {
                        Pending::Quit => self.save_all(),
                        Pending::Close(i) => self.save_document(i, false),
                    };
                    if saved {
                        self.finish_close(pending, ctx);
                    }
                }
                2 => self.finish_close(pending, ctx),
                3 => self.pending = None,
                _ => {}
            }
        }
    }
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.paint_background(ui, ui.max_rect());
        self.poll(&ctx);
        self.watch_disk(&ctx);
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_quit {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request_close(Pending::Quit, &ctx);
        }
        if self.pending.is_none() {
            self.shortcuts(&ctx);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            {
                let path = file.path().to_path_buf();
                if path.is_dir() {
                    self.open_project(path);
                } else if let Err(e) = self.open(&path) {
                    self.message = e;
                }
            }
        }
        layout::fonts_epoch(ui);
        let title = format!(
            "{}{} · MiyuLaTeX",
            self.editor().title(),
            if self.editor().dirty() { " *" } else { "" }
        );
        // Enviar una orden a la ventana pide otro cuadro: solo si el título cambia.
        let shown = Id::new("window_title");
        if ctx.data(|d| d.get_temp::<String>(shown)).as_ref() != Some(&title) {
            ctx.data_mut(|d| d.insert_temp(shown, title.clone()));
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
        }
        self.toolbar(ui);
        let status = egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.message).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.editor().format.label())
                            .size(13.0)
                            .color(col(self.theme.muted())),
                    );
                    if !self.editor().format.editable() {
                        return;
                    }
                    let cursor = self.editor().cursor;
                    ui.label(
                        RichText::new(format!(
                            "Línea {}, columna {}",
                            cursor.row + 1,
                            cursor.col + 1
                        ))
                        .size(13.0)
                        .color(col(self.theme.muted())),
                    );
                    let mut details = Vec::new();
                    if matches!(self.editor().format, Format::Code(_)) {
                        details.push(match self.editor().indent_style() {
                            Indent::Tabs => "Tabuladores".to_string(),
                            Indent::Spaces(width) => format!("Espacios: {width}"),
                        });
                    }
                    details.extend(marks::selection_summary(self.editor()));
                    for detail in details {
                        ui.label(
                            RichText::new(detail)
                                .size(13.0)
                                .color(col(self.theme.muted())),
                        );
                    }
                });
            });
        });
        let floor = status.response.rect.top();
        if self.config.show_sidebar {
            self.sidebar(ui);
        }
        if self.config.show_preview {
            match self.editor().format {
                Format::Latex => self.pdf_panel(ui),
                Format::Markdown => self.markdown_panel(ui),
                _ => {}
            }
        }
        if self.panel {
            self.problems(ui);
        }
        self.editor_panel(ui);
        if self.config.mascot {
            let busy = self.compile_rx.is_some();
            let ok = self.result.as_ref().is_some_and(|r| r.ok);
            let shown = [self.config.mascot_friend, self.config.mascot_dog];
            let mut friends = shown;
            let hide = self
                .mascot
                .show(ui, floor, &self.theme, busy, ok, &mut friends);
            if hide || friends != shown {
                self.config.mascot = !hide;
                [self.config.mascot_friend, self.config.mascot_dog] = friends;
                self.preferences_changed(&ctx);
            }
        }
        self.dialogs(&ctx);
        self.workspace_dialogs(&ctx);
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
    #[cfg(feature = "screenshot")]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown();
    }
    #[cfg(not(feature = "screenshot"))]
    fn on_exit(&mut self) {
        self.shutdown();
    }
}

impl App {
    fn shutdown(&mut self) {
        self.remember_session();
        let _ = self.config.save();
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.compile_thread.take() {
            let _ = worker.join();
        }
    }
}

fn action(ui: &mut egui::Ui, label: &str, enabled: bool, help: &str) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(label))
        .on_hover_text(help)
        .on_disabled_hover_text(help)
}


/// Panel lateral en el lado elegido en Ver. Si los dos comparten lado, el de
/// archivos queda en el borde de la ventana porque se dibuja primero.
fn side_panel(id: &'static str, right: bool) -> egui::Panel {
    if right {
        egui::Panel::right(id)
    } else {
        egui::Panel::left(id)
    }
}

/// Alto de una fila de las listas de la barra lateral.
fn list_row_height(ui: &egui::Ui) -> f32 {
    let text = ui.text_style_height(&egui::TextStyle::Button);
    (text + 2.0 * ui.spacing().button_padding.y).max(ui.spacing().interact_size.y)
}

pub fn run(target: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png"))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([480.0, 320.0])
            .with_icon(icon),
        // Metal (wgpu) sigue el refresco de la pantalla; con OpenGL los cuadros
        // salían por pares y ProMotion se quedaba en 60 Hz.
        #[cfg(feature = "screenshot")]
        renderer: eframe::Renderer::Glow,
        #[cfg(not(feature = "screenshot"))]
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "MiyuLaTeX",
        options,
        Box::new(move |cc| {
            // Las fuentes se instalan con el tema, en `custom::install_fonts`.
            Ok(Box::new(
                App::new(target, &cc.egui_ctx).map_err(std::io::Error::other)?,
            ))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
