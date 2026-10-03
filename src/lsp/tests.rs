//! Pruebas con un servidor falso (un script de Python 3) y, si está
//! instalado, con clangd de verdad.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{
    hub::{DEFINITION_TIMEOUT, Problem},
    *,
};
use crate::{compiler, editor::Pos};

/// Servidor mínimo; `mode` decide cómo se porta.
const FAKE: &str = r#"
import sys, json, os, time
mode = sys.argv[1]
if mode == "garbage":
    sys.stdout.write("esto no es LSP\r\n\r\n"); sys.stdout.flush(); time.sleep(30); sys.exit()
if mode == "die":
    sys.stderr.write("fallo de arranque\n"); sys.exit(3)
def read():
    n = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line: return None
        line = line.strip()
        if not line: break
        k, v = line.decode().split(":", 1)
        if k.lower() == "content-length": n = int(v)
    return json.loads(sys.stdin.buffer.read(n))
def send(m):
    m["jsonrpc"] = "2.0"
    b = json.dumps(m).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(b) + b); sys.stdout.buffer.flush()
def u16(s): return len(s.encode("utf-16-le")) // 2
def publish(uri, text):
    ds = []
    for i, line in enumerate(text.split("\n")):
        j = line.find("ERR")
        if j >= 0:
            c = u16(line[:j])
            ds.append({"range": {"start": {"line": i, "character": c}, "end": {"line": i, "character": c + 3}},
                       "severity": 1 if "WARN" not in line else 2, "message": "hay un ERR"})
    send({"method": "textDocument/publishDiagnostics", "params": {"uri": uri, "diagnostics": ds}})
docs = {}
while True:
    m = read()
    if m is None: break
    method = m.get("method")
    p = m.get("params") or {}
    if method == "initialize":
        send({"id": m["id"], "result": {"capabilities": {"textDocumentSync": 1, "definitionProvider": True, "hoverProvider": True}}})
    elif method == "initialized":
        send({"id": 900, "method": "window/workDoneProgress/create", "params": {"token": "idx"}})
        send({"method": "$/progress", "params": {"token": "idx", "value": {"kind": "begin", "title": "Indexing"}}})
    elif method == "textDocument/didOpen":
        d = p["textDocument"]; docs[d["uri"]] = d["text"]; publish(d["uri"], d["text"])
    elif method == "textDocument/didChange":
        u = p["textDocument"]["uri"]; docs[u] = p["contentChanges"][0]["text"]; publish(u, docs[u])
    elif method == "textDocument/didSave":
        open("saved.txt", "w").write(p["textDocument"]["uri"])
    elif method == "textDocument/definition":
        if mode == "silent": continue
        u = p["textDocument"]["uri"]
        for i, line in enumerate(docs[u].split("\n")):
            j = line.find("foo =")
            if j >= 0:
                c = u16(line[:j])
                send({"id": m["id"], "result": [{"uri": u, "range": {"start": {"line": i, "character": c}, "end": {"line": i, "character": c + 3}}}]})
                break
        else:
            send({"id": m["id"], "result": None})
    elif method == "textDocument/hover":
        pos = p["position"]
        send({"method": "$/progress", "params": {"token": "idx", "value": {"kind": "end"}}})
        send({"id": m["id"], "result": {"contents": {"kind": "markdown", "value": "L%d:C%d" % (pos["line"], pos["character"])}}})
    elif method == "shutdown":
        send({"id": m["id"], "result": None})
    elif method == "exit":
        open("exited.txt", "w").write("ok"); break
"#;

pub(crate) fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("miyu-lsp-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

pub(crate) fn fake_table(dir: &Path, mode: &str) -> Option<Vec<Spec>> {
    let python = compiler::which("python3")?;
    let script = dir.join("fake_server.py");
    fs::write(&script, FAKE).unwrap();
    Some(vec![Spec {
        name: "fake".into(),
        candidates: vec![Launch {
            program: python.to_string_lossy().into(),
            args: vec![script.to_string_lossy().into(), mode.into()],
        }],
        markers: Vec::new(),
        languages: vec![("fake".into(), "fake".into())],
    }])
}

/// Mantiene la sincronización y recoge resultados hasta que `done` se cumpla.
fn pump(
    hub: &mut Hub,
    project: &Path,
    docs: &[DocRef],
    got: &mut Vec<Outcome>,
    done: impl Fn(&[Outcome], &Hub) -> bool,
) {
    let limit = Instant::now() + Duration::from_secs(20);
    while Instant::now() < limit {
        hub.sync(true, project, docs);
        got.extend(hub.poll());
        if done(got, hub) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("se agotó la espera; llegó: {got:?}");
}

fn diagnostics<'a>(got: &'a [Outcome], path: &Path) -> Option<&'a Vec<Problem>> {
    got.iter().rev().find_map(|o| match o {
        Outcome::Diagnostics(p, problems) if p == path => Some(problems),
        _ => None,
    })
}

