//! Servidores en marcha y documentos abiertos en ellos. Todo aquí es de la
//! hebra de la interfaz y no espera nunca: lo que tarda ocurre en los hilos
//! de `Client` y vuelve como eventos que `poll` reparte.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{
    client::{Client, Event, Wake},
    position::utf16_col,
    servers::{Spec, default_table, file_uri, path_from_uri},
};
use crate::editor::Pos;

/// Pausa tras la última edición antes de mandar el texto al servidor.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Pasado este plazo, «ir a definición» recurre a la heurística local.
pub const DEFINITION_TIMEOUT: Duration = Duration::from_millis(1500);
const HOVER_TIMEOUT: Duration = Duration::from_secs(5);

pub struct DocRef<'a> {
    pub key: u64,
    pub path: Option<&'a Path>,
    pub revision: u64,
    pub source: &'a str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Problem {
    pub row: usize,
    /// Unidades UTF-16 desde el inicio de la línea.
    pub col: u32,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub row: usize,
    /// Unidades UTF-16 desde el inicio de la línea; `position::char_col` las convierte.
    pub col: u32,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Todos los problemas vigentes de un archivo; vacío los borra.
    Diagnostics(PathBuf, Vec<Problem>),
    /// Sin lugares: el servidor no halló nada, falló o tardó demasiado.
    Definition { doc: u64, locations: Vec<Location> },
    Hover {
        doc: u64,
        pos: Pos,
        text: Option<String>,
    },
}

#[derive(Debug, PartialEq)]
pub enum State {
    Starting,
    Ready,
    /// Trabajando en segundo plano (indexando, comprobando); con el título.
    Busy(String),
    Unavailable(String),
}

#[derive(Debug, PartialEq)]
pub struct Status {
    pub server: String,
    pub state: State,
}

type SlotKey = (String, PathBuf);

enum Pending {
    Definition {
        doc: u64,
        started: Instant,
    },
    Hover {
        doc: u64,
        pos: Pos,
        started: Instant,
    },
}

struct Running {
    client: Client,
    progress: BTreeMap<String, String>,
    pending: HashMap<i64, Pending>,
    published: HashSet<PathBuf>,
}

enum Slot {
    Running(Box<Running>),
    Unavailable(String),
}

struct Open {
    path: PathBuf,
    uri: String,
    language: String,
    slot: Option<SlotKey>,
    version: i32,
    /// Última revisión vista y desde cuándo está sin enviar.
    seen: u64,
    changed_at: Option<Instant>,
}

pub struct Hub {
    table: Vec<Spec>,
    project: PathBuf,
    wake: Wake,
    slots: HashMap<SlotKey, Slot>,
    open: HashMap<u64, Open>,
    uris: HashMap<String, PathBuf>,
    outbox: Vec<Outcome>,
    closing: Vec<JoinHandle<()>>,
}

impl Hub {
    pub fn new(table: Vec<Spec>) -> Self {
        Self {
            table,
            project: PathBuf::new(),
            wake: std::sync::Arc::new(|| {}),
            slots: HashMap::new(),
            open: HashMap::new(),
            uris: HashMap::new(),
            outbox: Vec::new(),
            closing: Vec::new(),
        }
    }

    pub fn standard() -> Self {
        Self::new(default_table())
    }

    pub fn set_wake(&mut self, wake: Wake) {
        self.wake = wake;
    }

    /// Ni servidores ni documentos que seguir: no hay nada que hacer cada cuadro.
    pub fn idle(&self) -> bool {
        self.slots.is_empty() && self.open.is_empty()
    }

