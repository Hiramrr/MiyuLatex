//! Sesión, proyectos recientes, cambios hechos por otros programas, apertura
//! rápida y gestión de los archivos del proyecto.

use super::{App, list_row_height, project_files};
use crate::{
    editor::{Editor, catalog},
    latex,
    theme::col,
};
use eframe::egui::{self, Id, Key, Modifiers, RichText, ScrollArea, TextEdit};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// Proyectos que recuerda el menú Archivo.
const RECENT: usize = 10;

const CODE_FILES: &[(&str, &str)] = &[
    ("Python", "main.py"),
    ("Rust", "main.rs"),
    ("JavaScript", "main.js"),
    ("TypeScript", "main.ts"),
    ("C", "main.c"),
    ("C++", "main.cpp"),
    ("Go", "main.go"),
];

#[derive(Clone, Copy, PartialEq)]
enum ProjectKind {
    Empty,
    Template,
    Code,
}

struct NewProject {
    parent: PathBuf,
    name: String,
    kind: ProjectKind,
    template: usize,
    code: usize,
    error: String,
    focus: bool,
}

#[derive(Clone, Copy)]
pub enum FileAction {
    Rename,
    Duplicate,
    Reveal,
    CopyPath,
}

#[derive(Default)]
pub struct State {
    /// La ventana estaba en segundo plano en el cuadro anterior.
    unfocused: bool,
    checked: Option<Instant>,
    /// Fecha y tamaño de cada archivo abierto la última vez que se miró.
    stamps: HashMap<PathBuf, (SystemTime, u64)>,
    /// Archivo que cambió en disco con cambios sin guardar aquí, y su texto nuevo.
    conflict: Option<(PathBuf, String)>,
    pub quick: bool,
    quick_query: String,
    quick_index: usize,
    quick_focus: bool,
    rename: Option<(PathBuf, String)>,
    create: Option<String>,
    project: Option<NewProject>,
    focus_name: bool,
    /// Carpetas plegadas en el árbol de archivos.
    collapsed: HashSet<PathBuf>,
    /// Filas del árbol y la huella de lo que las produjo.
    tree: (u64, Vec<TreeRow>),
}

/// Fila del árbol de archivos: una carpeta o un archivo.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    depth: usize,
    name: String,
    path: PathBuf,
    folder: bool,
    /// Carpeta desplegada.
    open: bool,
}

#[derive(Default)]
struct Node {
    folders: BTreeMap<String, Node>,
    files: Vec<(String, PathBuf)>,
}

/// Ordena los archivos del proyecto como un árbol: en cada nivel, primero las
/// carpetas y luego los archivos. Con un filtro se muestran todas las carpetas
/// que contienen coincidencias, desplegadas.
pub fn tree(
    project: &Path,
    files: &[PathBuf],
    collapsed: &HashSet<PathBuf>,
    query: &str,
) -> Vec<TreeRow> {
    fn flatten(
        node: Node,
        base: &Path,
        depth: usize,
        collapsed: Option<&HashSet<PathBuf>>,
        rows: &mut Vec<TreeRow>,
    ) {
        for (name, child) in node.folders {
            let path = base.join(&name);
            let open = collapsed.is_none_or(|collapsed| !collapsed.contains(&path));
            rows.push(TreeRow {
                depth,
                name,
                path: path.clone(),
                folder: true,
                open,
            });
            if open {
                flatten(child, &path, depth + 1, collapsed, rows);
            }
        }
        let mut files = node.files;
        files.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, path) in files {
            rows.push(TreeRow {
                depth,
                name,
                path,
                folder: false,
                open: false,
            });
        }
    }
    let query = query.to_lowercase();
    let mut root = Node::default();
    for file in files {
        let relative = file.strip_prefix(project).unwrap_or(file);
        if !query.is_empty() && !relative.to_string_lossy().to_lowercase().contains(&query) {
            continue;
        }
        let mut names: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let Some(name) = names.pop() else {
            continue;
        };
        let mut node = &mut root;
        for folder in names {
            node = node.folders.entry(folder).or_default();
        }
        node.files.push((name, file.clone()));
    }
    let mut rows = Vec::new();
    flatten(
        root,
        project,
        0,
        query.is_empty().then_some(collapsed),
        &mut rows,
    );
    rows
}

/// Puntuación de `candidate` para lo tecleado en Abrir rápido; más es mejor.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let query = query.to_lowercase();
    let candidate = candidate.to_lowercase();
    let length = candidate.chars().count() as i32;
    if query.is_empty() {
        return Some(-length);
    }
    let name = candidate.rsplit('/').next().unwrap_or(&candidate);
    if name.starts_with(&query) {
        return Some(3000 - length);
    }
    if name.contains(&query) {
        return Some(2000 - length);
    }
    if candidate.contains(&query) {
        return Some(1000 - length);
    }
    // Las letras en orden, aunque no seguidas: «c1» encuentra «capitulo1».
    let mut rest = candidate.chars();
    query
        .chars()
        .all(|c| rest.any(|d| d == c))
        .then_some(-length)
}

/// Nombre de archivo o ruta relativa sin `..` ni raíz.
fn relative(name: &str) -> Option<PathBuf> {
    let path = Path::new(name.trim());
    (!path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_))))
        .then(|| path.to_path_buf())
}

impl App {
    pub(super) fn new_project(&mut self) {
        self.templates = false;
        self.workspace.project = Some(NewProject {
            parent: self.project.parent().unwrap_or(&self.project).to_path_buf(),
            name: String::new(),
            kind: ProjectKind::Empty,
            template: 0,
            code: 0,
            error: String::new(),
            focus: true,
        });
    }

