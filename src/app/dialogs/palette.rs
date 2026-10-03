//! Paleta de comandos: las acciones de los menús, con búsqueda por nombre.

use super::*;

#[derive(Default)]
pub(in crate::app) struct Palette {
    pub(in crate::app) open: bool,
    pub(in crate::app) query: String,
    index: usize,
    focus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app) enum Command {
    NewDocument,
    Open,
    OpenQuick,
    Save,
    SaveAs,
    SaveAll,
    History,
    Close,
    ExportPdf,
    NewProject,
    OpenFolder,
    NewFile,
    AddFiles,
    ImportZip,
    ExportZip,
    Undo,
    Redo,
    Find,
    GotoLine,
    SearchProject,
    Comment,
    Definition,
    RenameLabel,
    NextProblem,
    PreviousProblem,
    ToggleSidebar,
    TogglePreview,
    ToggleWrap,
    ToggleProblems,
    ToggleMascot,
    Bold,
    Italic,
    Symbol,
    Table,
    Figure,
    Reference,
    Citation,
    CheckBibliography,
    Compile,
    Stop,
    Rebuild,
    Clean,
    ProjectOptions,
    ToggleAutocompile,
    ToggleAutosave,
    CountWords,
    SyncPdf,
    OpenPdf,
    Settings,
    Help,
}

/// Una acción de la paleta: nombre, atajo, si está disponible y qué ejecuta.
type Entry = (&'static str, &'static str, bool, Command);

/// Minúsculas sin tildes, para encontrar «símbolo» al teclear «simbolo».
fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' => 'a',
            'é' => 'e',
            'í' => 'i',
            'ó' => 'o',
            'ú' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

