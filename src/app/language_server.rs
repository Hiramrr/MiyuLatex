//! Servidores de lenguaje (LSP): diagnósticos en el margen y el panel de
//! problemas, ir a la definición y hover. Todo el trabajo corre en los hilos
//! de `crate::lsp`; aquí solo se reparten los resultados.

use super::*;
use crate::{
    editor::Pos,
    lsp::{self, DocRef, Outcome, Severity},
};
use super::preview_panel::copy_menu;
use std::collections::BTreeMap;

/// El ratón debe quedarse quieto este tiempo sobre un identificador.
const HOVER_DELAY: Duration = Duration::from_millis(500);
/// Tope del texto de un hover.
const HOVER_CHARS: usize = 1500;
const HOVER_LINES: usize = 24;
/// Problemas de servidor que muestra el panel: una lista enorme no se dibuja entera.
const PANEL_LIMIT: usize = 300;

/// Texto de hover: trozos de prosa y de código, en ese orden.
struct Shown {
    parts: Vec<(bool, String)>,
    pointer: egui::Pos2,
    revision: u64,
}

#[derive(Default)]
pub(super) struct Hover {
    doc: Option<u64>,
    pointer: Option<egui::Pos2>,
    since: Option<Instant>,
    /// Ya se miró qué hay bajo el puntero quieto.
    probed: bool,
    asked: Option<Pos>,
    shown: Option<Shown>,
}

impl Hover {
    /// Sigue el puntero sobre el editor. Devuelve su posición cuando lleva
    /// quieto el tiempo de espera y aún no se pidió nada para ella.
    pub(super) fn rest(
        &mut self,
        ctx: &egui::Context,
        doc: Id,
        pointer: Option<egui::Pos2>,
    ) -> Option<egui::Pos2> {
        let hover = self;
        let scrolled = ctx.input(|i| i.smooth_scroll_delta != egui::Vec2::ZERO);
        let moved = scrolled
            || hover.doc != Some(doc.value())
            || match (hover.pointer, pointer) {
                (Some(a), Some(b)) => a.distance(b) > 1.0,
                (None, None) => false,
                _ => true,
            };
        if moved {
            *hover = Hover {
                doc: Some(doc.value()),
                pointer,
                since: Some(Instant::now()),
                ..Hover::default()
            };
            if pointer.is_some() {
                ctx.request_repaint_after(HOVER_DELAY);
            }
            return None;
        }
        let waited = hover.since?.elapsed();
        if hover.probed {
            return None;
        }
        if waited < HOVER_DELAY {
            ctx.request_repaint_after(HOVER_DELAY - waited);
            return None;
        }
        hover.probed = true;
        pointer
    }
}

pub(super) struct State {
    hub: lsp::Hub,
    armed: bool,
    /// Problemas que publicó cada servidor, con la forma que usa el margen.
    pub diagnostics: BTreeMap<PathBuf, Vec<Diagnostic>>,
    /// Documento y posición de la definición que se espera.
    definition: Option<(u64, Pos)>,
    pub hover: Hover,
}

impl Default for State {
    fn default() -> Self {
        Self {
            // Las pruebas de la aplicación no lanzan los servidores de esta máquina.
            hub: if cfg!(test) {
                lsp::Hub::new(Vec::new())
            } else {
                lsp::Hub::standard()
            },
            armed: false,
            diagnostics: BTreeMap::new(),
            definition: None,
            hover: Hover::default(),
        }
    }
}

impl State {
    /// Cierra los servidores al salir de la aplicación.
    pub fn hub_shutdown(&mut self) {
        self.hub.shutdown();
    }
}

