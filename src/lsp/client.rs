//! Un servidor de lenguaje como proceso hijo. Un hilo lee su salida, otro
//! escribe su entrada y otro recoge sus errores; la interfaz solo consulta
//! el canal de eventos, sin esperar nunca.

use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use super::{
    rpc::{self, DecodeError, Decoder, Message},
    servers::file_uri,
};

/// Avisa a la interfaz de que hay algo que leer.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

const INITIALIZE: i64 = 1;
/// Mensajes que le importan a la interfaz; el resto se descarta en el hilo lector.
const FORWARDED: [&str; 2] = ["textDocument/publishDiagnostics", "$/progress"];

pub enum Event {
    Ready,
    Notification(String, Value),
    Response(i64, Result<Value, String>),
    /// El servidor terminó o dejó de hablar LSP; con lo último que dijo por stderr.
    Exited(String),
}

#[derive(Clone, Copy, Default)]
pub struct Capabilities {
    pub definition: bool,
    pub hover: bool,
    /// 0 sin sincronizar, 1 texto completo, 2 incremental.
    pub sync: u8,
}

impl Capabilities {
    fn parse(result: &Value) -> Self {
        let caps = &result["capabilities"];
        let provided = |name: &str| {
            let value = &caps[name];
            !value.is_null() && value.as_bool() != Some(false)
        };
        let sync = &caps["textDocumentSync"];
        Self {
            definition: provided("definitionProvider"),
            hover: provided("hoverProvider"),
            sync: sync
                .as_u64()
                .or_else(|| sync["change"].as_u64())
                .map_or(1, |n| n as u8),
        }
    }
}

pub struct Client {
    child: Child,
    out: Sender<Vec<u8>>,
    events: Receiver<Event>,
    next_id: i64,
    ready: bool,
    dead: bool,
    /// Cierre pedido por nosotros: que el fin del proceso no se registre como fallo.
    closing: Arc<AtomicBool>,
    /// Notificaciones que esperan a que el servidor responda a `initialize`.
    queued: Vec<Value>,
    pub capabilities: Capabilities,
}

