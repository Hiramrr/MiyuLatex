//! Control de versiones: cambios, commit, ramas, historial, stash, diff y
//! blame. Git corre siempre en un hilo; el cuadro solo lee los resultados.

use super::*;
use crate::repo::{self, Target};
use std::sync::mpsc::Sender;

#[derive(Clone)]
pub(super) enum Op {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Commit(String),
    Checkout(String),
    CreateBranch(String),
    StashPush(String),
    StashPop(String),
    Hunk {
        header: String,
        hunk: String,
        reverse: bool,
    },
}

impl Op {
    /// Cambia archivos del proyecto que pueden estar abiertos.
    fn rewrites_files(&self) -> bool {
        matches!(self, Op::Checkout(_) | Op::StashPush(_) | Op::StashPop(_))
    }
    /// Cambia la rama, el último commit o los archivos.
    fn moves_head(&self) -> bool {
        self.rewrites_files() || matches!(self, Op::Commit(_) | Op::CreateBranch(_))
    }
}

enum Event {
    Snapshot(PathBuf, Result<Option<repo::Snapshot>, String>),
    Done(Op, Result<String, String>),
    Diff(u64, Result<repo::Diff, String>),
    Blame(PathBuf, usize, Option<repo::Blame>),
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plain,
    Meta,
    Hunk,
    Added,
    Removed,
}

struct Row {
    kind: Kind,
    text: String,
}

struct Loaded {
    rows: Vec<Row>,
    patch: repo::Patch,
    /// Fila de cada línea `@@`, en el orden de `patch.hunks`.
    hunk_rows: Vec<usize>,
    widest: usize,
}

struct DiffView {
    title: String,
    target: Target,
    request: u64,
    body: Option<Result<Loaded, String>>,
}

#[derive(Default)]
struct BlameState {
    key: Option<(PathBuf, usize)>,
    label: Option<(String, String)>,
    loading: bool,
    /// Línea en que está el cursor y desde cuándo.
    resting: Option<(PathBuf, usize, Instant)>,
}

pub(super) struct State {
    /// La pestaña Git del panel lateral está elegida.
    pub tab: bool,
    stale: bool,
    loading: bool,
    busy: bool,
    last: Option<Instant>,
    folder: Option<PathBuf>,
    snapshot: Option<Result<Option<repo::Snapshot>, String>>,
    notice: Option<Result<String, String>>,
    commit: String,
    branch_name: String,
    stash_message: String,
    confirm: Option<Op>,
    diff: Option<DiffView>,
    next_request: u64,
    blame: BlameState,
    unfocused: bool,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl Default for State {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tab: false,
            stale: true,
            loading: false,
            busy: false,
            last: None,
            folder: None,
            snapshot: None,
            notice: None,
            commit: String::new(),
            branch_name: String::new(),
            stash_message: String::new(),
            confirm: None,
            diff: None,
            next_request: 0,
            blame: BlameState::default(),
            unfocused: false,
            tx,
            rx,
        }
    }
}

fn run_op(root: &Path, op: &Op) -> Result<String, String> {
    match op {
        Op::Stage(paths) => repo::stage(root, paths).map(|_| String::new()),
        Op::Unstage(paths) => repo::unstage(root, paths).map(|_| String::new()),
        Op::Commit(message) => {
            repo::commit(root, message).map(|line| format!("Commit creado. {line}"))
        }
        Op::Checkout(name) => {
            repo::checkout(root, name).map(|_| format!("Ahora estás en la rama {name}."))
        }
        Op::CreateBranch(name) => repo::create_branch(root, name)
            .map(|_| format!("Rama {} creada y activa.", name.trim())),
        Op::StashPush(message) => {
            repo::stash_push(root, message).map(|_| "Cambios guardados en el stash.".to_string())
        }
        Op::StashPop(name) => {
            repo::stash_pop(root, name).map(|_| "Cambios recuperados del stash.".to_string())
        }
        Op::Hunk {
            header,
            hunk,
            reverse,
        } => repo::apply_hunk(root, header, hunk, *reverse).map(|_| String::new()),
    }
}

