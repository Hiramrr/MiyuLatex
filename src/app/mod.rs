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
mod editor_panel;
mod build;
mod project;
use project::project_files;
mod documents;
mod shortcuts;
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