impl App {
    pub(in crate::app) fn open_palette(&mut self) {
        self.palette = Palette {
            open: true,
            focus: true,
            ..Default::default()
        };
    }
    /// Todas las acciones, en el orden de los menús. `Mod` es Cmd o Ctrl.
    fn commands(&self) -> Vec<Entry> {
        let editor = self.editor();
        let editable = editor.format.editable();
        let latex = editor.format == Format::Latex;
        let prose = latex || editor.format == Format::Markdown;
        let saved_source = self.root().is_some();
        let compiling = self.compile_rx.is_some();
        let tool_ready = self.tool_rx.is_none();
        let has_pdf = self.pdf_path().is_some();
        let has_files = !self.files.is_empty();
        let unsaved = self
            .documents
            .iter()
            .any(|d| d.editor.format.editable() && (d.editor.dirty() || d.editor.path.is_none()));
        let problems = !self.diagnostics.is_empty();
        vec![
            ("Nuevo documento…", "Mod+N", true, Command::NewDocument),
            ("Abrir archivo…", "Mod+O", true, Command::Open),
            (
                "Abrir archivo del proyecto…",
                "Mod+Shift+O",
                has_files,
                Command::OpenQuick,
            ),
            ("Guardar", "Mod+S", editable, Command::Save),
            ("Guardar como…", "Mod+Shift+S", editable, Command::SaveAs),
            ("Guardar todos", "", unsaved, Command::SaveAll),
            (
                "Historial del archivo LaTeX…",
                "",
                saved_source,
                Command::History,
            ),
            ("Cerrar documento", "Mod+W", true, Command::Close),
            ("Exportar PDF…", "", has_pdf, Command::ExportPdf),
            ("Nuevo proyecto…", "Mod+Shift+N", true, Command::NewProject),
            ("Abrir carpeta de proyecto…", "", true, Command::OpenFolder),
            ("Crear archivo en el proyecto…", "", true, Command::NewFile),
            ("Añadir archivos al proyecto…", "", true, Command::AddFiles),
            ("Importar proyecto ZIP…", "", tool_ready, Command::ImportZip),
            ("Exportar proyecto ZIP…", "", tool_ready, Command::ExportZip),
            (
                "Deshacer",
                "Mod+Z",
                editable && editor.can_undo(),
                Command::Undo,
            ),
            (
                "Rehacer",
                "Mod+Shift+Z",
                editable && editor.can_redo(),
                Command::Redo,
            ),
            ("Buscar y reemplazar…", "Mod+F", editable, Command::Find),
            ("Ir a línea…", "Mod+G", editable, Command::GotoLine),
            (
                "Buscar en el proyecto…",
                "Mod+Shift+F",
                has_files,
                Command::SearchProject,
            ),
            (
                "Comentar o descomentar líneas",
                "Mod+/",
                editable && editor.format.comment().is_some(),
                Command::Comment,
            ),
            ("Ir a la definición", "F12", latex, Command::Definition),
            ("Renombrar etiqueta LaTeX…", "", latex, Command::RenameLabel),
            ("Problema siguiente", "F8", problems, Command::NextProblem),
            (
                "Problema anterior",
                "Shift+F8",
                problems,
                Command::PreviousProblem,
            ),
            (
                "Mostrar u ocultar el panel lateral",
                "F2",
                true,
                Command::ToggleSidebar,
            ),
            (
                "Mostrar u ocultar la vista previa",
                "F3",
                true,
                Command::TogglePreview,
            ),
            (
                "Ajustar líneas al ancho del editor",
                "",
                true,
                Command::ToggleWrap,
            ),
            (
                "Mostrar u ocultar problemas y registro",
                "F4",
                true,
                Command::ToggleProblems,
            ),
            (
                "Mostrar u ocultar el gatito",
                "",
                true,
                Command::ToggleMascot,
            ),
            ("Negrita", "Mod+B", prose, Command::Bold),
            ("Cursiva", "Mod+I", prose, Command::Italic),
            ("Insertar símbolo LaTeX…", "Mod+T", latex, Command::Symbol),
            ("Insertar tabla LaTeX…", "", latex, Command::Table),
            (
                "Insertar figura LaTeX…",
                "",
                latex && saved_source,
                Command::Figure,
            ),
            (
                "Insertar cita o referencia LaTeX…",
                "",
                latex,
                Command::Reference,
            ),
            (
                "Añadir cita por DOI o arXiv…",
                "",
                saved_source,
                Command::Citation,
            ),
            (
                "Revisar bibliografía…",
                "",
                saved_source,
                Command::CheckBibliography,
            ),
            (
                "Compilar",
                "F5",
                !compiling && (latex || saved_source),
                Command::Compile,
            ),
            (
                "Detener compilación",
                "",
                compiling && !self.cancel.load(Ordering::Relaxed),
                Command::Stop,
            ),
            (
                "Recompilar desde cero",
                "",
                !compiling && saved_source,
                Command::Rebuild,
            ),
            (
                "Limpiar archivos auxiliares",
                "",
                !compiling && saved_source,
                Command::Clean,
            ),
            (
                "Configurar proyecto LaTeX…",
                "",
                true,
                Command::ProjectOptions,
            ),
            (
                "Compilar al dejar de escribir",
                "",
                true,
                Command::ToggleAutocompile,
            ),
            (
                "Guardar LaTeX automáticamente",
                "",
                true,
                Command::ToggleAutosave,
            ),
            (
                "Contar palabras del proyecto…",
                "",
                tool_ready && saved_source,
                Command::CountWords,
            ),
            (
                "Mostrar línea del cursor en PDF",
                "Mod+Shift+J",
                tool_ready && saved_source && has_pdf,
                Command::SyncPdf,
            ),
            (
                "Abrir el PDF en el visor del sistema",
                "F6",
                has_pdf,
                Command::OpenPdf,
            ),
            ("Preferencias", "Mod+,", true, Command::Settings),
            ("Ayuda", "F1", true, Command::Help),
        ]
    }
    /// Acciones que coinciden con lo tecleado: primero las disponibles.
    pub(in crate::app) fn palette_matches(&self) -> Vec<Entry> {
        let query = fold(&self.palette.query);
        let mut ranked: Vec<(i32, Entry)> = self
            .commands()
            .into_iter()
            .filter_map(|entry| workspace::score(&query, &fold(entry.0)).map(|s| (s, entry)))
            .collect();
        // Sin texto se conserva el orden de los menús.
        if !query.is_empty() {
            ranked.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        }
        ranked.sort_by_key(|(_, entry)| !entry.2);
        ranked.into_iter().map(|(_, entry)| entry).collect()
    }
    pub(in crate::app) fn run_command(&mut self, command: Command, ctx: &egui::Context) {
        self.focus_editor = true;
        match command {
            Command::NewDocument => self.templates = true,
            Command::Open => self.open_dialog(),
            Command::OpenQuick => self.open_quick(),
            Command::Save => {
                self.save_document(self.active, false);
            }
            Command::SaveAs => {
                self.save_document(self.active, true);
            }
            Command::SaveAll => {
                self.save_all();
            }
            Command::History => self.show_history(),
            Command::Close => self.request_close(Pending::Close(self.active), ctx),
            Command::ExportPdf => self.export_pdf(),
            Command::NewProject => self.new_project(),
            Command::OpenFolder => self.folder_dialog(),
            Command::NewFile => self.new_file(),
            Command::AddFiles => self.add_files(),
            Command::ImportZip => self.import_project(ctx),
            Command::ExportZip => self.export_project(ctx),
            Command::Undo | Command::Redo => {
                self.editor_mut().undo(command == Command::Redo);
                self.changed_editor();
            }
            Command::Find => self.start_find(),
            Command::GotoLine => self.start_goto(),
            Command::SearchProject => {
                self.search.open = true;
                self.search_project();
            }
            Command::Comment => {
                self.editor_mut().rewrite_lines(true, false);
                self.changed_editor();
            }
            Command::Definition => self.goto_definition(self.editor().cursor),
            Command::RenameLabel => self.start_rename_label(),
            Command::NextProblem => self.next_problem(false),
            Command::PreviousProblem => self.next_problem(true),
            Command::ToggleSidebar => {
                self.config.show_sidebar = !self.config.show_sidebar;
                self.preferences_changed(ctx);
            }
            Command::TogglePreview => {
                self.config.show_preview = !self.config.show_preview;
                self.preferences_changed(ctx);
            }
            Command::ToggleWrap => {
                self.config.soft_wrap = !self.config.soft_wrap;
                self.preferences_changed(ctx);
            }
            Command::ToggleProblems => self.panel = !self.panel,
            Command::ToggleMascot => {
                self.config.mascot = !self.config.mascot;
                self.preferences_changed(ctx);
            }
            Command::Bold | Command::Italic => {
                self.editor_mut().emphasize(command == Command::Bold);
                self.changed_editor();
            }
            Command::Symbol => self.symbols = true,
            Command::Table => self.table.open = true,
            Command::Figure => self.insert_figure(),
            Command::Reference => {
                self.references = true;
                self.outline = false;
                self.config.show_sidebar = true;
            }
            Command::Citation => self.open_citation(),
            Command::CheckBibliography => self.check_bibliography(),
            Command::Compile => self.compile(false, ctx),
            Command::Stop => {
                self.cancel.store(true, Ordering::Relaxed);
                self.message = "Deteniendo la compilación…".into();
            }
            Command::Rebuild => {
                if self.clean_aux() {
                    self.compile(false, ctx);
                }
            }
            Command::Clean => {
                self.clean_aux();
            }
            Command::ProjectOptions => self.project_options = true,
            Command::ToggleAutocompile => {
                self.config.autocompile = !self.config.autocompile;
                self.preferences_changed(ctx);
                self.message = format!(
                    "Compilar al dejar de escribir: {}",
                    if self.config.autocompile {
                        "activado"
                    } else {
                        "desactivado"
                    }
                );
            }
            Command::ToggleAutosave => {
                self.config.autosave = !self.config.autosave;
                self.preferences_changed(ctx);
                self.message = format!(
                    "Guardado automático: {}",
                    if self.config.autosave {
                        "activado"
                    } else {
                        "desactivado"
                    }
                );
            }
            Command::CountWords => self.count_words(ctx),
            Command::SyncPdf => self.sync_to_pdf(ctx),
            Command::OpenPdf => self.open_pdf(),
            Command::Settings => self.settings = true,
            Command::Help => self.help = true,
        }
    }
    pub(in crate::app) fn palette_dialog(&mut self, ctx: &egui::Context) {
        if !self.palette.open {
            return;
        }
        let matches = self.palette_matches();
        let mut open = true;
        let mut chosen = None;
        let step = ctx.input_mut(|i| {
            i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown) as isize
                - i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp) as isize
        });
        self.palette.index = self
            .palette
            .index
            .saturating_add_signed(step)
            .min(matches.len().saturating_sub(1));
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            open = false;
        }
        let modifier = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        egui::Window::new("Paleta de comandos")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
            .default_width(520.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    TextEdit::singleline(&mut self.palette.query)
                        .hint_text("Nombre de una acción")
                        .desired_width(f32::INFINITY),
                );
                if std::mem::take(&mut self.palette.focus) {
                    response.request_focus();
                }
                if response.changed() {
                    self.palette.index = 0;
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    chosen = matches.get(self.palette.index).copied();
                }
                ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    if matches.is_empty() {
                        ui.label("Ninguna acción coincide.");
                    }
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                        for (i, entry) in matches.iter().enumerate() {
                            let (label, shortcut, enabled, _) = *entry;
                            let selected = i == self.palette.index;
                            let row = ui.add_enabled(
                                enabled,
                                egui::Button::selectable(selected, label)
                                    .shortcut_text(shortcut.replace("Mod", modifier)),
                            );
                            if selected && step != 0 {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                chosen = Some(*entry);
                            }
                        }
                    });
                });
                ui.label(
                    RichText::new("↑ ↓ para elegir · Enter para ejecutar · Esc para cerrar")
                        .size(12.0),
                );
            });
        self.palette.open = open;
        if let Some((_, _, enabled, command)) = chosen {
            if enabled {
                self.palette.open = false;
                self.run_command(command, ctx);
            } else {
                // Sin el foco en el campo no se podría seguir tecleando.
                self.palette.focus = true;
            }
        }
        if !self.palette.open {
            self.focus_editor |= chosen.is_none();
        }
    }
}