fn load_diff(diff: repo::Diff) -> Loaded {
    let mut in_patch = false;
    let rows: Vec<Row> = diff
        .text
        .lines()
        .map(|line| {
            in_patch |= line.starts_with("diff --git ");
            // Antes del primer archivo está el mensaje del commit y su resumen.
            let kind = if !in_patch {
                Kind::Plain
            } else if line.starts_with("@@") {
                Kind::Hunk
            } else if line.starts_with("+++")
                || line.starts_with("---")
                || line.starts_with("diff ")
                || line.starts_with('\\')
            {
                Kind::Meta
            } else if line.starts_with('+') {
                Kind::Added
            } else if line.starts_with('-') {
                Kind::Removed
            } else if line.starts_with(' ') || line.is_empty() {
                Kind::Plain
            } else {
                Kind::Meta
            };
            Row {
                kind,
                text: line.replace('\t', "    "),
            }
        })
        .collect();
    let hunk_rows = (0..rows.len())
        .filter(|&i| rows[i].kind == Kind::Hunk)
        .collect();
    let widest = rows
        .iter()
        .map(|r| r.text.chars().count())
        .max()
        .unwrap_or(0)
        .min(400);
    let patch = if diff.truncated {
        repo::Patch::default()
    } else {
        repo::parse_patch(&diff.text)
    };
    Loaded {
        rows,
        patch,
        hunk_rows,
        widest,
    }
}

/// Fila de una lista: marca, texto (clic) y una acción opcional a la derecha.
fn list_row(
    ui: &mut egui::Ui,
    mark: &str,
    color: Color32,
    text: &str,
    hover: &str,
    button: Option<(&str, &str)>,
) -> (bool, bool) {
    let mut clicked = (false, false);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some((label, help)) = button
                && ui.small_button(label).on_hover_text(help).clicked()
            {
                clicked.1 = true;
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                if !mark.is_empty() {
                    ui.label(RichText::new(mark).monospace().color(color));
                }
                let label = egui::Label::new(text)
                    .truncate()
                    .sense(egui::Sense::click());
                if ui
                    .add(label)
                    .on_hover_text(hover)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    clicked.0 = true;
                }
            });
        });
    });
    clicked
}

impl App {
    /// La pestaña Git del panel lateral se está mostrando.
    pub(super) fn git_active(&self) -> bool {
        self.git.tab && !self.outline && !self.references
    }
    pub(super) fn show_git(&mut self, ctx: &egui::Context) {
        self.config.show_sidebar = true;
        self.git.tab = true;
        self.outline = false;
        self.references = false;
        self.git.stale = true;
        self.preferences_changed(ctx);
    }
    /// Algo cambió en disco o en el repositorio: lo leído ya no vale.
    pub(super) fn git_touched(&mut self) {
        self.git.stale = true;
        self.git.blame.key = None;
    }
    fn unsaved_documents(&self) -> usize {
        self.documents
            .iter()
            .filter(|d| d.editor.format.editable() && d.editor.dirty())
            .count()
    }
    fn git_visible(&self) -> bool {
        (self.config.show_sidebar && self.git_active()) || self.git.diff.is_some()
    }