#[test]
fn fake_server_diagnostics_definition_hover_save_and_exit() {
    let dir = folder("fake");
    let Some(table) = fake_table(&dir, "normal") else {
        return;
    };
    let path = dir.join("a.fake");
    let text = "é😀 ERR\n😀foo = 1\nfoo\n";
    fs::write(&path, text).unwrap();
    let mut hub = Hub::new(table);
    let mut got = Vec::new();
    let doc = |revision: u64, source: &'static str| DocRef {
        key: 7,
        path: Some(&path),
        revision,
        source,
    };
    let docs = [doc(1, text)];
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        diagnostics(got, &path).is_some()
    });
    // El servidor cuenta en UTF-16: «ERR» está en la unidad 4, el carácter 3.
    let problem = &diagnostics(&got, &path).unwrap()[0];
    assert_eq!(
        (problem.row, problem.col, problem.severity),
        (0, 4, Severity::Error)
    );
    assert_eq!(char_col("é😀 ERR", problem.col), 3);

    // Mientras indexa (progreso abierto) el estado lo dice; al terminar, listo.
    pump(&mut hub, &dir, &docs, &mut got, |_, hub| {
        matches!(hub.status(7).map(|s| s.state), Some(State::Busy(_)))
    });
    assert_eq!(hub.status(7).unwrap().server, "fake");
    assert!(hub.hover_ready(7));
    // Posición del editor (fila 0, carácter 3) -> UTF-16 (4).
    assert!(hub.hover(&docs[0], "é😀 ERR", Pos::new(0, 3)));
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        got.iter().any(|o| matches!(o, Outcome::Hover { .. }))
    });
    let hover = got.iter().find_map(|o| match o {
        Outcome::Hover { text, pos, .. } => Some((text.clone(), *pos)),
        _ => None,
    });
    assert_eq!(hover, Some((Some("L0:C4".into()), Pos::new(0, 3))));
    assert_eq!(hub.status(7).unwrap().state, State::Ready);

    // «foo =» está tras un emoji: unidad 2, carácter 1.
    assert!(hub.definition(&docs[0], "foo", Pos::new(2, 1)));
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        got.iter().any(|o| matches!(o, Outcome::Definition { .. }))
    });
    let Some(Outcome::Definition { locations, .. }) =
        got.iter().find(|o| matches!(o, Outcome::Definition { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (
            locations[0].path.clone(),
            locations[0].row,
            locations[0].col
        ),
        (path.clone(), 1, 2)
    );
    assert_eq!(char_col("😀foo = 1", locations[0].col), 1);

    // Un cambio llega tras la pausa y el servidor republica sin el error.
    let edited = "sin errores\n😀foo = 1\n";
    let docs = [doc(2, edited)];
    got.clear();
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        diagnostics(got, &path).is_some_and(Vec::is_empty)
    });

    hub.saved(&docs[0]);
    let limit = Instant::now() + Duration::from_secs(10);
    while !dir.join("saved.txt").exists() && Instant::now() < limit {
        hub.sync(true, &dir, &docs);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        fs::read_to_string(dir.join("saved.txt"))
            .unwrap()
            .ends_with("a.fake")
    );

    hub.shutdown();
    assert!(
        dir.join("exited.txt").exists(),
        "el servidor no recibió exit"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn closing_a_document_clears_its_diagnostics() {
    let dir = folder("close");
    let Some(table) = fake_table(&dir, "normal") else {
        return;
    };
    let path = dir.join("a.fake");
    let mut hub = Hub::new(table);
    let mut got = Vec::new();
    let docs = [DocRef {
        key: 1,
        path: Some(&path),
        revision: 1,
        source: "ERR\n",
    }];
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        diagnostics(got, &path).is_some()
    });
    got.clear();
    pump(&mut hub, &dir, &[], &mut got, |got, _| {
        diagnostics(got, &path).is_some_and(Vec::is_empty)
    });
    assert!(hub.status(1).is_none());
    hub.shutdown();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_server_that_writes_garbage_or_dies_means_no_server() {
    for (mode, expected) in [("garbage", "salida inválida"), ("die", "fallo de arranque")] {
        let dir = folder(mode);
        let Some(table) = fake_table(&dir, mode) else {
            return;
        };
        let path = dir.join("a.fake");
        let mut hub = Hub::new(table);
        let mut got = Vec::new();
        let docs = [DocRef {
            key: 1,
            path: Some(&path),
            revision: 1,
            source: "x\n",
        }];
        pump(&mut hub, &dir, &docs, &mut got, |_, hub| {
            matches!(hub.status(1).map(|s| s.state), Some(State::Unavailable(_)))
        });
        let Some(State::Unavailable(reason)) = hub.status(1).map(|s| s.state) else {
            unreachable!()
        };
        assert!(reason.contains(expected), "{mode}: {reason}");
        assert!(!hub.hover_ready(1));
        assert!(!hub.definition(&docs[0], "x", Pos::new(0, 0)));
        hub.shutdown();
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_missing_executable_is_no_server_without_failing() {
    let dir = folder("missing");
    let spec = Spec {
        name: "inexistente".into(),
        candidates: vec![Launch {
            program: "miyu-servidor-que-no-existe".into(),
            args: Vec::new(),
        }],
        markers: Vec::new(),
        languages: vec![("fake".into(), "fake".into())],
    };
    let path = dir.join("a.fake");
    let mut hub = Hub::new(vec![spec]);
    let docs = [DocRef {
        key: 1,
        path: Some(&path),
        revision: 1,
        source: "x\n",
    }];
    hub.sync(true, &dir, &docs);
    let status = hub.status(1).unwrap();
    assert!(matches!(status.state, State::Unavailable(r) if r.contains("no está instalado")));
    // Un archivo de un lenguaje sin servidor no tiene estado.
    let other = dir.join("a.zzz");
    hub.sync(
        true,
        &dir,
        &[DocRef {
            key: 2,
            path: Some(&other),
            revision: 1,
            source: "",
        }],
    );
    assert!(hub.status(2).is_none());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_definition_the_server_never_answers_times_out_empty() {
    let dir = folder("silent");
    let Some(table) = fake_table(&dir, "silent") else {
        return;
    };
    let path = dir.join("a.fake");
    let mut hub = Hub::new(table);
    let mut got = Vec::new();
    let docs = [DocRef {
        key: 1,
        path: Some(&path),
        revision: 1,
        source: "foo = 1\n",
    }];
    pump(&mut hub, &dir, &docs, &mut got, |_, hub| hub.hover_ready(1));
    let started = Instant::now();
    assert!(hub.definition(&docs[0], "foo = 1", Pos::new(0, 0)));
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        got.iter().any(|o| matches!(o, Outcome::Definition { .. }))
    });
    assert!(started.elapsed() >= DEFINITION_TIMEOUT);
    assert!(matches!(
        got.last(),
        Some(Outcome::Definition { locations, .. }) if locations.is_empty()
    ));
    hub.shutdown();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn clangd_reports_an_error_and_finds_a_definition() {
    if compiler::which("clangd").is_none() {
        eprintln!("clangd no está instalado: se omite");
        return;
    }
    let dir = folder("clangd");
    let path = dir.join("prueba.c");
    let text = "int suma(int a, int b) { return a + b; }\nint main(void) {\n    int x = suma(1, 2)\n    return x;\n}\n";
    fs::write(&path, text).unwrap();
    let mut hub = Hub::standard();
    let mut got = Vec::new();
    let docs = [DocRef {
        key: 1,
        path: Some(&path),
        revision: 1,
        source: text,
    }];
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        diagnostics(got, &path).is_some_and(|d| !d.is_empty())
    });
    let problems = diagnostics(&got, &path).unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p.severity == Severity::Error && (2..=3).contains(&p.row)),
        "{problems:?}"
    );
    // Falta el «;» de la fila 2; clangd lo señala al llegar a la siguiente.
    assert_eq!(hub.status(1).unwrap().server, "clangd");
    // `suma` en la llamada (fila 2, carácter 12) se define en la fila 0.
    assert!(hub.definition(&docs[0], "    int x = suma(1, 2)", Pos::new(2, 13)));
    pump(&mut hub, &dir, &docs, &mut got, |got, _| {
        got.iter().any(|o| matches!(o, Outcome::Definition { .. }))
    });
    let Some(Outcome::Definition { locations, .. }) =
        got.iter().find(|o| matches!(o, Outcome::Definition { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        (locations[0].path.clone(), locations[0].row),
        (path.clone(), 0)
    );
    hub.shutdown();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rust_analyzer_here_is_either_a_server_or_no_server() {
    let dir = folder("rust");
    let path = dir.join("lib.rs");
    let mut hub = Hub::standard();
    let docs = [DocRef {
        key: 1,
        path: Some(&path),
        revision: 1,
        source: "fn main() {}\n",
    }];
    let mut got = Vec::new();
    // Si es solo el proxy de rustup, termina enseguida: sin servidor.
    pump(&mut hub, &dir, &docs, &mut got, |_, hub| {
        !matches!(hub.status(1).map(|s| s.state), Some(State::Starting))
    });
    let status = hub.status(1).unwrap();
    eprintln!("rust-analyzer: {status:?}");
    assert_eq!(status.server, "rust-analyzer");
    hub.shutdown();
    fs::remove_dir_all(dir).unwrap();
}