    /// Abre en el servidor los documentos nuevos, cierra los que se fueron y
    /// manda los cambios tras la pausa. Devuelve cuánto falta para el
    /// próximo envío pendiente, para pedir otro cuadro entonces.
    pub fn sync(&mut self, enabled: bool, project: &Path, docs: &[DocRef]) -> Option<Duration> {
        if !enabled || self.project != project {
            self.stop_all();
            self.project = project.to_path_buf();
            if !enabled {
                return None;
            }
        }
        for doc in docs {
            let Some(path) = doc.path else { continue };
            if self.open.get(&doc.key).is_none_or(|open| open.path != path) {
                self.close_doc(doc.key);
                self.open_doc(doc, path);
            }
        }
        let gone: Vec<u64> = self
            .open
            .keys()
            .filter(|key| !docs.iter().any(|d| d.key == **key && d.path.is_some()))
            .copied()
            .collect();
        for key in gone {
            self.close_doc(key);
        }
        let now = Instant::now();
        let mut wait: Option<Duration> = None;
        for doc in docs {
            let Some(open) = self.open.get_mut(&doc.key) else {
                continue;
            };
            if open.seen != doc.revision {
                open.seen = doc.revision;
                open.changed_at = Some(now);
            }
            let Some(at) = open.changed_at else { continue };
            let due = at + DEBOUNCE;
            if now >= due {
                open.changed_at = None;
                Self::send_change(&mut self.slots, open, doc.source);
            } else {
                let left = due - now;
                wait = Some(wait.map_or(left, |w| w.min(left)));
            }
        }
        wait
    }

    fn open_doc(&mut self, doc: &DocRef, path: &Path) {
        let found = self
            .table
            .iter()
            .find_map(|spec| Some((spec, spec.language(path)?.to_string())));
        let mut open = Open {
            path: path.to_path_buf(),
            uri: file_uri(path),
            language: String::new(),
            slot: None,
            version: 1,
            seen: doc.revision,
            changed_at: None,
        };
        if let Some((spec, language)) = found {
            let key = (spec.name.clone(), spec.root(path, &self.project));
            if !self.slots.contains_key(&key) {
                let slot = match spec.resolve() {
                    None => Slot::Unavailable(format!("{} no está instalado", spec.name)),
                    Some((program, args)) => {
                        match Client::start(&program, &args, &key.1, self.wake.clone()) {
                            Ok(client) => Slot::Running(Box::new(Running {
                                client,
                                progress: BTreeMap::new(),
                                pending: HashMap::new(),
                                published: HashSet::new(),
                            })),
                            Err(e) => {
                                eprintln!("lsp: {e}");
                                Slot::Unavailable(e)
                            }
                        }
                    }
                };
                self.slots.insert(key.clone(), slot);
            }
            if let Some(Slot::Running(run)) = self.slots.get_mut(&key) {
                run.client.notify(
                    "textDocument/didOpen",
                    json!({"textDocument": {
                        "uri": open.uri,
                        "languageId": language,
                        "version": open.version,
                        "text": doc.source,
                    }}),
                );
            }
            open.language = language;
            open.slot = Some(key);
        }
        self.open.insert(doc.key, open);
    }

    fn close_doc(&mut self, key: u64) {
        let Some(open) = self.open.remove(&key) else {
            return;
        };
        if let Some(Slot::Running(run)) = open.slot.as_ref().and_then(|k| self.slots.get_mut(k)) {
            run.client.notify(
                "textDocument/didClose",
                json!({"textDocument": {"uri": open.uri}}),
            );
            if run.published.remove(&open.path) {
                self.outbox
                    .push(Outcome::Diagnostics(open.path, Vec::new()));
            }
        }
    }