    /// Recoge lo que terminaron los hilos y relee el estado si hace falta.
    pub(super) fn poll_git(&mut self, ctx: &egui::Context) {
        if self.git.folder.as_ref().is_some_and(|f| *f != self.project) {
            // Otro proyecto: nada de lo leído vale.
            let git = &mut self.git;
            git.folder = None;
            git.snapshot = None;
            git.notice = None;
            git.confirm = None;
            git.diff = None;
            git.blame = BlameState::default();
            git.stale = true;
        }
        let focused = ctx.input(|i| i.focused);
        if focused && self.git.unfocused {
            self.git_touched();
        }
        self.git.unfocused = !focused;
        while let Ok(event) = self.git.rx.try_recv() {
            self.git_event(ctx, event);
        }
        if self.git.stale && !self.git.loading && !self.git.busy && self.git_visible() {
            let wait = Duration::from_secs(1);
            match self.git.last {
                Some(at) if at.elapsed() < wait => ctx.request_repaint_after(wait - at.elapsed()),
                _ => self.git_refresh(ctx),
            }
        }
    }
    fn git_refresh(&mut self, ctx: &egui::Context) {
        self.git.stale = false;
        self.git.loading = true;
        self.git.last = Some(Instant::now());
        self.git.folder = Some(self.project.clone());
        let (tx, folder, ctx) = (self.git.tx.clone(), self.project.clone(), ctx.clone());
        thread::spawn(move || {
            let snapshot = repo::snapshot(&folder);
            let _ = tx.send(Event::Snapshot(folder, snapshot));
            ctx.request_repaint();
        });
    }
    fn git_event(&mut self, ctx: &egui::Context, event: Event) {
        match event {
            Event::Snapshot(folder, snapshot) => {
                self.git.loading = false;
                if folder == self.project {
                    self.git.snapshot = Some(snapshot);
                }
            }
            Event::Done(op, result) => {
                self.git.busy = false;
                self.git.stale = true;
                self.git.blame.key = None;
                if result.is_ok() {
                    match op {
                        Op::Commit(_) => self.git.commit.clear(),
                        Op::CreateBranch(_) => self.git.branch_name.clear(),
                        Op::StashPush(_) => self.git.stash_message.clear(),
                        _ => {}
                    }
                    if op.moves_head() {
                        self.git_head_moved(ctx);
                    }
                }
                self.git.notice = match result {
                    Ok(text) if text.is_empty() => None,
                    other => Some(other),
                };
                // Lo preparado cambió: el diff abierto también.
                if let Some(view) = &self.git.diff {
                    let (title, target) = (view.title.clone(), view.target.clone());
                    self.git_show_diff(ctx, title, target);
                }
            }
            Event::Diff(request, result) => {
                if let Some(view) = self.git.diff.as_mut().filter(|v| v.request == request) {
                    view.body = Some(result.map(load_diff));
                }
            }
            Event::Blame(file, row, blame) => {
                self.git.blame.loading = false;
                self.git.blame.key = Some((file, row));
                self.git.blame.label = blame.map(|b| {
                    let hover = format!(
                        "{}\n{} · {}\n{}",
                        if b.committed {
                            &b.hash
                        } else {
                            "Sin confirmar"
                        },
                        b.author,
                        repo::relative(repo::now(), b.time),
                        b.summary
                    );
                    if !b.committed {
                        return ("Sin confirmar".to_string(), hover);
                    }
                    let summary: String = b.summary.chars().take(40).collect();
                    (
                        format!(
                            "{} · {} · {summary}",
                            b.author,
                            repo::relative(repo::now(), b.time)
                        ),
                        hover,
                    )
                });
            }
        }
    }
    /// Tras un checkout, un stash o un commit, los archivos abiertos y sus
    /// marcas del margen se releen del repositorio.
    fn git_head_moved(&mut self, ctx: &egui::Context) {
        self.files = project_files(&self.project);
        self.refresh_sources();
        let all = self.documents.len() <= 8;
        for (index, doc) in self.documents.iter_mut().enumerate() {
            doc.changes = (u64::MAX, Vec::new());
            if all || index == self.active {
                doc.git = doc
                    .editor
                    .path
                    .as_deref()
                    .filter(|_| doc.editor.format.editable())
                    .and_then(crate::git::info);
            }
        }
        // `watch_disk` recarga lo que cambió en disco; que no espere a otro cuadro.
        ctx.request_repaint_after(Duration::from_millis(2100));
    }