    fn create_project(&mut self, draft: &NewProject) -> Result<(), String> {
        let name = relative(&draft.name)
            .filter(|p| p.components().count() == 1 && !draft.name.contains(['/', '\\', '\0']))
            .ok_or("Escribe un nombre de carpeta, sin rutas ni '..'")?;
        let initial = match draft.kind {
            ProjectKind::Empty => None,
            ProjectKind::Template => {
                let template = catalog()
                    .templates
                    .get(draft.template)
                    .ok_or("Elige una plantilla")?;
                Some((template.filename.as_str(), template.text.as_str()))
            }
            ProjectKind::Code => {
                let (_, filename) = CODE_FILES.get(draft.code).ok_or("Elige un lenguaje")?;
                Some((*filename, ""))
            }
        };
        let path = draft.parent.join(name);
        fs::create_dir(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("Ya existe {}. Elige otro nombre.", path.display())
            } else {
                format!("No pude crear {}: {e}", path.display())
            }
        })?;
        let result = (|| -> std::io::Result<()> {
            if let Some((filename, text)) = initial {
                fs::write(path.join(filename), text)?;
                if draft.kind == ProjectKind::Template {
                    latex::Project {
                        main: Some(filename.into()),
                        ..Default::default()
                    }
                    .save(&path)?;
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            return Err(match fs::remove_dir_all(&path) {
                Ok(()) => format!("No pude crear el proyecto: {e}"),
                Err(cleanup) => format!(
                    "No pude crear el proyecto: {e}. Quedó una carpeta incompleta en {}: {cleanup}",
                    path.display()
                ),
            });
        }
        self.set_project(path);
        if let Some((filename, _)) = initial {
            self.open(&self.project.join(filename))?;
        } else {
            self.add_document(Editor::untitled(String::new(), "sin-titulo.txt"));
        }
        self.config.show_sidebar = true;
        self.outline = false;
        self.references = false;
        self.file_query.clear();
        self.message = format!("Proyecto creado en {}", self.project.display());
        Ok(())
    }

    fn new_project_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.workspace.project.take() else {
            return;
        };
        let mut open = true;
        let mut confirm = false;
        let mut cancel = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        egui::Window::new("Nuevo proyecto")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(460.0)
            .show(ctx, |ui| {
                let label = ui.label("Nombre del proyecto");
                let response = ui
                    .add(
                        TextEdit::singleline(&mut draft.name)
                            .hint_text("mi-proyecto")
                            .desired_width(f32::INFINITY),
                    )
                    .labelled_by(label.id);
                if std::mem::take(&mut draft.focus) {
                    response.request_focus();
                }
                if response.changed() {
                    draft.error.clear();
                }
                confirm = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.label("Ubicación");
                ui.label(draft.parent.display().to_string());
                if ui.button("Elegir ubicación…").clicked()
                    && let Some(parent) = rfd::FileDialog::new()
                        .set_title("Carpeta donde crear el proyecto")
                        .set_directory(&draft.parent)
                        .pick_folder()
                {
                    draft.parent = parent;
                    draft.error.clear();
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut draft.kind, ProjectKind::Empty, "Vacío");
                    ui.radio_value(&mut draft.kind, ProjectKind::Template, "Con plantilla");
                    ui.radio_value(&mut draft.kind, ProjectKind::Code, "Código");
                });
                match draft.kind {
                    ProjectKind::Empty => {
                        ui.label("Se crea una carpeta sin archivos.");
                    }
                    ProjectKind::Template => {
                        egui::ComboBox::from_label("Plantilla LaTeX")
                            .selected_text(&catalog().templates[draft.template].title)
                            .show_ui(ui, |ui| {
                                for (i, template) in catalog().templates.iter().enumerate() {
                                    if ui
                                        .selectable_value(&mut draft.template, i, &template.title)
                                        .clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            });
                        let template = &catalog().templates[draft.template];
                        ui.label(&template.description);
                        ui.label(format!("Archivo inicial: {}", template.filename));
                    }
                    ProjectKind::Code => {
                        egui::ComboBox::from_label("Lenguaje")
                            .selected_text(CODE_FILES[draft.code].0)
                            .show_ui(ui, |ui| {
                                for (i, (language, _)) in CODE_FILES.iter().enumerate() {
                                    if ui.selectable_value(&mut draft.code, i, *language).clicked()
                                    {
                                        ui.close();
                                    }
                                }
                            });
                        ui.label(format!(
                            "Se crea {} vacío, listo para editar.",
                            CODE_FILES[draft.code].1
                        ));
                    }
                }
                ui.separator();
                if !draft.name.trim().is_empty() {
                    ui.label(format!(
                        "Carpeta nueva: {}",
                        draft.parent.join(draft.name.trim()).display()
                    ));
                }
                if !draft.error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &draft.error);
                }
                ui.horizontal(|ui| {
                    confirm |= ui
                        .add_enabled(
                            !draft.name.trim().is_empty(),
                            egui::Button::new("Crear proyecto"),
                        )
                        .clicked();
                    cancel |= ui.button("Cancelar").clicked();
                });
            });
        if !open || cancel {
            return;
        }
        if confirm {
            match self.create_project(&draft) {
                Ok(()) => return,
                Err(e) => {
                    draft.error = e;
                    draft.focus = true;
                }
            }
        }
        self.workspace.project = Some(draft);
    }

    /// Cambia la carpeta del proyecto y la apunta entre los recientes.
    pub(super) fn set_project(&mut self, path: PathBuf) {
        self.project = path.canonicalize().unwrap_or(path);
        self.project_settings = latex::Project::load(&self.project);
        self.files = project_files(&self.project);
        self.refresh_sources();
        let project = self.project.to_string_lossy().into_owned();
        self.config.recent_projects.retain(|p| *p != project);
        self.config.recent_projects.insert(0, project);
        self.config.recent_projects.truncate(RECENT);
        if let Err(e) = self.config.save() {
            self.message = format!("No pude guardar las preferencias: {e}");
        }
    }
    pub(super) fn project_entry(&self) -> Option<PathBuf> {
        self.project_settings
            .main
            .as_ref()
            .map(|p| self.project.join(p))
            .into_iter()
            .chain(
                [
                    "main.tex",
                    "principal.tex",
                    "tesis.tex",
                    "README.md",
                    "readme.md",
                ]
                .iter()
                .map(|name| self.project.join(name)),
            )
            .chain(
                CODE_FILES
                    .iter()
                    .map(|(_, filename)| self.project.join(filename)),
            )
            .find(|p| p.is_file())
    }

    /// Abre el proyecto y su archivo principal, como al arrancar en esa carpeta.
    pub(super) fn open_project(&mut self, path: PathBuf) {
        if !path.is_dir() {
            self.message = format!("La carpeta {} ya no existe", path.display());
            let gone = path.to_string_lossy();
            self.config.recent_projects.retain(|p| *p != gone);
            return;
        }
        self.set_project(path);
        if let Some(main) = self.project_entry()
            && let Err(e) = self.open(&main)
        {
            self.message = e;
        }
    }
    pub(super) fn recent_menu(&mut self, ui: &mut egui::Ui) {
        let mut open = None;
        ui.menu_button("Proyectos recientes", |ui| {
            if self.config.recent_projects.is_empty() {
                ui.label("Todavía no hay proyectos recientes.");
            }
            for project in &self.config.recent_projects {
                let path = Path::new(project);
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if ui
                    .button(name.as_ref())
                    .on_hover_text(project.as_str())
                    .clicked()
                {
                    open = Some(path.to_path_buf());
                    ui.close();
                }
            }
        });
        if let Some(path) = open {
            self.open_project(path);
        }
    }
    /// Vuelve a abrir las pestañas de la última sesión. Devuelve si abrió alguna.
    pub(super) fn restore_session(&mut self) -> bool {
        let files = self.config.session_files.clone();
        let mut opened = false;
        for file in &files {
            let path = Path::new(file);
            opened |= path.is_file() && self.open(path).is_ok();
        }
        let active = Path::new(&self.config.session_active);
        if let Some(index) = self
            .documents
            .iter()
            .position(|d| d.editor.path.as_deref() == Some(active))
        {
            self.activate(index);
        }
        if opened {
            self.message = "Sesión anterior restaurada".into();
        }
        opened
    }
    pub(super) fn remember_session(&mut self) {
        fn text(path: &Path) -> String {
            path.to_string_lossy().into_owned()
        }
        self.config.session_project = text(&self.project);
        self.config.session_files = self
            .documents
            .iter()
            .filter_map(|d| d.editor.path.as_deref().map(text))
            .collect();
        self.config.session_active = self.editor().path.as_deref().map(text).unwrap_or_default();
    }
    /// Detecta lo que otros programas cambiaron en los archivos abiertos. No
    /// despierta la interfaz: mira al volver a la ventana y, como mucho, cada
    /// dos segundos mientras ya se está dibujando.
    pub(super) fn watch_disk(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let returned = focused && self.workspace.unfocused;
        self.workspace.unfocused = !focused;
        let due = self
            .workspace
            .checked
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(2));
        if !(returned || (focused && due)) || self.workspace.conflict.is_some() {
            return;
        }
        self.workspace.checked = Some(Instant::now());
        if returned && self.files.len() < 1000 {
            let files = project_files(&self.project);
            if files != self.files {
                self.files = files;
                self.refresh_sources();
            }
        }
        let mut reloaded = Vec::new();
        for index in 0..self.documents.len() {
            let editor = &self.documents[index].editor;
            let Some(path) = editor.path.clone().filter(|_| editor.format.editable()) else {
                continue;
            };
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            let stamp = (
                meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                meta.len(),
            );
            if self.workspace.stamps.insert(path.clone(), stamp) == Some(stamp) {
                continue;
            }
            let Ok(disk) = fs::read_to_string(&path) else {
                continue;
            };
            if disk == editor.saved {
                continue;
            }
            if editor.dirty() {
                self.workspace.conflict = Some((path, disk));
                break;
            }
            self.documents[index].editor.reload(disk);
            reloaded.push(index);
        }
        if let Some(index) = reloaded.first() {
            self.message = format!(
                "{} cambió en disco y se recargó",
                self.documents[*index].editor.title()
            );
            self.after_reload(reloaded.contains(&self.active));
        }
        if self.workspace.stamps.len() > 256 {
            let open: Vec<_> = self
                .documents
                .iter()
                .filter_map(|d| d.editor.path.clone())
                .collect();
            self.workspace.stamps.retain(|path, _| open.contains(path));
        }
    }
    fn after_reload(&mut self, active: bool) {
        if active {
            self.sync_cursor = true;
        }
        self.refresh_sources();
        // El PDF ya no corresponde al texto.
        if self.root().is_some() {
            self.edited_at = Some(Instant::now());
        }
    }
    pub(super) fn file_action(&mut self, ctx: &egui::Context, action: FileAction, path: PathBuf) {
        match action {
            FileAction::Rename => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                self.workspace.rename = Some((path.clone(), name.into_owned()));
                self.workspace.focus_name = true;
            }
            FileAction::CopyPath => ctx.copy_text(path.to_string_lossy().into_owned()),
            FileAction::Reveal => {
                let mut command;
                #[cfg(target_os = "macos")]
                {
                    command = std::process::Command::new("open");
                    command.arg("-R").arg(&path);
                }
                #[cfg(target_os = "windows")]
                {
                    command = std::process::Command::new("explorer");
                    command.arg(format!("/select,{}", path.display()));
                }
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    command = std::process::Command::new("xdg-open");
                    command.arg(path.parent().unwrap_or(&path));
                }
                if let Err(e) = command.spawn() {
                    self.message = e.to_string();
                }
            }
            FileAction::Duplicate => {
                let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                let extension = path
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                let copy = (1..1000)
                    .map(|n| {
                        let suffix = if n == 1 {
                            " copia".to_string()
                        } else {
                            format!(" copia {n}")
                        };
                        path.with_file_name(format!("{stem}{suffix}{extension}"))
                    })
                    .find(|p| !p.exists());
                match copy.map(|copy| fs::copy(&path, &copy).map(|_| copy)) {
                    Some(Ok(copy)) => {
                        self.message = format!("Duplicado en {}", copy.display());
                        self.files = project_files(&self.project);
                        self.refresh_sources();
                    }
                    Some(Err(e)) => self.message = format!("No pude duplicar: {e}"),
                    None => {}
                }
            }
        }
    }
    pub(super) fn new_file(&mut self) {
        self.workspace.create = Some(String::new());
        self.workspace.focus_name = true;
    }
    /// Árbol de carpetas y archivos del proyecto en la barra lateral.
    pub(super) fn file_tree(&mut self, ui: &mut egui::Ui) {
        // El árbol solo se rehace cuando cambia algo de lo que depende.
        let mut hasher = DefaultHasher::new();
        (
            &self.project,
            &self.files,
            &self.file_query,
            self.workspace.collapsed.len(),
        )
            .hash(&mut hasher);
        for folder in &self.workspace.collapsed {
            // El orden de un conjunto no es estable: se combinan sin él.
            let mut one = DefaultHasher::new();
            folder.hash(&mut one);
            hasher.write_u64(one.finish().rotate_left(7));
        }
        let key = hasher.finish() | 1;
        if self.workspace.tree.0 != key {
            self.workspace.tree = (
                key,
                tree(
                    &self.project,
                    &self.files,
                    &self.workspace.collapsed,
                    &self.file_query,
                ),
            );
        }
        let rows = std::mem::take(&mut self.workspace.tree.1);
        if rows.is_empty() {
            ui.label(if self.files.is_empty() {
                "No hay archivos. Abre una carpeta o crea un documento."
            } else {
                "Ningún archivo coincide con el filtro."
            });
        }
        let active = self.editor().path.clone();
        let theme = self.theme.clone();
        let height = list_row_height(ui);
        let mut toggle = None;
        let mut open = None;
        let mut action = None;
        let mut create = None;
        // Solo se dibujan las filas visibles.
        ScrollArea::vertical().id_salt("files_list").show_rows(
            ui,
            height,
            rows.len(),
            |ui, range| {
                for row in &rows[range] {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), height),
                        egui::Sense::click(),
                    );
                    let selected = !row.folder
                        && active.as_ref().is_some_and(|p| {
                            *p == row.path
                                || (p.file_name() == row.path.file_name()
                                    && row.path.canonicalize().is_ok_and(|f| f == *p))
                        });
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            ui.is_enabled(),
                            selected,
                            &row.name,
                        )
                    });
                    let visuals = ui.style().interact_selectable(&response, selected);
                    let painter = ui.painter();
                    if selected || response.hovered() || response.has_focus() {
                        painter.rect_filled(rect, visuals.corner_radius, visuals.weak_bg_fill);
                    }
                    let middle = rect.center().y;
                    let left = rect.left() + 6.0 + row.depth as f32 * 14.0;
                    let muted = col(theme.muted());
                    if row.folder {
                        // Flecha: hacia abajo si la carpeta está desplegada.
                        let center = egui::pos2(left + 4.0, middle);
                        let arrow = if row.open {
                            [(-4.0, -2.5), (4.0, -2.5), (0.0, 3.0)]
                        } else {
                            [(-2.5, -4.0), (3.0, 0.0), (-2.5, 4.0)]
                        };
                        painter.add(egui::Shape::convex_polygon(
                            arrow
                                .iter()
                                .map(|(x, y)| center + egui::vec2(*x, *y))
                                .collect(),
                            muted,
                            egui::Stroke::NONE,
                        ));
                        let color = col(theme.primary);
                        let body = egui::Rect::from_min_max(
                            egui::pos2(left + 13.0, middle - 4.0),
                            egui::pos2(left + 27.0, middle + 6.0),
                        );
                        let tab = egui::Rect::from_min_max(
                            egui::pos2(left + 13.0, middle - 6.5),
                            egui::pos2(left + 19.5, middle - 3.0),
                        );
                        painter.rect_filled(tab, 1.5, color);
                        if row.open {
                            painter.rect_filled(body, 1.5, color.gamma_multiply(0.35));
                            painter.rect_stroke(
                                body,
                                1.5,
                                egui::Stroke::new(1.2, color),
                                egui::StrokeKind::Inside,
                            );
                        } else {
                            painter.rect_filled(body, 1.5, color);
                        }
                    } else {
                        let extension = row
                            .path
                            .extension()
                            .map(|e| e.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        let color = match extension.as_str() {
                            "tex" | "ltx" | "sty" | "cls" | "tikz" => col(theme.secondary),
                            "bib" => col(theme.warning),
                            "pdf" => col(theme.error),
                            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => col(theme.success),
                            "md" | "markdown" => col(theme.accent),
                            _ => muted,
                        };
                        // Una hoja con la esquina doblada.
                        let sheet = egui::Rect::from_min_max(
                            egui::pos2(left + 1.0, middle - 6.5),
                            egui::pos2(left + 11.0, middle + 6.5),
                        );
                        let fold = 3.5;
                        painter.add(egui::Shape::closed_line(
                            vec![
                                sheet.left_top(),
                                sheet.right_top() - egui::vec2(fold, 0.0),
                                sheet.right_top() + egui::vec2(0.0, fold),
                                sheet.right_bottom(),
                                sheet.left_bottom(),
                            ],
                            egui::Stroke::new(1.2, color),
                        ));
                        for offset in [-1.0, 2.0] {
                            painter.line_segment(
                                [
                                    egui::pos2(sheet.left() + 2.5, middle + offset),
                                    egui::pos2(sheet.right() - 2.5, middle + offset),
                                ],
                                egui::Stroke::new(1.0, color.gamma_multiply(0.7)),
                            );
                        }
                    }
                    // Los archivos no llevan flecha: su icono va bajo el de su carpeta.
                    let text_left = left + if row.folder { 34.0 } else { 19.0 };
                    let galley = egui::WidgetText::from(row.name.as_str()).into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        (rect.right() - 4.0 - text_left).max(0.0),
                        egui::TextStyle::Button,
                    );
                    let elided = galley.elided;
                    ui.painter().galley(
                        egui::pos2(text_left, middle - galley.size().y / 2.0),
                        galley,
                        visuals.text_color(),
                    );
                    if response.clicked() {
                        if row.folder {
                            toggle = Some(row.path.clone());
                        } else {
                            open = Some(row.path.clone());
                        }
                    }
                    response.context_menu(|ui| {
                        if row.folder && ui.button("Nuevo archivo aquí…").clicked() {
                            create = Some(row.path.clone());
                            ui.close();
                        }
                        let choices: &[(&str, FileAction)] = if row.folder {
                            &[
                                ("Mostrar en la carpeta", FileAction::Reveal),
                                ("Copiar la ruta", FileAction::CopyPath),
                            ]
                        } else {
                            &[
                                ("Renombrar…", FileAction::Rename),
                                ("Duplicar", FileAction::Duplicate),
                                ("Mostrar en la carpeta", FileAction::Reveal),
                                ("Copiar la ruta", FileAction::CopyPath),
                            ]
                        };
                        for (label, choice) in choices {
                            if ui.button(*label).clicked() {
                                action = Some((*choice, row.path.clone()));
                                ui.close();
                            }
                        }
                    });
                    if elided {
                        response.on_hover_text(&row.name);
                    }
                }
            },
        );
        self.workspace.tree.1 = rows;
        if let Some(folder) = toggle
            && !self.workspace.collapsed.remove(&folder)
        {
            self.workspace.collapsed.insert(folder);
        }
        if let Some(file) = open
            && let Err(e) = self.open(&file)
        {
            self.message = e;
        }
        if let Some((action, path)) = action {
            self.file_action(ui.ctx(), action, path);
        }
        if let Some(folder) = create {
            let inside = folder.strip_prefix(&self.project).unwrap_or(&folder);
            self.workspace.create = Some(format!("{}/", inside.to_string_lossy()));
            self.workspace.focus_name = true;
        }
    }
    fn rename(&mut self, from: &Path, name: &str) -> Result<(), String> {
        let name = relative(name)
            .filter(|p| p.components().count() == 1)
            .ok_or("Escribe un nombre de archivo, sin carpetas")?;
        let to = from.with_file_name(name);
        if to == from {
            return Ok(());
        }
        if to.exists() {
            return Err(format!("Ya existe {}", to.display()));
        }
        fs::rename(from, &to).map_err(|e| format!("No pude renombrar: {e}"))?;
        let to = to.canonicalize().unwrap_or(to);
        for doc in &mut self.documents {
            if doc.editor.path.as_deref() == Some(from) {
                doc.editor.renamed(to.clone());
            }
        }
        if let Some(main) = &self.project_settings.main
            && self.project.join(main) == from
            && let Ok(main) = to.strip_prefix(&self.project)
        {
            self.project_settings.main = Some(main.to_path_buf());
            if let Err(e) = self.project_settings.save(&self.project) {
                self.message = e.to_string();
            }
        }
        self.files = project_files(&self.project);
        self.refresh_sources();
        self.message = format!("Renombrado a {}", to.display());
        Ok(())
    }
    fn create(&mut self, name: &str) -> Result<(), String> {
        let relative = relative(name).ok_or("Escribe un nombre dentro del proyecto, sin «..»")?;
        let relative = if relative.extension().is_none() {
            relative.with_extension("tex")
        } else {
            relative
        };
        let path = self.project.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("No pude crear {}: {e}", path.display()))?;
        self.files = project_files(&self.project);
        self.open(&path)
    }
    /// Archivos del proyecto que mejor coinciden con lo tecleado.
    fn quick_matches(&self) -> Vec<PathBuf> {
        let mut ranked: Vec<(i32, &PathBuf)> = self
            .files
            .iter()
            .filter_map(|path| {
                let name = path.strip_prefix(&self.project).unwrap_or(path);
                score(&self.workspace.quick_query, &name.to_string_lossy()).map(|s| (s, path))
            })
            .collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        ranked
            .into_iter()
            .take(40)
            .map(|(_, p)| p.clone())
            .collect()
    }
    pub(super) fn open_quick(&mut self) {
        self.workspace.quick = true;
        self.workspace.quick_focus = true;
        self.workspace.quick_query.clear();
        self.workspace.quick_index = 0;
    }
    pub(super) fn workspace_dialogs(&mut self, ctx: &egui::Context) {
        self.new_project_dialog(ctx);
        if self.workspace.quick {
            let matches = self.quick_matches();
            let mut open = true;
            let mut chosen = None;
            let step = ctx.input_mut(|i| {
                i.count_and_consume_key(Modifiers::NONE, Key::ArrowDown) as isize
                    - i.count_and_consume_key(Modifiers::NONE, Key::ArrowUp) as isize
            });
            self.workspace.quick_index = self
                .workspace
                .quick_index
                .saturating_add_signed(step)
                .min(matches.len().saturating_sub(1));
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                open = false;
            }
            egui::Window::new("Abrir rápido")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, [0.0, 90.0])
                .default_width(520.0)
                .show(ctx, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.workspace.quick_query)
                            .hint_text("Nombre de un archivo del proyecto")
                            .desired_width(f32::INFINITY),
                    );
                    if std::mem::take(&mut self.workspace.quick_focus) {
                        response.request_focus();
                    }
                    if response.changed() {
                        self.workspace.quick_index = 0;
                    }
                    if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        chosen = matches.get(self.workspace.quick_index).cloned();
                    }
                    ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        if matches.is_empty() {
                            ui.label("Ningún archivo coincide.");
                        }
                        for (i, path) in matches.iter().enumerate() {
                            let name = path.strip_prefix(&self.project).unwrap_or(path);
                            let selected = i == self.workspace.quick_index;
                            let row = ui.selectable_label(selected, name.to_string_lossy());
                            if selected && step != 0 {
                                row.scroll_to_me(None);
                            }
                            if row.clicked() {
                                chosen = Some(path.clone());
                            }
                        }
                    });
                    ui.label(
                        RichText::new("↑ ↓ para elegir · Enter para abrir · Esc para cerrar")
                            .size(12.0),
                    );
                });
            self.workspace.quick = open;
            if let Some(path) = chosen {
                self.workspace.quick = false;
                if let Err(e) = self.open(&path) {
                    self.message = e;
                }
            }
        }
        if let Some((path, mut name)) = self.workspace.rename.take() {
            let mut open = true;
            let mut confirm = false;
            egui::Window::new("Renombrar archivo")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let response = ui.add(TextEdit::singleline(&mut name).desired_width(320.0));
                    if std::mem::take(&mut self.workspace.focus_name) {
                        response.request_focus();
                    }
                    confirm = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.label(
                        RichText::new("Los \\input y \\include que lo nombran no se actualizan.")
                            .size(12.0),
                    );
                    confirm |= ui.button("Renombrar").clicked();
                });
            if confirm {
                if let Err(e) = self.rename(&path, &name) {
                    self.message = e;
                    self.workspace.rename = Some((path, name));
                }
            } else if open {
                self.workspace.rename = Some((path, name));
            }
        }
        if let Some(mut name) = self.workspace.create.take() {
            let mut open = true;
            let mut confirm = false;
            egui::Window::new("Nuevo archivo del proyecto")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut name)
                            .hint_text("capitulos/capitulo3.tex")
                            .desired_width(320.0),
                    );
                    if std::mem::take(&mut self.workspace.focus_name) {
                        response.request_focus();
                    }
                    confirm = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    ui.label(
                        RichText::new("Se crea dentro del proyecto. Sin extensión se usa .tex.")
                            .size(12.0),
                    );
                    confirm |= ui.button("Crear").clicked();
                });
            if confirm {
                if let Err(e) = self.create(&name) {
                    self.message = e;
                    self.workspace.create = Some(name);
                }
            } else if open {
                self.workspace.create = Some(name);
            }
        }
        if let Some((path, disk)) = self.workspace.conflict.take() {
            let Some(index) = self
                .documents
                .iter()
                .position(|d| d.editor.path.as_ref() == Some(&path))
            else {
                return;
            };
            let mut action = 0;
            egui::Modal::new(Id::new("disk_conflict")).show(ctx, |ui| {
                ui.heading(format!(
                    "{} cambió en disco",
                    self.documents[index].editor.title()
                ));
                ui.label("Otro programa modificó el archivo y aquí hay cambios sin guardar.");
                ui.horizontal(|ui| {
                    if ui
                        .button("Recargar del disco")
                        .on_hover_text("Tus cambios se pueden recuperar con Deshacer")
                        .clicked()
                    {
                        action = 1;
                    }
                    if ui
                        .button("Conservar mi versión")
                        .on_hover_text("Al guardar se sobrescribe la versión del disco")
                        .clicked()
                    {
                        action = 2;
                    }
                });
            });
            self.workspace.conflict = Some((path, disk));
            if action != 0 {
                self.resolve_conflict(action == 1);
            }
        }
    }
    /// Cierra el aviso de cambio en disco: recarga o conserva el texto del editor.
    fn resolve_conflict(&mut self, reload: bool) {
        let Some((path, disk)) = self.workspace.conflict.take() else {
            return;
        };
        let Some(index) = self
            .documents
            .iter()
            .position(|d| d.editor.path.as_ref() == Some(&path))
        else {
            return;
        };
        if reload {
            self.documents[index].editor.reload(disk);
            self.after_reload(index == self.active);
        } else {
            // Guardar compara con lo último leído del disco.
            self.documents[index].editor.saved = disk;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_open_ranking_and_safe_names() {
        fn rank<'a>(query: &str, names: &[&'a str]) -> Vec<&'a str> {
            let mut found: Vec<_> = names
                .iter()
                .filter_map(|n| score(query, n).map(|s| (s, *n)))
                .collect();
            found.sort_by_key(|found| std::cmp::Reverse(found.0));
            found.into_iter().map(|(_, n)| n).collect()
        }
        let files = [
            "capitulos/capitulo1.tex",
            "capitulos/capitulo2.tex",
            "main.tex",
            "preliminares/portada.tex",
            "referencias.bib",
        ];
        assert_eq!(rank("main", &files), ["main.tex"]);
        assert_eq!(rank("c2", &files), ["capitulos/capitulo2.tex"]);
        assert_eq!(rank("POR", &files), ["preliminares/portada.tex"]);
        // El nombre pesa más que la carpeta.
        assert_eq!(
            rank("cap", &["capitulos/intro.tex", "cap.tex"])[0],
            "cap.tex"
        );
        assert_eq!(rank("", &files).len(), files.len());
        assert!(rank("zzz", &files).is_empty());

        assert_eq!(relative(" cap/tres.tex "), Some("cap/tres.tex".into()));
        assert_eq!(relative("../fuera.tex"), None);
        assert_eq!(relative("/etc/passwd"), None);
        assert_eq!(relative("  "), None);
    }

    #[test]
    fn file_tree_folders_first_and_filter() {
        let project = Path::new("/tesis");
        let files: Vec<PathBuf> = [
            "capitulos/capitulo1.tex",
            "capitulos/capitulo2.tex",
            "configuracion.tex",
            "imagenes/uv/logo.png",
            "main.tex",
            "Referencias.bib",
        ]
        .iter()
        .map(|f| project.join(f))
        .collect();
        let shape = |rows: &[TreeRow]| -> Vec<String> {
            rows.iter()
                .map(|r| {
                    let mark = match (r.folder, r.open) {
                        (true, true) => "v ",
                        (true, false) => "> ",
                        _ => "",
                    };
                    format!("{}{mark}{}", "  ".repeat(r.depth), r.name)
                })
                .collect()
        };
        let mut collapsed = HashSet::new();
        assert_eq!(
            shape(&tree(project, &files, &collapsed, "")),
            [
                "v capitulos",
                "  capitulo1.tex",
                "  capitulo2.tex",
                "v imagenes",
                "  v uv",
                "    logo.png",
                "configuracion.tex",
                "main.tex",
                "Referencias.bib",
            ]
        );
        collapsed.insert(project.join("capitulos"));
        collapsed.insert(project.join("imagenes"));
        let rows = tree(project, &files, &collapsed, "");
        assert_eq!(
            shape(&rows),
            [
                "> capitulos",
                "> imagenes",
                "configuracion.tex",
                "main.tex",
                "Referencias.bib"
            ]
        );
        assert_eq!(rows[0].path, project.join("capitulos"));
        assert_eq!(rows[3].path, project.join("main.tex"));
        // El filtro despliega las carpetas con coincidencias aunque estén plegadas.
        assert_eq!(
            shape(&tree(project, &files, &collapsed, "TULO2")),
            ["v capitulos", "  capitulo2.tex"]
        );
        assert!(tree(project, &files, &collapsed, "nada").is_empty());
    }

    fn draw(app: &mut App, ctx: &egui::Context) {
        input(app, ctx, Vec::new());
    }

    fn input(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 820.0),
            )),
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| app.draw(ui)).textures_delta.clear();
    }

    #[test]
    fn session_disk_changes_and_file_management() {
        let folder = std::env::temp_dir().join(format!("miyu-workspace-{}", std::process::id()));
        fs::create_dir_all(folder.join("cap")).unwrap();
        let folder = folder.canonicalize().unwrap();
        let main = folder.join("main.tex");
        let one = folder.join("cap/uno.tex");
        fs::write(&main, "\\documentclass{article}\n\\input{cap/uno}\n").unwrap();
        fs::write(&one, "Uno\n").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(Some(folder.clone()), &ctx).unwrap();
        app.config.autocompile = false;
        assert_eq!(app.editor().path.as_ref(), Some(&main));
        draw(&mut app, &ctx);

        // Otro programa cambia un archivo sin cambios aquí: se recarga y se puede deshacer.
        fs::write(&main, "\\documentclass{report}\r\n").unwrap();
        app.workspace.checked = None;
        draw(&mut app, &ctx);
        assert_eq!(app.editor().text(), "\\documentclass{report}\n");
        assert!(!app.editor().dirty());
        assert!(app.message.contains("se recargó"), "{}", app.message);
        app.editor_mut().undo(false);
        assert!(app.editor().text().contains("article"));
        assert!(app.editor().dirty());

        // Con cambios sin guardar se pregunta; conservar permite sobrescribir al guardar.
        fs::write(&main, "externo\n").unwrap();
        app.workspace.checked = None;
        draw(&mut app, &ctx);
        assert!(app.workspace.conflict.is_some());
        assert!(app.editor().text().contains("article"));
        assert!(!app.save_document(app.active, false));
        app.resolve_conflict(false);
        assert!(app.save_document(app.active, false), "{}", app.message);
        assert!(fs::read_to_string(&main).unwrap().contains("article"));
        draw(&mut app, &ctx);
        assert!(app.workspace.conflict.is_none());
        app.editor_mut().insert("mío");
        fs::write(&main, "externo\n").unwrap();
        app.workspace.checked = None;
        draw(&mut app, &ctx);
        app.resolve_conflict(true);
        assert_eq!(app.editor().text(), "externo\n");
        assert!(!app.editor().dirty());

        // Archivos nuevos, renombrados y duplicados dentro del proyecto.
        assert!(app.create("../fuera.tex").is_err());
        app.create("cap/dos").unwrap();
        let two = folder.join("cap/dos.tex");
        assert_eq!(app.editor().path.as_ref(), Some(&two));
        assert!(app.create("cap/dos.tex").is_err());
        assert!(app.rename(&two, "uno.tex").is_err());
        assert!(app.rename(&two, "otra/tres.tex").is_err());
        app.rename(&two, "tres.tex").unwrap();
        let three = folder.join("cap/tres.tex");
        assert!(!two.exists());
        assert_eq!(app.editor().path.as_ref(), Some(&three));
        assert!(app.files.contains(&three) && !app.files.contains(&two));
        app.file_action(&ctx, FileAction::Duplicate, three.clone());
        assert!(folder.join("cap/tres copia.tex").is_file());

        app.open_quick();
        app.workspace.quick_query = "uno".into();
        assert_eq!(app.quick_matches(), std::slice::from_ref(&one));
        draw(&mut app, &ctx);

        // La sesión vuelve con el mismo proyecto, pestañas y documento activo.
        app.open(&one).unwrap();
        app.remember_session();
        let mut config = crate::config::Config::default();
        config.session_project = app.config.session_project.clone();
        config.session_files = app.config.session_files.clone();
        config.session_active = app.config.session_active.clone();
        config.save().unwrap();
        let restored = App::new(None, &ctx).unwrap();
        assert_eq!(restored.project, folder);
        let paths = |app: &App| -> Vec<_> {
            app.documents
                .iter()
                .map(|d| d.editor.path.clone())
                .collect()
        };
        assert_eq!(paths(&restored), paths(&app));
        assert_eq!(restored.editor().path.as_ref(), Some(&one));

        // Crear desde el diálogo conserva las pestañas y deja la carpeta vacía.
        app.workspace.quick = false;
        app.editor_mut().insert("Cambios sin guardar");
        let preserved = app.active;
        let text = app.editor().text();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        input(
            &mut app,
            &ctx,
            vec![key(Key::N, Modifiers::COMMAND | Modifiers::SHIFT)],
        );
        assert!(app.workspace.project.is_some());
        assert!(!app.templates);
        let draft = app.workspace.project.as_mut().unwrap();
        draft.parent = folder.clone();
        draft.name = "Vacío ñ".into();
        draw(&mut app, &ctx);
        input(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(app.workspace.project.is_none());
        assert_eq!(app.project, folder.join("Vacío ñ"));
        assert_eq!(fs::read_dir(&app.project).unwrap().count(), 0);
        assert!(app.files.is_empty());
        assert_eq!(app.editor().format, crate::format::Format::Text);
        assert_eq!(app.documents[preserved].editor.text(), text);
        assert!(app.documents[preserved].editor.dirty());
        assert_eq!(app.config.recent_projects[0], app.project.to_string_lossy());

        // Nombres inválidos y destinos ocupados no cambian el proyecto.
        app.new_project();
        let mut draft = app.workspace.project.take().unwrap();
        draft.parent = folder.clone();
        let current = app.project.clone();
        for name in [
            "",
            " ",
            ".",
            "..",
            "../fuera",
            "/tmp/fuera",
            "a/b",
            "a\\b",
            "a\0b",
        ] {
            draft.name = name.into();
            assert!(app.create_project(&draft).is_err(), "{name:?}");
            assert_eq!(app.project, current);
        }
        let occupied = folder.join("existente");
        fs::create_dir(&occupied).unwrap();
        fs::write(occupied.join("conservar.txt"), "Original").unwrap();
        draft.name = "existente".into();
        app.workspace.project = Some(draft);
        app.workspace.project.as_mut().unwrap().focus = true;
        draw(&mut app, &ctx);
        input(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(
            app.workspace
                .project
                .as_ref()
                .unwrap()
                .error
                .contains("Ya existe")
        );
        assert_eq!(app.project, current);
        assert_eq!(
            fs::read_to_string(occupied.join("conservar.txt")).unwrap(),
            "Original"
        );
        input(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(app.workspace.project.is_none());

        // Cada plantilla y lenguaje abre su archivo guardado, también al reabrir.
        app.new_project();
        let mut draft = app.workspace.project.take().unwrap();
        draft.parent = folder.clone();
        draft.kind = ProjectKind::Template;
        for (i, template) in catalog().templates.iter().enumerate() {
            draft.name = format!("plantilla-{i}");
            draft.template = i;
            app.create_project(&draft).unwrap();
            let source = app.project.join(&template.filename);
            assert_eq!(fs::read_to_string(&source).unwrap(), template.text);
            assert_eq!(app.editor().path.as_ref(), Some(&source));
            assert!(!app.editor().dirty());
            assert_eq!(
                latex::Project::load(&app.project).main,
                Some(template.filename.clone().into())
            );
            let reopened = App::new(Some(app.project.clone()), &ctx).unwrap();
            assert_eq!(reopened.editor().path.as_ref(), Some(&source));
            draw(&mut app, &ctx);
        }
        draft.kind = ProjectKind::Code;
        for (i, (_, filename)) in CODE_FILES.iter().enumerate() {
            draft.name = format!("codigo-{i}");
            draft.code = i;
            app.create_project(&draft).unwrap();
            let source = app.project.join(filename);
            assert_eq!(fs::read_to_string(&source).unwrap(), "");
            assert_eq!(app.files, std::slice::from_ref(&source));
            assert_eq!(app.editor().path.as_ref(), Some(&source));
            assert!(!app.editor().dirty());
            let reopened = App::new(Some(app.project.clone()), &ctx).unwrap();
            assert_eq!(reopened.editor().path.as_ref(), Some(&source));
            app.open_project(folder.clone());
            app.open_project(folder.join(&draft.name));
            assert_eq!(app.editor().path.as_ref(), Some(&source));
            draw(&mut app, &ctx);
        }
        app.remember_session();
        app.config.save().unwrap();
        let restored = App::new(None, &ctx).unwrap();
        assert_eq!(restored.project, app.project);
        assert_eq!(restored.editor().path, app.editor().path);
        assert_eq!(app.documents[preserved].editor.text(), text);
        fs::remove_file(crate::config::directory().join("config.json")).unwrap();
        fs::remove_dir_all(folder).unwrap();
    }
}