    fn send_change(slots: &mut HashMap<SlotKey, Slot>, open: &mut Open, source: &str) {
        let Some(Slot::Running(run)) = open.slot.as_ref().and_then(|k| slots.get_mut(k)) else {
            return;
        };
        if run.client.capabilities.sync == 0 {
            return;
        }
        open.version += 1;
        run.client.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": open.uri, "version": open.version},
                "contentChanges": [{"text": source}],
            }),
        );
    }

    /// El texto pendiente llega ya al servidor, sin esperar la pausa.
    fn flush(&mut self, doc: &DocRef) {
        if let Some(open) = self.open.get_mut(&doc.key)
            && (open.changed_at.take().is_some() || open.seen != doc.revision)
        {
            open.seen = doc.revision;
            Self::send_change(&mut self.slots, open, doc.source);
        }
    }

    pub fn saved(&mut self, doc: &DocRef) {
        self.flush(doc);
        let Some(open) = self.open.get(&doc.key) else {
            return;
        };
        if let Some(Slot::Running(run)) = open.slot.as_ref().and_then(|k| self.slots.get_mut(k)) {
            run.client.notify(
                "textDocument/didSave",
                json!({"textDocument": {"uri": open.uri}}),
            );
        }
    }

    pub fn status(&self, key: u64) -> Option<Status> {
        let slot_key = self.open.get(&key)?.slot.as_ref()?;
        let state = match self.slots.get(slot_key)? {
            Slot::Unavailable(reason) => State::Unavailable(reason.clone()),
            Slot::Running(run) if !run.client.ready() => State::Starting,
            Slot::Running(run) => match run.progress.values().next() {
                Some(title) => State::Busy(title.clone()),
                None => State::Ready,
            },
        };
        Some(Status {
            server: slot_key.0.clone(),
            state,
        })
    }

    /// Hay un servidor listo que sabe responder al pasar el ratón.
    pub fn hover_ready(&self, key: u64) -> bool {
        self.running(key)
            .is_some_and(|run| run.client.capabilities.hover)
    }

    fn running(&self, key: u64) -> Option<&Running> {
        let slot = self.open.get(&key)?.slot.as_ref()?;
        match self.slots.get(slot)? {
            Slot::Running(run) if run.client.ready() => Some(run),
            _ => None,
        }
    }

    /// Pide la definición del símbolo en `pos`. `false`: no hay servidor que
    /// pueda responder y quien llama debe usar su heurística.
    pub fn definition(&mut self, doc: &DocRef, line: &str, pos: Pos) -> bool {
        let Some(run) = self.running(doc.key) else {
            return false;
        };
        if !run.client.capabilities.definition {
            return false;
        }
        self.flush(doc);
        let started = Instant::now();
        self.request(doc.key, "textDocument/definition", line, pos, |_| {
            Pending::Definition {
                doc: doc.key,
                started,
            }
        })
    }

    pub fn hover(&mut self, doc: &DocRef, line: &str, pos: Pos) -> bool {
        if !self.hover_ready(doc.key) {
            return false;
        }
        self.flush(doc);
        let started = Instant::now();
        self.request(doc.key, "textDocument/hover", line, pos, |_| {
            Pending::Hover {
                doc: doc.key,
                pos,
                started,
            }
        })
    }

    fn request(
        &mut self,
        key: u64,
        method: &str,
        line: &str,
        pos: Pos,
        pending: impl FnOnce(i64) -> Pending,
    ) -> bool {
        let Some(open) = self.open.get(&key) else {
            return false;
        };
        let params = json!({
            "textDocument": {"uri": open.uri},
            "position": {"line": pos.row, "character": utf16_col(line, pos.col)},
        });
        let Some(Slot::Running(run)) = open.slot.as_ref().and_then(|k| self.slots.get_mut(k))
        else {
            return false;
        };
        let Some(id) = run.client.request(method, params) else {
            return false;
        };
        run.pending.insert(id, pending(id));
        true
    }

    /// Reparte lo que llegó de los servidores desde la última vez.
    pub fn poll(&mut self) -> Vec<Outcome> {
        let mut out = std::mem::take(&mut self.outbox);
        let mut dead = Vec::new();
        for (key, slot) in &mut self.slots {
            let Slot::Running(run) = slot else { continue };
            for event in run.client.poll() {
                match event {
                    Event::Ready => {}
                    Event::Notification(method, params) => match method.as_str() {
                        "textDocument/publishDiagnostics" => {
                            if let Some((path, problems)) =
                                parse_diagnostics(&params, &mut self.uris)
                            {
                                if problems.is_empty() {
                                    run.published.remove(&path);
                                } else {
                                    run.published.insert(path.clone());
                                }
                                out.push(Outcome::Diagnostics(path, problems));
                            }
                        }
                        "$/progress" => progress(&mut run.progress, &params),
                        _ => {}
                    },
                    Event::Response(id, result) => match run.pending.remove(&id) {
                        Some(Pending::Definition { doc, .. }) => out.push(Outcome::Definition {
                            doc,
                            locations: result
                                .map(|r| parse_locations(&r, &mut self.uris))
                                .unwrap_or_default(),
                        }),
                        Some(Pending::Hover { doc, pos, .. }) => out.push(Outcome::Hover {
                            doc,
                            pos,
                            text: result.ok().and_then(|r| hover_text(&r)),
                        }),
                        None => {}
                    },
                    Event::Exited(reason) => dead.push((key.clone(), reason)),
                }
            }
            run.pending.retain(|_, pending| match pending {
                Pending::Definition { doc, started } if started.elapsed() > DEFINITION_TIMEOUT => {
                    out.push(Outcome::Definition {
                        doc: *doc,
                        locations: Vec::new(),
                    });
                    false
                }
                Pending::Hover { started, .. } => started.elapsed() <= HOVER_TIMEOUT,
                _ => true,
            });
        }
        for (key, reason) in dead {
            // El editor sigue con sus heurísticas; el proceso se recoge al soltar el cliente.
            if let Some(Slot::Running(run)) = self.slots.get_mut(&key) {
                for (_, pending) in run.pending.drain() {
                    if let Pending::Definition { doc, .. } = pending {
                        out.push(Outcome::Definition {
                            doc,
                            locations: Vec::new(),
                        });
                    }
                }
                out.extend(
                    run.published
                        .drain()
                        .map(|path| Outcome::Diagnostics(path, Vec::new())),
                );
            }
            self.slots.insert(key, Slot::Unavailable(reason));
        }
        out
    }

    /// Cierra todos los servidores sin esperarlos en la hebra de la interfaz.
    pub fn stop_all(&mut self) {
        self.closing.retain(|handle| !handle.is_finished());
        for (_, slot) in self.slots.drain() {
            if let Slot::Running(mut run) = slot {
                self.outbox.extend(
                    run.published
                        .drain()
                        .map(|path| Outcome::Diagnostics(path, Vec::new())),
                );
                self.closing.push(thread::spawn(move || run.client.close()));
            }
        }
        self.open.clear();
    }

    /// Al salir de la aplicación: cierra los servidores y espera su cierre.
    pub fn shutdown(&mut self) {
        self.stop_all();
        for handle in self.closing.drain(..) {
            let _ = handle.join();
        }
    }
}