    pub(super) fn git_request(&mut self, ctx: &egui::Context, op: Op) {
        if self.git.busy {
            return;
        }
        if op.rewrites_files() && self.unsaved_documents() > 0 {
            self.git.confirm = Some(op);
            return;
        }
        self.git_start(ctx, op);
    }
    fn git_start(&mut self, ctx: &egui::Context, op: Op) {
        let Some(Ok(Some(snapshot))) = &self.git.snapshot else {
            return;
        };
        let root = snapshot.root.clone();
        self.git.busy = true;
        self.git.notice = None;
        let (tx, ctx) = (self.git.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let result = run_op(&root, &op);
            let _ = tx.send(Event::Done(op, result));
            ctx.request_repaint();
        });
    }
    pub(super) fn git_show_diff(&mut self, ctx: &egui::Context, title: String, target: Target) {
        let Some(Ok(Some(snapshot))) = &self.git.snapshot else {
            return;
        };
        let root = snapshot.root.clone();
        self.git.next_request += 1;
        let request = self.git.next_request;
        self.git.diff = Some(DiffView {
            title,
            target: target.clone(),
            request,
            body: None,
        });
        let (tx, ctx) = (self.git.tx.clone(), ctx.clone());
        thread::spawn(move || {
            let _ = tx.send(Event::Diff(request, repo::diff(&root, &target)));
            ctx.request_repaint();
        });
    }

    /// Autor y fecha del último commit que tocó la línea del cursor, listos
    /// para la barra de estado. Se pide a git una vez por línea y archivo.
    pub(super) fn blame_label(&mut self, ctx: &egui::Context) -> Option<(String, String)> {
        let doc = &self.documents[self.active];
        let path = doc.editor.path.as_ref()?;
        if doc.git.as_ref()?.base.is_none() || doc.editor.dirty() {
            return None;
        }
        let row = doc.editor.cursor.row;
        let known = self
            .git
            .blame
            .key
            .as_ref()
            .is_some_and(|(p, r)| p == path && *r == row);
        if known {
            return self.git.blame.label.clone();
        }
        // Al recorrer el archivo con las flechas no se pide cada línea: solo
        // la que el cursor lleva un rato quieto.
        let settled = match &self.git.blame.resting {
            Some((p, r, since)) if p == path && *r == row => since.elapsed(),
            _ => {
                self.git.blame.resting = Some((path.clone(), row, Instant::now()));
                Duration::ZERO
            }
        };
        const WAIT: Duration = Duration::from_millis(400);
        if settled < WAIT {
            ctx.request_repaint_after(WAIT - settled);
            return None;
        }
        if !self.git.blame.loading {
            self.git.blame.loading = true;
            let (file, tx, ctx) = (path.clone(), self.git.tx.clone(), ctx.clone());
            thread::spawn(move || {
                let blame = repo::blame(&file, row);
                let _ = tx.send(Event::Blame(file, row, blame));
                ctx.request_repaint();
            });
        }
        None
    }

    pub(super) fn git_sidebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let muted = col(self.theme.muted());
        let (error, success, warning, accent) = (
            col(self.theme.error),
            col(self.theme.success),
            col(self.theme.warning),
            col(self.theme.accent),
        );
        let unsaved = self.unsaved_documents();
        let git = &mut self.git;
        let mut ops = Vec::new();
        let mut diff = None;
        let mut refresh = false;
        ScrollArea::vertical()
            .id_salt("git")
            .auto_shrink([false, false])
            .show(ui, |ui| match &git.snapshot {
                None => {
                    ui.label("Leyendo el repositorio…");
                }
                Some(Err(message)) => {
                    ui.colored_label(error, message);
                    refresh = ui.button("Reintentar").clicked();
                }
                Some(Ok(None)) => {
                    ui.label(
                        "Esta carpeta no está en un repositorio de Git. Crea uno con «git init» en la terminal.",
                    );
                }
                Some(Ok(Some(snapshot))) => {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            refresh = ui
                                .small_button("Actualizar")
                                .on_hover_text("Vuelve a leer el estado del repositorio.")
                                .clicked();
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                    ui.label(
                                        RichText::new(format!("Rama {}", snapshot.branch))
                                            .strong(),
                                    )
                                    .on_hover_text(snapshot.root.display().to_string());
                                },
                            );
                        });
                    });
                    match &git.notice {
                        Some(Ok(text)) => {
                            ui.colored_label(success, text);
                        }
                        Some(Err(text)) => {
                            ui.colored_label(error, text);
                        }
                        None => {}
                    }
                    ui.add(
                        TextEdit::multiline(&mut git.commit)
                            .hint_text("Mensaje del commit")
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    let staged = snapshot.entries.iter().filter(|e| e.staged()).count();
                    if ui
                        .add_enabled(
                            !git.busy && !git.commit.trim().is_empty(),
                            egui::Button::new(format!("Confirmar ({staged} preparados)")),
                        )
                        .on_hover_text(
                            "Crea un commit con los archivos preparados. Si no hay ninguno, Git lo rechaza y el motivo aparece aquí.",
                        )
                        .on_disabled_hover_text("Escribe un mensaje para el commit.")
                        .clicked()
                    {
                        ops.push(Op::Commit(git.commit.clone()));
                    }
                    if unsaved > 0 {
                        ui.colored_label(
                            warning,
                            format!("{unsaved} documento(s) sin guardar: sus cambios no entran en el commit."),
                        );
                    }
                    ui.separator();
                    let paths = |entries: &[&repo::Entry], old: bool| -> Vec<String> {
                        entries.iter().flat_map(|e| e.paths(old)).collect()
                    };
                    let letter = |c: char| -> (String, Color32) {
                        let color = match c {
                            'A' => success,
                            'D' | 'U' => error,
                            'M' => warning,
                            '?' => muted,
                            _ => accent,
                        };
                        ((if c == '?' { 'U' } else { c }).to_string(), color)
                    };
                    let staged_entries: Vec<_> =
                        snapshot.entries.iter().filter(|e| e.staged()).collect();
                    let unstaged_entries: Vec<_> = snapshot
                        .entries
                        .iter()
                        .filter(|e| e.unstaged() && !e.untracked())
                        .collect();
                    let untracked: Vec<_> =
                        snapshot.entries.iter().filter(|e| e.untracked()).collect();
                    if snapshot.entries.is_empty() {
                        ui.label(RichText::new("Sin cambios.").color(muted));
                    }
                    for (title, id, rows) in [
                        ("Preparados", "staged", &staged_entries),
                        ("Sin preparar", "unstaged", &unstaged_entries),
                        ("Sin seguimiento", "untracked", &untracked),
                    ] {
                        if rows.is_empty() {
                            continue;
                        }
                        let header = format!("{title} ({})", rows.len());
                        egui::CollapsingHeader::new(header)
                            .id_salt(id)
                            .default_open(true)
                            .show(ui, |ui| {
                                let all = if id == "staged" {
                                    ("Quitar todos", Op::Unstage(paths(rows, true)))
                                } else {
                                    ("Preparar todos", Op::Stage(paths(rows, false)))
                                };
                                if ui
                                    .small_button(all.0)
                                    .on_hover_text("Aplica la acción a todos los archivos de esta lista.")
                                    .clicked()
                                {
                                    ops.push(all.1);
                                }
                                for entry in rows.iter() {
                                    let (mark, color) = letter(if id == "staged" {
                                        entry.index
                                    } else {
                                        entry.tree.max(entry.index)
                                    });
                                    let (button, help) = if id == "staged" {
                                        ("−", "Quita este archivo de los preparados.")
                                    } else {
                                        ("+", "Prepara este archivo para el commit.")
                                    };
                                    let (open, act) = list_row(
                                        ui,
                                        &mark,
                                        color,
                                        &entry.path,
                                        &format!("{}\nClic para ver los cambios.", entry.path),
                                        Some((button, help)),
                                    );
                                    if open {
                                        diff = Some(match id {
                                            "staged" => (
                                                format!("Preparado · {}", entry.path),
                                                Target::Staged(entry.paths(true)),
                                            ),
                                            "unstaged" => (
                                                format!("Sin preparar · {}", entry.path),
                                                Target::Unstaged(entry.paths(false)),
                                            ),
                                            _ => (
                                                format!("Sin seguimiento · {}", entry.path),
                                                Target::Untracked(entry.path.clone()),
                                            ),
                                        });
                                    }
                                    if act {
                                        ops.push(if id == "staged" {
                                            Op::Unstage(entry.paths(true))
                                        } else {
                                            Op::Stage(entry.paths(false))
                                        });
                                    }
                                }
                            });
                    }
                    egui::CollapsingHeader::new(format!("Ramas ({})", snapshot.branches.len()))
                        .id_salt("branches")
                        .show(ui, |ui| {
                            for branch in &snapshot.branches {
                                if ui
                                    .selectable_label(branch.current, &branch.name)
                                    .on_hover_text(if branch.current {
                                        "Rama actual."
                                    } else {
                                        "Cambia a esta rama."
                                    })
                                    .clicked()
                                    && !branch.current
                                {
                                    ops.push(Op::Checkout(branch.name.clone()));
                                }
                            }
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 64.0).max(60.0);
                                let field = ui.add(
                                    TextEdit::singleline(&mut git.branch_name)
                                        .hint_text("Rama nueva")
                                        .desired_width(width),
                                );
                                let enter =
                                    field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                                if (ui
                                    .add_enabled(
                                        !git.busy && !git.branch_name.trim().is_empty(),
                                        egui::Button::new("Crear"),
                                    )
                                    .on_hover_text("Crea la rama a partir de la actual y cambia a ella.")
                                    .clicked()
                                    || enter)
                                    && !git.branch_name.trim().is_empty()
                                {
                                    ops.push(Op::CreateBranch(git.branch_name.clone()));
                                }
                            });
                        });
                    egui::CollapsingHeader::new("Historial")
                        .id_salt("history")
                        .show(ui, |ui| {
                            if snapshot.log.is_empty() {
                                ui.label(RichText::new("Todavía no hay commits.").color(muted));
                            }
                            let now = repo::now();
                            for commit in &snapshot.log {
                                let when = repo::relative(now, commit.time);
                                let (open, _) = list_row(
                                    ui,
                                    &commit.hash,
                                    accent,
                                    &commit.subject,
                                    &format!(
                                        "{}\n{} · {when}\nClic para ver sus cambios.",
                                        commit.subject, commit.author
                                    ),
                                    None,
                                );
                                ui.label(
                                    RichText::new(format!("{} · {when}", commit.author))
                                        .size(11.0)
                                        .color(muted),
                                );
                                if open {
                                    diff = Some((
                                        format!("Commit {} · {}", commit.hash, commit.subject),
                                        Target::Commit(commit.hash.clone()),
                                    ));
                                }
                            }
                        });
                    egui::CollapsingHeader::new(format!("Stash ({})", snapshot.stashes.len()))
                        .id_salt("stash")
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let width = (ui.available_width() - 64.0).max(60.0);
                                ui.add(
                                    TextEdit::singleline(&mut git.stash_message)
                                        .hint_text("Mensaje (opcional)")
                                        .desired_width(width),
                                );
                                if ui
                                    .add_enabled(!git.busy, egui::Button::new("Guardar"))
                                    .on_hover_text("Aparta los cambios de los archivos con seguimiento y deja el árbol limpio.")
                                    .clicked()
                                {
                                    ops.push(Op::StashPush(git.stash_message.clone()));
                                }
                            });
                            if snapshot.stashes.is_empty() {
                                ui.label(RichText::new("No hay nada en el stash.").color(muted));
                            }
                            let now = repo::now();
                            for stash in &snapshot.stashes {
                                let (_, pop) = list_row(
                                    ui,
                                    "",
                                    muted,
                                    &stash.subject,
                                    &format!(
                                        "{} · {}\n{}",
                                        stash.name,
                                        repo::relative(now, stash.time),
                                        stash.subject
                                    ),
                                    Some(("Recuperar", "Aplica este stash y lo quita de la lista.")),
                                );
                                if pop && !git.busy {
                                    ops.push(Op::StashPop(stash.name.clone()));
                                }
                            }
                        });
                }
            });
        if refresh {
            self.git_touched();
            self.git.last = None;
        }
        if let Some((title, target)) = diff {
            self.git_show_diff(&ctx, title, target);
        }
        for op in ops {
            self.git_request(&ctx, op);
        }
    }

    pub(super) fn git_dialog(&mut self, ctx: &egui::Context) {
        self.git_confirm_dialog(ctx);
        self.git_diff_window(ctx);
    }

    fn git_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(op) = self.git.confirm.clone() else {
            return;
        };
        let count = self.unsaved_documents();
        let mut choice = 0;
        egui::Modal::new(Id::new("git_unsaved")).show(ctx, |ui| {
            ui.heading("Hay cambios sin guardar");
            ui.label(format!(
                "{count} documento(s) tienen cambios sin guardar. Esta acción cambia archivos del proyecto: si sigues sin guardar, esos documentos entrarán en conflicto con los del disco."
            ));
            ui.horizontal_wrapped(|ui| {
                if action(ui, "Guardar y continuar", true, "Guarda todos los documentos y ejecuta la acción de Git.").clicked() {
                    choice = 1;
                }
                if action(ui, "Continuar sin guardar", true, "Ejecuta la acción; luego decides qué versión de cada documento conservar.").clicked() {
                    choice = 2;
                }
                if action(ui, "Cancelar", true, "No hace nada.").clicked() {
                    choice = 3;
                }
            });
        });
        match choice {
            1 => {
                if self.save_all() {
                    self.git.confirm = None;
                    self.git_start(ctx, op);
                }
            }
            2 => {
                self.git.confirm = None;
                self.git_start(ctx, op);
            }
            3 => self.git.confirm = None,
            _ => {}
        }
    }

    fn git_diff_window(&mut self, ctx: &egui::Context) {
        let Some(view) = &self.git.diff else {
            return;
        };
        let colors = &self.theme;
        let (fg, bg, muted, accent, error) = (
            col(colors.fg),
            colors.bg,
            col(colors.muted()),
            col(colors.accent),
            col(colors.error),
        );
        let added = col(theme::mix(bg, colors.success, 0.2));
        let removed = col(theme::mix(bg, colors.error, 0.2));
        let busy = self.git.busy;
        let mut open = true;
        let mut hunk = None;
        egui::Window::new(&view.title)
            .id(Id::new("git_diff"))
            .open(&mut open)
            .default_size([780.0, 520.0])
            .resizable(true)
            .show(ctx, |ui| {
                let loaded = match &view.body {
                    None => {
                        ui.label("Leyendo los cambios…");
                        return;
                    }
                    Some(Err(message)) => {
                        ui.colored_label(error, message);
                        return;
                    }
                    Some(Ok(loaded)) => loaded,
                };
                if loaded.rows.is_empty() {
                    ui.label("Sin diferencias.");
                    return;
                }
                let reverse = matches!(view.target, Target::Staged(_));
                let by_hunk = matches!(view.target, Target::Staged(_) | Target::Unstaged(_))
                    && !loaded.hunk_rows.is_empty()
                    && loaded.patch.hunks.len() == loaded.hunk_rows.len();
                let font = FontId::monospace(13.0);
                let (char_width, row_height) =
                    ui.fonts_mut(|f| (f.glyph_width(&font, '0'), f.row_height(&font)));
                let row_height = row_height + 2.0;
                ScrollArea::both().auto_shrink(false).show_rows(
                    ui,
                    row_height,
                    loaded.rows.len(),
                    |ui, range| {
                        let width =
                            (loaded.widest as f32 * char_width + 150.0).max(ui.available_width());
                        for index in range {
                            let row = &loaded.rows[index];
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(width, row_height),
                                egui::Sense::hover(),
                            );
                            match row.kind {
                                Kind::Added => {
                                    ui.painter().rect_filled(rect, 0.0, added);
                                }
                                Kind::Removed => {
                                    ui.painter().rect_filled(rect, 0.0, removed);
                                }
                                _ => {}
                            }
                            let mut x = rect.left() + 6.0;
                            if row.kind == Kind::Hunk
                                && by_hunk
                                && let Ok(number) = loaded.hunk_rows.binary_search(&index)
                            {
                                let label = if reverse {
                                    "[Quitar bloque]"
                                } else {
                                    "[Preparar bloque]"
                                };
                                let size = egui::vec2(
                                    label.chars().count() as f32 * char_width,
                                    row_height,
                                );
                                let target =
                                    egui::Rect::from_min_size(egui::pos2(x, rect.top()), size);
                                let response = ui
                                    .interact(
                                        target,
                                        ui.id().with(("hunk", index)),
                                        egui::Sense::click(),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                                ui.painter().text(
                                    target.left_center(),
                                    egui::Align2::LEFT_CENTER,
                                    label,
                                    font.clone(),
                                    if response.hovered() { fg } else { accent },
                                );
                                if response.clicked() && !busy {
                                    hunk = Some(Op::Hunk {
                                        header: loaded.patch.header.clone(),
                                        hunk: loaded.patch.hunks[number].clone(),
                                        reverse,
                                    });
                                }
                                x += size.x + 2.0 * char_width;
                            }
                            let color = match row.kind {
                                Kind::Meta => muted,
                                Kind::Hunk => accent,
                                _ => fg,
                            };
                            ui.painter().text(
                                egui::pos2(x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                &row.text,
                                font.clone(),
                                color,
                            );
                        }
                    },
                );
            });
        if !open {
            self.git.diff = None;
        }
        if let Some(op) = hunk {
            self.git_request(ctx, op);
        }
    }
}

#[cfg(test)]
impl App {
    /// Lo leído del repositorio: `None` mientras carga, `Some(None)` fuera de uno.
    pub(super) fn git_loaded(&self) -> Option<Option<&repo::Snapshot>> {
        match self.git.snapshot.as_ref()? {
            Ok(snapshot) => Some(snapshot.as_ref()),
            Err(_) => Some(None),
        }
    }
    pub(super) fn git_idle(&self) -> bool {
        !self.git.busy && !self.git.loading
    }
    pub(super) fn git_confirming(&self) -> bool {
        self.git.confirm.is_some()
    }
    pub(super) fn git_diff_text(&self) -> Option<String> {
        let Some(Ok(loaded)) = self.git.diff.as_ref()?.body.as_ref() else {
            return None;
        };
        Some(
            loaded
                .rows
                .iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
    pub(super) fn git_blame_text(&self) -> Option<String> {
        self.git.blame.label.as_ref().map(|(text, _)| text.clone())
    }
    pub(super) fn git_notice(&self) -> Option<Result<String, String>> {
        self.git.notice.clone()
    }
}
