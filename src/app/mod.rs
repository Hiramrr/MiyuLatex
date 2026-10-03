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

mod appearance;
mod build;
mod developer;
mod dialogs;
mod documents;
mod editor_panel;
mod export;
mod git_panel;
#[cfg(target_os = "macos")]
mod native_menu;
mod preview_panel;
mod project;
mod shortcuts;
mod sidebar;
mod toolbar;
mod workspace;
mod writing;

use project::project_files;

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
    /// Rama y versión confirmada del archivo, si está en un repositorio.
    git: Option<crate::git::Info>,
    /// Líneas que cambiaron desde el último commit y la revisión del texto
    /// con que se calcularon.
    changes: (u64, Vec<(usize, crate::git::Mark)>),
    /// Meta de palabras de la sesión de escritura.
    goal: Option<writing::Goal>,
    typewriter: writing::Typewriter,
}
/// Clave, celdas del tramado, puntos por celda y tamaño de ventana.
type BackgroundFrame = (String, egui::ColorImage, f32, egui::Vec2);
#[derive(Clone, Copy)]
enum Pending {
    Quit,
    Close(usize),
}
/// Problema de la última compilación situado en una línea de un archivo.
struct Diagnostic {
    path: PathBuf,
    row: usize,
    error: bool,
    message: String,
}
enum ToolResult {
    Message(String),
    Imported(PathBuf),
    Words(String),
    /// Documento, revisión que se formateó y texto formateado.
    Formatted(Id, u64, String),
    /// Entrada BibTeX descargada.
    Citation(String),
    Forward(usize, f32, f32),
    Back(PathBuf, usize),
}
pub struct App {
    #[cfg(target_os = "macos")]
    native_menu: Option<native_menu::NativeMenu>,
    config: Config,
    documents: Vec<Document>,
    active: usize,
    /// Documentos de los dos paneles de la vista dividida y cuál tiene el foco.
    split: Option<[Id; 2]>,
    split_focus: usize,
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
    search: dialogs::ProjectSearch,
    history: Option<dialogs::History>,
    rename_label: Option<dialogs::RenameLabel>,
    citation: dialogs::Citation,
    equation: dialogs::Equation,
    /// Avisos de la última revisión de la bibliografía, con la ventana abierta.
    bib_report: Option<Vec<Target>>,
    table: dialogs::Table,
    palette: dialogs::Palette,
    word_count: Option<String>,
    goal_dialog: Option<dialogs::GoalDialog>,
    /// Modo sin distracciones. No se guarda: solo oculta los paneles al dibujar.
    zen: bool,
    theme: Theme,
    backdrop: Backdrop,
    background_texture: Option<TextureHandle>,
    background_key: String,
    /// Puntos por celda y tamaño de ventana con que se generó la textura.
    background_layout: (f32, egui::Vec2),
    background_job: Option<Receiver<BackgroundFrame>>,
    /// Posición y tamaño de la ventana en el último cuadro, para recordarlos al salir.
    window: Option<egui::Rect>,
    preview: Preview,
    markdown_cache: CommonMarkCache,
    markdown_view: crate::mdview::MarkdownView,
    compile_rx: Option<Receiver<Result<CompileResult, String>>>,
    cancel: Arc<AtomicBool>,
    compile_thread: Option<thread::JoinHandle<()>>,
    result: Option<CompileResult>,
    /// Problemas de `result` con línea, para marcarlos en el editor.
    diagnostics: Vec<Diagnostic>,
    panel: bool,
    log: bool,
    developer: developer::State,
    git: git_panel::State,
    settings: bool,
    project_options: bool,
    templates: bool,
    symbols: bool,
    symbol_query: String,
    help: bool,
    find: bool,
    focus_find: bool,
    focus_pdf_find: bool,
    /// El último Cmd+V pegó texto; ver `editor_keys`.
    pasted_text: bool,
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
            #[cfg(target_os = "macos")]
            native_menu: None,
            preview: Preview::new(config.invert_preview),
            config,
            documents: Vec::new(),
            active: 0,
            split: None,
            split_focus: 0,
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
            search: dialogs::ProjectSearch::default(),
            history: None,
            rename_label: None,
            citation: dialogs::Citation::default(),
            equation: dialogs::Equation::default(),
            bib_report: None,
            table: dialogs::Table::default(),
            palette: dialogs::Palette::default(),
            word_count: None,
            goal_dialog: None,
            zen: false,
            theme: theme::builtin().remove(0),
            backdrop,
            background_texture: None,
            background_key: String::new(),
            background_layout: (1.0, egui::Vec2::ZERO),
            background_job: None,
            window: None,
            markdown_cache: CommonMarkCache::default(),
            markdown_view: Default::default(),
            compile_rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            compile_thread: None,
            result: None,
            diagnostics: Vec::new(),
            panel: false,
            log: false,
            developer: developer::State::default(),
            git: git_panel::State::default(),
            settings: false,
            project_options: false,
            templates: false,
            symbols: false,
            symbol_query: String::new(),
            help: false,
            find: false,
            focus_find: false,
            focus_pdf_find: false,
            pasted_text: false,
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
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // En pantalla completa o minimizada se conserva el tamaño anterior.
        ctx.input(|i| {
            let viewport = i.viewport();
            if viewport.fullscreen != Some(true)
                && viewport.minimized != Some(true)
                && let (Some(outer), Some(inner)) = (viewport.outer_rect, viewport.inner_rect)
            {
                self.window = Some(egui::Rect::from_min_size(outer.min, inner.size()));
            }
        });
        self.paint_background(ui, ui.max_rect());
        self.poll(&ctx);
        self.watch_disk(&ctx);
        self.poll_terminals(&ctx);
        self.poll_git(&ctx);
        self.refresh_tasks();
        self.update_goal(&ctx);
        // La barra de búsqueda se cierra con Esc sin consumirla.
        let searching = self.find;
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_quit {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request_close(Pending::Quit, &ctx);
        }
        #[cfg(target_os = "macos")]
        self.native_menus(&ctx);
        if self.pending.is_none() {
            self.shortcuts(&ctx);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for file in dropped {
            {
                let path = file.path().to_path_buf();
                if path.is_dir() {
                    self.open_project(path);
                } else if self.accepts_image(&path) {
                    self.insert_image(&path);
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
        let blame = self.blame_label(&ctx);
        let status = egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.message).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(self.editor().format.label())
                            .size(13.0)
                            .color(col(self.theme.muted())),
                    );
                    if let Some((text, hover, reached)) = self.goal_status() {
                        let color = if reached {
                            self.theme.success
                        } else {
                            self.theme.muted()
                        };
                        ui.label(RichText::new(text).size(13.0).color(col(color)))
                            .on_hover_text(hover);
                    }
                    if let Some(git) = &self.documents[self.active].git {
                        ui.label(
                            RichText::new(format!("Git: {}", git.branch))
                                .size(13.0)
                                .color(col(self.theme.muted())),
                        )
                        .on_hover_text(if git.base.is_some() {
                            "Rama del repositorio. El margen del editor marca las líneas que cambiaron desde el último commit."
                        } else {
                            "Rama del repositorio. Este archivo todavía no está en ningún commit."
                        });
                    }
                    if let Some((text, hover)) = &blame {
                        ui.add(
                            egui::Label::new(
                                RichText::new(text).size(13.0).color(col(self.theme.muted())),
                            )
                            .truncate(),
                        )
                        .on_hover_text(hover);
                    }
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
                    let extras = self.editor().extras().len();
                    if extras > 0 {
                        details.push(format!("{} cursores", extras + 1));
                    }
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
        if self.panel && !self.zen {
            self.problems(ui);
        }
        if self.config.show_sidebar && !self.zen {
            self.sidebar(ui);
        }
        if self.config.show_preview && !self.zen {
            match self.editor().format {
                Format::Latex => self.pdf_panel(ui),
                Format::Markdown => self.markdown_panel(ui),
                _ => {}
            }
        }
        self.editor_panel(ui);
        if self.config.mascot && !self.zen {
            let busy = self.compile_rx.is_some();
            let ok = self.result.as_ref().is_some_and(|r| r.ok);
            let shown = [
                self.config.mascot_friend,
                self.config.mascot_dog,
                self.config.mascot_jasmine,
            ];
            let mut company = shown;
            let hide = self
                .mascot
                .show(ui, floor, &self.theme, busy, ok, &mut company);
            if hide || company != shown {
                self.config.mascot = !hide;
                [
                    self.config.mascot_friend,
                    self.config.mascot_dog,
                    self.config.mascot_jasmine,
                ] = company;
                self.preferences_changed(&ctx);
            }
        }
        self.dialogs(&ctx);
        self.workspace_dialogs(&ctx);
        // Los diálogos, las sugerencias y los cursores múltiples ya consumieron su Esc.
        if self.zen && !searching && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.toggle_zen();
        }
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
    fn has_native_menu(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native_menu.is_some()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
    fn shutdown(&mut self) {
        self.developer.terminals.clear();
        self.remember_session();
        if let Some(window) = self.window {
            self.config.window_x = window.min.x.into();
            self.config.window_y = window.min.y.into();
            self.config.window_width = window.width().into();
            self.config.window_height = window.height().into();
        }
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
    // El icono vacío evita que eframe sustituya el icono nativo del paquete.
    #[cfg(target_os = "macos")]
    let icon =
        if std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/")) {
            egui::IconData::default()
        } else {
            icon
        };
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([480.0, 320.0])
        .with_icon(icon);
    // La ventana vuelve a donde quedó la última vez.
    let config = Config::load();
    let remembered = [
        config.window_x,
        config.window_y,
        config.window_width,
        config.window_height,
    ]
    .map(|v| v as f32);
    let viewport = if remembered.iter().all(|v| v.is_finite())
        && remembered[2] >= 480.0
        && remembered[3] >= 320.0
    {
        viewport
            .with_position([remembered[0], remembered[1]])
            .with_inner_size([remembered[2], remembered[3]])
    } else {
        viewport
    };
    #[cfg(target_os = "macos")]
    let viewport = viewport
        .with_fullsize_content_view(true)
        .with_title_shown(false)
        .with_titlebar_shown(false);
    let options = eframe::NativeOptions {
        viewport,
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
            let app = App::new(target, &cc.egui_ctx).map_err(std::io::Error::other)?;
            #[cfg(target_os = "macos")]
            let app = {
                let mut app = app;
                app.native_menu = Some(native_menu::NativeMenu::new(&cc.egui_ctx));
                app.native_menus(&cc.egui_ctx);
                app
            };
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