fn progress(active: &mut BTreeMap<String, String>, params: &Value) {
    let token = params["token"].to_string();
    let value = &params["value"];
    match value["kind"].as_str() {
        Some("begin") | Some("report") => {
            let title = value["title"]
                .as_str()
                .or_else(|| value["message"].as_str());
            let mut label = active.get(&token).cloned().unwrap_or_default();
            if let Some(title) = title.filter(|_| value["kind"] == "begin") {
                label = title.to_string();
            }
            if let Some(message) = value["message"].as_str().filter(|m| !m.is_empty())
                && value["kind"] == "report"
            {
                label = format!("{label} {message}");
            }
            active.insert(token, label);
        }
        Some("end") => {
            active.remove(&token);
        }
        _ => {}
    }
}

fn parse_diagnostics(
    params: &Value,
    uris: &mut HashMap<String, PathBuf>,
) -> Option<(PathBuf, Vec<Problem>)> {
    let path = resolve(params["uri"].as_str()?, uris)?;
    let problems = params["diagnostics"]
        .as_array()?
        .iter()
        .filter_map(|d| {
            let severity = match d["severity"].as_u64().unwrap_or(1) {
                1 => Severity::Error,
                2 => Severity::Warning,
                3 => Severity::Info,
                // Las pistas (código sin usar, sugerencias) no son problemas.
                _ => return None,
            };
            let start = &d["range"]["start"];
            let mut message: String = d["message"].as_str()?.chars().take(600).collect();
            if let Some(source) = d["source"].as_str() {
                message = format!("{message} [{source}]");
            }
            Some(Problem {
                row: start["line"].as_u64()? as usize,
                col: start["character"].as_u64().unwrap_or(0) as u32,
                severity,
                message,
            })
        })
        .collect();
    Some((path, problems))
}