impl App {
    /// Una vez por cuadro: abre y sincroniza los documentos con sus servidores
    /// y recoge lo que contestaron. No espera a nadie.
    pub(super) fn poll_lsp(&mut self, ctx: &egui::Context) {
        if !self.config.lsp && self.lsp.hub.idle() {
            return;
        }
        if !self.lsp.armed {
            let ctx = ctx.clone();
            self.lsp
                .hub
                .set_wake(Arc::new(move || ctx.request_repaint()));
            self.lsp.armed = true;
        }
        let docs: Vec<DocRef> = self
            .documents
            .iter()
            .filter(|d| d.editor.format.editable())
            .map(|d| doc_ref(d))
            .collect();
        let wait = self.lsp.hub.sync(self.config.lsp, &self.project, &docs);
        let outcomes = self.lsp.hub.poll();
        drop(docs);
        for outcome in outcomes {
            self.lsp_outcome(outcome);
        }
        if let Some(wait) = wait {
            ctx.request_repaint_after(wait);
        }
        // El plazo de la definición lo vigila `poll`, que corre en cada cuadro.
        if self.lsp.definition.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn lsp_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Diagnostics(path, problems) => {
                if problems.is_empty() {
                    self.lsp.diagnostics.remove(&path);
                    return;
                }
                let mut list: Vec<Diagnostic> = problems
                    .into_iter()
                    .map(|p| Diagnostic {
                        path: path.clone(),
                        row: p.row,
                        error: p.severity == Severity::Error,
                        message: p.message,
                        severity: p.severity.label(),
                    })
                    .collect();
                list.sort_by_key(|d| d.row);
                self.lsp.diagnostics.insert(path, list);
            }
            Outcome::Definition { doc, locations } => {
                let Some((_, pos)) = self.lsp.definition.filter(|(d, _)| *d == doc) else {
                    return;
                };
                self.lsp.definition = None;
                if self.documents[self.active].id.value() != doc {
                    return;
                }
                let Some(first) = locations.first() else {
                    self.goto_definition_local(pos);
                    return;
                };
                let col = lsp::char_col(&self.line_of(&first.path, first.row), first.col);
                let target = Target {
                    path: first.path.clone(),
                    row: first.row,
                    col,
                    label: String::new(),
                    detail: String::new(),
                };
                self.jump(&target);
                if locations.len() > 1 {
                    self.message = format!("{} definiciones; abrí la primera", locations.len());
                }
            }
            Outcome::Hover { doc, pos, text } => {
                let hover = &mut self.lsp.hover;
                if hover.doc != Some(doc) || hover.asked != Some(pos) {
                    return;
                }
                let (Some(text), Some(pointer)) = (text, hover.pointer) else {
                    return;
                };
                let revision = self.documents[self.active].editor.revision;
                self.lsp.hover.shown = Some(Shown {
                    parts: hover_parts(&text),
                    pointer,
                    revision,
                });
            }
        }
    }

    /// Texto de una línea, de la pestaña abierta o, si no, del disco.
    fn line_of(&self, path: &Path, row: usize) -> String {
        match self
            .documents
            .iter()
            .find(|d| d.editor.path.as_deref() == Some(path))
        {
            Some(doc) => doc.editor.lines.get(row).cloned().unwrap_or_default(),
            None => fs::read_to_string(path)
                .ok()
                .and_then(|text| text.lines().nth(row).map(str::to_owned))
                .unwrap_or_default(),
        }
    }

    /// Pide la definición al servidor del documento activo. `false` si no hay
    /// ninguno que pueda responder: se usa la heurística local.
    pub(super) fn lsp_definition(&mut self, pos: Pos) -> bool {
        if !self.config.lsp {
            return false;
        }
        let doc = &self.documents[self.active];
        let Some(line) = doc.editor.lines.get(pos.row) else {
            return false;
        };
        let asked = self.lsp.hub.definition(&doc_ref(doc), line, pos);
        if asked {
            self.lsp.definition = Some((doc.id.value(), pos));
        }
        asked
    }

    pub(super) fn lsp_saved(&mut self, index: usize) {
        if self.config.lsp && !self.lsp.hub.idle() {
            self.lsp.hub.saved(&doc_ref(&self.documents[index]));
        }
    }

    /// Los problemas de las compilaciones y los de los servidores.
    pub(super) fn all_diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .chain(self.lsp.diagnostics.values().flatten())
    }

    /// Estado del servidor del documento activo para la barra de estado.
    pub(super) fn lsp_status(&self) -> Option<(String, String)> {
        use lsp::State::*;
        if !self.config.lsp {
            return None;
        }
        let status = self
            .lsp
            .hub
            .status(self.documents[self.active].id.value())?;
        let server = status.server;
        Some(match status.state {
            Starting => (
                format!("{server}: iniciando"),
                "El servidor de lenguaje está arrancando.".into(),
            ),
            Ready => (
                format!("{server}: listo"),
                "Servidor de lenguaje: diagnósticos, definición (F12) y hover.".into(),
            ),
            Busy(work) => (format!("{server}: indexando"), work),
            Unavailable(reason) => (
                "sin servidor".into(),
                format!("{reason}. El editor usa sus propias heurísticas."),
            ),
        })
    }

    /// Problemas de los servidores de lenguaje en el panel de problemas.
    pub(super) fn language_server_problems(
        &self,
        ui: &mut egui::Ui,
        jump: &mut Option<(PathBuf, Option<usize>)>,
    ) {
        let all = self.lsp.diagnostics.values().flatten();
        for problem in all.clone().take(PANEL_LIMIT) {
            let color = match problem.severity {
                "error" => self.theme.error,
                "warning" => self.theme.warning,
                _ => self.theme.muted(),
            };
            let text = format!(
                "{} · {}:{} · {}",
                problem.severity,
                problem.path.display(),
                problem.row + 1,
                problem.message
            );
            let button = ui.button(RichText::new(&text).color(col(color)));
            copy_menu(&button, text);
            if button.clicked() {
                *jump = Some((problem.path.clone(), Some(problem.row + 1)));
            }
        }
        let more = all.count().saturating_sub(PANEL_LIMIT);
        if more > 0 {
            ui.label(format!("… y {more} problemas más del servidor de lenguaje"));
        }
    }

    /// Los problemas del servidor de lenguaje, uno por línea, para copiarlos.
    pub(super) fn language_server_text(&self) -> String {
        self.lsp
            .diagnostics
            .values()
            .flatten()
            .map(|problem| {
                format!(
                    "{} · {}:{} · {}",
                    problem.severity,
                    problem.path.display(),
                    problem.row + 1,
                    problem.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Hay un servidor listo para el documento activo.
    pub(super) fn has_language_server(&self) -> bool {
        self.config.lsp
            && self
                .lsp
                .hub
                .status(self.documents[self.active].id.value())
                .is_some_and(|s| matches!(s.state, lsp::State::Ready | lsp::State::Busy(_)))
    }

    /// El servidor del documento puede responder al pasar el ratón.
    pub(super) fn lsp_hover_enabled(&self, doc: Id) -> bool {
        self.config.lsp && self.lsp.hub.hover_ready(doc.value())
    }

    /// Pide el hover del identificador en `pos`, si lo hay.
    pub(super) fn lsp_hover(&mut self, pos: Pos) {
        let doc = &self.documents[self.active];
        let Some(line) = doc.editor.lines.get(pos.row) else {
            return;
        };
        if !line
            .chars()
            .nth(pos.col)
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            return;
        }
        if self.lsp.hub.hover(&doc_ref(doc), line, pos) {
            self.lsp.hover.asked = Some(pos);
        }
    }

    pub(super) fn lsp_hover_ui(&mut self, ctx: &egui::Context) {
        let Some(shown) = &self.lsp.hover.shown else {
            return;
        };
        if shown.revision != self.documents[self.active].editor.revision
            || self.lsp.hover.pointer != Some(shown.pointer)
        {
            self.lsp.hover.shown = None;
            return;
        }
        let muted = col(self.theme.muted());
        egui::Area::new(Id::new("lsp_hover"))
            .order(egui::Order::Tooltip)
            .fixed_pos(shown.pointer + egui::vec2(14.0, 18.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(520.0);
                    for (code, text) in &shown.parts {
                        if *code {
                            ui.label(RichText::new(text).monospace().size(13.0));
                        } else {
                            ui.label(RichText::new(text).size(13.0).color(muted));
                        }
                    }
                });
            });
    }
}