impl Client {
    pub fn start(program: &Path, args: &[String], root: &Path, wake: Wake) -> Result<Self, String> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("no pude lanzar {}: {e}", program.display()))?;
        let (stdin, mut stdout, mut stderr) = (
            child.stdin.take().unwrap(),
            child.stdout.take().unwrap(),
            child.stderr.take().unwrap(),
        );
        let (out, to_write) = mpsc::channel::<Vec<u8>>();
        let (events_tx, events) = mpsc::channel();
        thread::spawn(move || {
            let mut stdin = stdin;
            while let Ok(bytes) = to_write.recv() {
                if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                    break;
                }
            }
        });
        let tail = Arc::new(Mutex::new(String::new()));
        let tail_writer = tail.clone();
        let errors = thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(n @ 1..) = stderr.read(&mut buffer) {
                let mut tail = tail_writer.lock().unwrap();
                tail.push_str(&String::from_utf8_lossy(&buffer[..n]));
                let excess = tail.len().saturating_sub(1500);
                if excess > 0 {
                    let cut = (excess..).find(|i| tail.is_char_boundary(*i)).unwrap();
                    tail.drain(..cut);
                }
            }
        });
        let replies = out.clone();
        let closing = Arc::new(AtomicBool::new(false));
        let quiet = closing.clone();
        thread::spawn(move || {
            let mut decoder = Decoder::default();
            let mut buffer = vec![0u8; 64 * 1024];
            let reason = 'read: loop {
                let n = match stdout.read(&mut buffer) {
                    Ok(0) => break "terminó".to_string(),
                    Ok(n) => n,
                    Err(e) => break format!("error de lectura: {e}"),
                };
                decoder.push(&buffer[..n]);
                loop {
                    let value = match decoder.next() {
                        Ok(Some(value)) => value,
                        Ok(None) => break,
                        Err(DecodeError::Skipped(e)) => {
                            eprintln!("lsp: mensaje ilegible ({e})");
                            continue;
                        }
                        Err(DecodeError::Fatal(e)) => break 'read format!("salida inválida: {e}"),
                    };
                    let Some(message) = rpc::classify(value) else {
                        continue;
                    };
                    let event = match message {
                        Message::Request { id, method, params } => {
                            let _ = replies.send(rpc::encode(&answer(id, &method, &params)));
                            continue;
                        }
                        Message::Notification { method, params } => {
                            if !FORWARDED.contains(&method.as_str()) {
                                continue;
                            }
                            Event::Notification(method, params)
                        }
                        Message::Response { id, result } => Event::Response(id, result),
                    };
                    if events_tx.send(event).is_err() {
                        return;
                    }
                    wake();
                }
            };
            // stderr suele llegar junto con el cierre: se le da un momento.
            let waited = Instant::now();
            while !errors.is_finished() && waited.elapsed() < Duration::from_millis(300) {
                thread::sleep(Duration::from_millis(10));
            }
            let tail = tail.lock().unwrap().trim().to_string();
            let reason = if tail.is_empty() {
                reason
            } else {
                format!("{reason}: {}", tail.lines().last().unwrap_or_default())
            };
            if !quiet.load(Ordering::Relaxed) {
                eprintln!("lsp: el servidor ya no responde ({reason})");
            }
            let _ = events_tx.send(Event::Exited(reason));
            wake();
        });
        let mut client = Self {
            child,
            out,
            events,
            next_id: INITIALIZE,
            ready: false,
            dead: false,
            closing,
            queued: Vec::new(),
            capabilities: Capabilities::default(),
        };
        client.send(&rpc::request(
            INITIALIZE,
            "initialize",
            initialize_params(root),
        ));
        client.next_id += 1;
        Ok(client)
    }

    fn send(&mut self, message: &Value) {
        let _ = self.out.send(rpc::encode(message));
    }

    pub fn ready(&self) -> bool {
        self.ready && !self.dead
    }

    /// Notificación; si el servidor aún no está listo espera su turno.
    pub fn notify(&mut self, method: &str, params: Value) {
        if self.dead {
            return;
        }
        let message = rpc::notification(method, params);
        if self.ready {
            self.send(&message);
        } else {
            self.queued.push(message);
        }
    }

    /// Envía una petición y devuelve su número; la respuesta llega como evento.
    pub fn request(&mut self, method: &str, params: Value) -> Option<i64> {
        if !self.ready() {
            return None;
        }
        self.next_id += 1;
        let id = self.next_id;
        self.send(&rpc::request(id, method, params));
        Some(id)
    }

    /// Eventos pendientes, sin esperar.
    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Response(INITIALIZE, result) => match result {
                    Ok(result) => {
                        self.capabilities = Capabilities::parse(&result);
                        self.ready = true;
                        self.send(&rpc::notification("initialized", json!({})));
                        for message in std::mem::take(&mut self.queued) {
                            self.send(&message);
                        }
                        events.push(Event::Ready);
                    }
                    Err(e) => {
                        self.dead = true;
                        events.push(Event::Exited(format!("initialize falló: {e}")));
                    }
                },
                Event::Exited(reason) => {
                    self.dead = true;
                    events.push(Event::Exited(reason));
                }
                event => events.push(event),
            }
        }
        events
    }

    /// Cierre ordenado (`shutdown` y `exit`) con plazos cortos; bloquea hasta
    /// ~1,2 s, así que se llama desde un hilo.
    pub fn close(mut self) {
        self.closing.store(true, Ordering::Relaxed);
        if self.ready() {
            self.next_id += 1;
            let id = self.next_id;
            self.send(&rpc::request(id, "shutdown", Value::Null));
            let limit = Instant::now() + Duration::from_millis(700);
            while let Some(left) = limit.checked_duration_since(Instant::now()) {
                match self.events.recv_timeout(left) {
                    Ok(Event::Response(got, _)) if got == id => break,
                    Ok(Event::Exited(_)) | Err(RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
            }
            self.send(&rpc::notification("exit", Value::Null));
            let limit = Instant::now() + Duration::from_millis(500);
            while Instant::now() < limit && matches!(self.child.try_wait(), Ok(None)) {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// Respuesta a una petición del servidor. Se contesta siempre: algunos
/// servidores se quedan esperando.
fn answer(id: Value, method: &str, params: &Value) -> Value {
    match method {
        "workspace/configuration" => {
            let count = params["items"].as_array().map_or(0, Vec::len);
            rpc::reply(id, Value::Array(vec![Value::Null; count]))
        }
        "window/workDoneProgress/create"
        | "client/registerCapability"
        | "client/unregisterCapability"
        | "workspace/workspaceFolders" => rpc::reply(id, Value::Null),
        _ => rpc::reply_error(id, -32601, "método no admitido"),
    }
}

fn initialize_params(root: &Path) -> Value {
    let uri = file_uri(root);
    let name = root.file_name().unwrap_or_default().to_string_lossy();
    json!({
        "processId": std::process::id(),
        "clientInfo": {"name": "MiyuLaTeX", "version": env!("CARGO_PKG_VERSION")},
        "rootUri": uri,
        "rootPath": root.to_string_lossy(),
        "workspaceFolders": [{"uri": uri, "name": name}],
        "capabilities": {
            "general": {"positionEncodings": ["utf-16"]},
            "window": {"workDoneProgress": true},
            "textDocument": {
                "synchronization": {"didSave": true},
                "publishDiagnostics": {"relatedInformation": false},
                "definition": {"linkSupport": true},
                "hover": {"contentFormat": ["markdown", "plaintext"]}
            }
        }
    })
}