fn parse_locations(result: &Value, uris: &mut HashMap<String, PathBuf>) -> Vec<Location> {
    let items = match result {
        Value::Array(items) => items.iter().collect(),
        Value::Object(_) => vec![result],
        _ => Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| {
            // `Location` o `LocationLink`.
            let (uri, range) = match item.get("targetUri") {
                Some(uri) => (
                    uri,
                    item.get("targetSelectionRange")
                        .or(item.get("targetRange"))?,
                ),
                None => (item.get("uri")?, item.get("range")?),
            };
            Some(Location {
                path: resolve(uri.as_str()?, uris)?,
                row: range["start"]["line"].as_u64()? as usize,
                col: range["start"]["character"].as_u64().unwrap_or(0) as u32,
            })
        })
        .collect()
}

fn resolve(uri: &str, uris: &mut HashMap<String, PathBuf>) -> Option<PathBuf> {
    if let Some(path) = uris.get(uri) {
        return Some(path.clone());
    }
    let path = path_from_uri(uri)?;
    if uris.len() > 4096 {
        uris.clear();
    }
    uris.insert(uri.to_string(), path.clone());
    Some(path)
}

/// Texto de un `Hover`: `MarkupContent`, `MarkedString` o una lista de ellos.
fn hover_text(result: &Value) -> Option<String> {
    fn one(value: &Value) -> Option<String> {
        match value {
            Value::String(text) => Some(text.clone()),
            Value::Object(_) => {
                let text = value["value"].as_str()?;
                Some(match value["language"].as_str() {
                    Some(language) => format!("```{language}\n{text}\n```"),
                    None => text.to_string(),
                })
            }
            _ => None,
        }
    }
    let contents = &result["contents"];
    let text = match contents {
        Value::Array(items) => items
            .iter()
            .filter_map(one)
            .collect::<Vec<_>>()
            .join("\n\n"),
        other => one(other)?,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_contents_come_in_three_shapes() {
        assert_eq!(
            hover_text(&json!({"contents": {"kind": "markdown", "value": " hola "}})),
            Some("hola".into())
        );
        assert_eq!(
            hover_text(&json!({"contents": ["a", {"language": "c", "value": "int x"}]})),
            Some("a\n\n```c\nint x\n```".into())
        );
        assert_eq!(hover_text(&json!({"contents": ""})), None);
        assert_eq!(hover_text(&Value::Null), None);
    }

    #[test]
    fn definition_accepts_locations_and_links() {
        let mut uris = HashMap::new();
        let range =
            json!({"start": {"line": 3, "character": 7}, "end": {"line": 3, "character": 9}});
        let one = json!({"uri": "file:///tmp/a.c", "range": range});
        let link = json!([{"targetUri": "file:///tmp/b.c", "targetRange": range, "targetSelectionRange": range}]);
        assert_eq!(parse_locations(&one, &mut uris).len(), 1);
        let got = parse_locations(&link, &mut uris);
        assert_eq!((got[0].row, got[0].col), (3, 7));
        assert!(parse_locations(&Value::Null, &mut uris).is_empty());
    }

    #[test]
    fn diagnostics_drop_hints_and_keep_severity() {
        let mut uris = HashMap::new();
        let at = |line| json!({"start": {"line": line, "character": 2}, "end": {"line": line, "character": 3}});
        let params = json!({"uri": "file:///tmp/a.c", "diagnostics": [
            {"range": at(0), "severity": 1, "message": "malo"},
            {"range": at(1), "severity": 2, "message": "dudoso", "source": "clang"},
            {"range": at(2), "severity": 4, "message": "pista"},
        ]});
        let (_, problems) = parse_diagnostics(&params, &mut uris).unwrap();
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].severity, Severity::Error);
        assert_eq!(problems[1].message, "dudoso [clang]");
    }

    #[test]
    fn progress_tracks_begin_report_end() {
        let mut active = BTreeMap::new();
        progress(
            &mut active,
            &json!({"token": "t", "value": {"kind": "begin", "title": "Indexing"}}),
        );
        assert_eq!(active.get("\"t\"").map(String::as_str), Some("Indexing"));
        progress(
            &mut active,
            &json!({"token": "t", "value": {"kind": "report", "message": "3/9"}}),
        );
        assert_eq!(
            active.get("\"t\"").map(String::as_str),
            Some("Indexing 3/9")
        );
        progress(
            &mut active,
            &json!({"token": "t", "value": {"kind": "end"}}),
        );
        assert!(active.is_empty());
    }
}