fn doc_ref(doc: &Document) -> DocRef<'_> {
    DocRef {
        key: doc.id.value(),
        path: doc.editor.path.as_deref(),
        revision: doc.editor.revision,
        source: doc.editor.source(),
    }
}

/// Markdown simple: los bloques de código van aparte; el resto, como texto.
/// Se recorta a un tamaño razonable.
fn hover_parts(markdown: &str) -> Vec<(bool, String)> {
    let mut parts: Vec<(bool, String)> = Vec::new();
    let (mut code, mut chars, mut lines) = (false, 0, 0);
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if lines >= HOVER_LINES || chars >= HOVER_CHARS {
            if let Some((_, last)) = parts.last_mut() {
                last.push_str("\n…");
            }
            break;
        }
        let line: String = line.chars().take(HOVER_CHARS - chars).collect();
        chars += line.chars().count();
        lines += 1;
        match parts.last_mut() {
            Some((kind, text)) if *kind == code => {
                text.push('\n');
                text.push_str(&line);
            }
            _ => parts.push((code, line)),
        }
    }
    parts.retain(|(_, text)| !text.trim().is_empty());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_separates_code_and_truncates() {
        let parts = hover_parts("```c\nint f(void)\n```\n\nHace *cosas*.\n");
        assert_eq!(parts[0], (true, "int f(void)".into()));
        assert!(!parts[1].0);
        assert!(parts[1].1.contains("Hace"));
        let long = "x\n".repeat(100);
        let parts = hover_parts(&long);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].1.ends_with('…') && parts[0].1.lines().count() <= HOVER_LINES + 1);
    }
}

#[cfg(test)]
mod app_tests {
    use super::*;
    use crate::{
        app::tests::{key, tick},
        backdrop::Backdrop,
        lsp::tests::{fake_table, folder},
    };

    fn pump(app: &mut App, ctx: &egui::Context, done: impl Fn(&App) -> bool) {
        let limit = Instant::now() + Duration::from_secs(20);
        while Instant::now() < limit {
            tick(app, ctx, vec![]);
            if done(app) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("se agotó la espera");
    }

    fn app_with_fake(name: &str, text: &str) -> Option<(App, egui::Context, PathBuf)> {
        let dir = folder(name);
        let table = fake_table(&dir, "normal")?;
        let path = dir.join("a.fake");
        fs::write(&path, text).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(Some(path), &ctx).unwrap();
        app.config.autocompile = false;
        app.config.autosave = false;
        app.config.mascot = false;
        app.backdrop = Backdrop::default();
        app.lsp.hub = lsp::Hub::new(table);
        Some((app, ctx, dir))
    }

    #[test]
    fn problems_status_definition_and_switch() {
        let Some((mut app, ctx, dir)) = app_with_fake("app", "é😀 ERR\n😀foo = 1\nfoo\n")
        else {
            return;
        };
        pump(&mut app, &ctx, |app| !app.lsp.diagnostics.is_empty());
        let marks: Vec<_> = app.all_diagnostics().map(|d| (d.row, d.error)).collect();
        assert_eq!(marks, vec![(0, true)]);
        pump(&mut app, &ctx, |app| app.has_language_server());
        assert!(app.lsp_status().unwrap().0.starts_with("fake: "));

        // F12 en `foo` de la fila 2: el servidor manda a «foo =» tras el emoji.
        app.editor_mut().goto(2, 1);
        tick(&mut app, &ctx, vec![key(Key::F12, Modifiers::NONE)]);
        pump(&mut app, &ctx, |app| app.editor().cursor.row == 1);
        assert_eq!(app.editor().cursor, Pos::new(1, 1));

        // Quitar el error: el cambio llega tras la pausa y el margen se limpia.
        let end = app.editor().end();
        app.editor_mut()
            .replace(Pos::new(0, 0), end, "sin error\n😀foo = 1\n");
        pump(&mut app, &ctx, |app| app.lsp.diagnostics.is_empty());

        // Apagar el LSP en Preferencias cierra los servidores.
        app.config.lsp = false;
        tick(&mut app, &ctx, vec![]);
        tick(&mut app, &ctx, vec![]);
        assert!(app.lsp.hub.idle() && app.lsp_status().is_none());
        // Sin servidor, F12 cae a la heurística local (aquí no hay referencia).
        tick(&mut app, &ctx, vec![key(Key::F12, Modifiers::NONE)]);
        assert!(app.message.contains("no hay una referencia") || app.lsp.definition.is_none());
        app.lsp.hub_shutdown();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hover_shows_the_server_text_after_the_pointer_rests() {
        let Some((mut app, ctx, dir)) = app_with_fake("hover", "hola mundo\n") else {
            return;
        };
        app.config.show_sidebar = false;
        app.config.show_preview = false;
        pump(&mut app, &ctx, |app| {
            app.lsp_hover_enabled(app.documents[0].id)
        });
        let id = app.documents[0].id;
        // Se busca el punto del texto que cae sobre «hola».
        let start = Instant::now();
        let mut found = false;
        'scan: for y in (90..140).step_by(6) {
            let at = egui::pos2(60.0, y as f32);
            tick(&mut app, &ctx, vec![egui::Event::PointerMoved(at)]);
            let rested = Instant::now() + HOVER_DELAY + Duration::from_millis(150);
            while Instant::now() < rested {
                tick(&mut app, &ctx, vec![]);
                thread::sleep(Duration::from_millis(20));
            }
            for _ in 0..30 {
                tick(&mut app, &ctx, vec![]);
                if app.lsp.hover.asked.is_none() {
                    break;
                }
                if app.lsp.hover.shown.is_some() {
                    found = true;
                    break 'scan;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        assert!(found, "no apareció el hover tras {:?}", start.elapsed());
        let shown = app.lsp.hover.shown.as_ref().unwrap();
        assert!(shown.parts[0].1.starts_with("L0:C"), "{:?}", shown.parts);
        // Moverse lo quita.
        tick(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(900.0, 700.0))],
        );
        assert!(app.lsp.hover.shown.is_none());
        let _ = id;
        app.lsp.hub_shutdown();
        fs::remove_dir_all(dir).unwrap();
    }

    /// Como `rendimiento_documentos_grandes`, pero con un servidor vivo que
    /// recibe el texto: `cargo test --release rendimiento -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rendimiento_con_servidor_de_lenguaje() {
        let text: String = (0..15000)
            .map(|i| format!("línea {i} con texto de relleno 😀 y ERR a veces\n"))
            .collect();
        let Some((mut app, ctx, dir)) = app_with_fake("perf", &text) else {
            return;
        };
        let row = app.editor().lines.len() / 2;
        app.editor_mut().goto(row, 0);
        pump(&mut app, &ctx, |app| app.has_language_server());
        for _ in 0..200 {
            tick(&mut app, &ctx, vec![]);
        }
        let time = |app: &mut App, events: &dyn Fn(usize) -> Vec<egui::Event>| {
            let mut frames: Vec<_> = (0..40)
                .map(|i| {
                    let started = Instant::now();
                    tick(app, &ctx, events(i));
                    started.elapsed()
                })
                .collect();
            frames.sort();
            (frames[frames.len() / 2], frames[frames.len() - 1])
        };
        for lsp in [true, false] {
            app.config.lsp = lsp;
            for _ in 0..5 {
                tick(&mut app, &ctx, vec![]);
            }
            let idle = time(&mut app, &|_| vec![]);
            let typing = time(&mut app, &|i| {
                vec![egui::Event::Text(
                    ((b'a' + (i % 26) as u8) as char).to_string(),
                )]
            });
            println!(
                "{} líneas, LSP {} · reposo {:.2?} (máx {:.2?}) · tecla {:.2?} (máx {:.2?})",
                app.editor().lines.len(),
                if lsp {
                    "activo con servidor falso"
                } else {
                    "desactivado"
                },
                idle.0,
                idle.1,
                typing.0,
                typing.1,
            );
        }
        app.lsp.hub_shutdown();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_dead_server_leaves_the_editor_working() {
        let dir = folder("dead");
        let Some(table) = fake_table(&dir, "die") else {
            return;
        };
        let path = dir.join("a.fake");
        fs::write(&path, "foo = 1\n").unwrap();
        let ctx = egui::Context::default();
        let mut app = App::new(Some(path), &ctx).unwrap();
        app.config.mascot = false;
        app.backdrop = Backdrop::default();
        app.lsp.hub = lsp::Hub::new(table);
        pump(&mut app, &ctx, |app| {
            app.lsp_status()
                .is_some_and(|(text, _)| text == "sin servidor")
        });
        tick(&mut app, &ctx, vec![egui::Event::Text("x".into())]);
        assert!(app.editor().source().starts_with('x'));
        app.editor_mut().goto(0, 0);
        tick(&mut app, &ctx, vec![key(Key::F12, Modifiers::NONE)]);
        assert!(app.lsp.definition.is_none());
        app.lsp.hub_shutdown();
        fs::remove_dir_all(dir).unwrap();
    }
}
