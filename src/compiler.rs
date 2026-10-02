use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::{editor::regex, latex, synctex};

pub const ENGINES: &[&str] = &["tectonic", "latexmk", "pdflatex", "xelatex", "lualatex"];
pub const AUX: &[&str] = &[
    "aux",
    "log",
    "out",
    "toc",
    "lof",
    "lot",
    "fls",
    "fdb_latexmk",
    "synctex.gz",
    "bbl",
    "blg",
    "nav",
    "snm",
    "vrb",
    "bcf",
    "run.xml",
    "xdv",
];
#[derive(Clone, Debug)]
pub struct Problem {
    pub severity: String,
    pub message: String,
    pub file: PathBuf,
    pub line: Option<usize>,
}
pub struct CompileResult {
    pub ok: bool,
    pub engine: String,
    pub root: PathBuf,
    pub pdf: Option<PathBuf>,
    pub duration: f64,
    pub output: String,
    pub problems: Vec<Problem>,
}

/// Carpeta con herramientas propias de Miyu, como el biber que exige Tectonic.
pub fn tools() -> PathBuf {
    crate::config::directory().join("bin")
}

fn search_paths() -> Vec<PathBuf> {
    let mut paths: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    paths.extend(["/Library/TeX/texbin", "/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    paths.extend([home.join(".cargo/bin"), home.join(".local/bin")]);
    paths
}

pub fn which(program: &str) -> Option<PathBuf> {
    search_paths()
        .into_iter()
        .map(|p| p.join(program))
        .find(|p| {
            let Ok(meta) = p.metadata() else {
                return false;
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                meta.is_file() && meta.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                meta.is_file()
            }
        })
}

pub fn engines() -> Vec<(String, PathBuf)> {
    ENGINES
        .iter()
        .filter_map(|e| which(e).map(|p| ((*e).into(), p)))
        .collect()
}

/// `\documentclass` fuera de comentarios y de código literal.
fn declares_class(text: &str) -> bool {
    // Suele estar al principio: basta limpiar el texto hasta su línea.
    text.match_indices("\\documentclass").any(|(at, _)| {
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        latex::code(&text[..end])
            .rsplit('\n')
            .next()
            .is_some_and(|line| line.contains("\\documentclass"))
    })
}

/// Lo único del texto de lo que depende `find_root`: la directiva
/// `% !TEX root` y si declara una clase. Mientras no cambie, el archivo
/// principal es el mismo y no hace falta volver a buscarlo en disco.
pub fn root_signature(text: &str) -> (Option<String>, bool) {
    let end = text.char_indices().nth(4000).map_or(text.len(), |(i, _)| i);
    let directive = regex(r"(?im)^\s*%+\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$")
        .captures(&text[..end])
        .map(|m| m[1].to_string());
    (directive, declares_class(text))
}

pub fn find_root(path: &Path, text: &str) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut current = path.to_path_buf();
    let mut content = text.to_owned();
    let mut visited = vec![current.clone()];
    for _ in 0..16 {
        let prefix: String = content.chars().take(4000).collect();
        let Some(m) = regex(r"(?im)^\s*%+\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$").captures(&prefix)
        else {
            break;
        };
        let Ok(root) = current
            .parent()
            .unwrap_or(parent)
            .join(m[1].trim().trim_matches('"'))
            .canonicalize()
        else {
            break;
        };
        if !root.is_file() || visited.contains(&root) {
            break;
        }
        content = fs::read_to_string(&root).unwrap_or_default();
        current = root.clone();
        visited.push(root);
    }
    if visited.len() > 1 || declares_class(text) {
        return current;
    }
    for directory in parent.ancestors().take(12) {
        for name in ["main.tex", "principal.tex", "tesis.tex", "thesis.tex"] {
            let p = directory.join(name);
            if p.is_file() {
                return p;
            }
        }
        if let Ok(files) = fs::read_dir(directory) {
            let mut files: Vec<_> = files
                .flatten()
                .map(|f| f.path())
                .filter(|p| p.extension().is_some_and(|e| e == "tex"))
                .collect();
            files.sort();
            for file in files {
                if fs::read_to_string(&file)
                    .is_ok_and(|s| latex::code(&s).contains("\\documentclass"))
                {
                    return file;
                }
            }
        }
    }
    path.into()
}

pub fn select_engine(root: &Path, preference: &str) -> Result<(String, PathBuf), String> {
    let text = fs::read_to_string(root).unwrap_or_default();
    let requested = regex(r"(?im)^\s*%+\s*!\s*TEX\s+(?:TS-)?program\s*=\s*(\S+)\s*$")
        .captures(&text)
        .map(|m| m[1].to_lowercase());
    let engine = if preference.is_empty() || preference == "auto" {
        requested.as_deref().unwrap_or("auto")
    } else {
        preference
    };
    if engine == "auto" {
        engines().into_iter().next().ok_or_else(|| {
            "No encontré un motor LaTeX. Instala Tectonic o una distribución TeX.".into()
        })
    } else if !ENGINES.contains(&engine) {
        Err(format!("Motor LaTeX desconocido: {engine}"))
    } else {
        which(engine).map(|path| (engine.into(), path)).ok_or_else(|| format!("El documento necesita {engine}, pero no está instalado. Cambia el motor o instálalo."))
    }
}

pub fn build_command(engine: &str, executable: &Path, root: &Path) -> Command {
    let mut cmd = Command::new(executable);
    // Una app abierta desde Finder hereda un PATH mínimo. El motor busca ahí
    // sus herramientas externas, como biber.
    let mut paths = search_paths();
    if engine == "tectonic" {
        // El biber de Miyu va primero: Tectonic exige una versión exacta.
        paths.insert(0, tools());
        cmd.args(["-X", "compile", "--keep-logs", "--synctex"]);
    } else {
        if engine == "latexmk" {
            cmd.arg("-pdf");
        }
        cmd.args(["-interaction=nonstopmode", "-file-line-error", "-synctex=1"]);
    }
    if let Ok(path) = std::env::join_paths(paths) {
        cmd.env("PATH", path);
    }
    cmd.arg(root.file_name().unwrap())
        .current_dir(root.parent().unwrap_or(Path::new(".")));
    cmd
}

/// Archivo del proyecto que contiene `key` en la línea `number`. El registro
/// de LaTeX da la línea de una cita sin resolver, pero no el archivo.
fn locate(root: &Path, key: &str, number: usize) -> Option<PathBuf> {
    let directory = root.parent()?.canonicalize().ok()?;
    for source in latex::sources(root, &[]) {
        if source
            .text
            .lines()
            .nth(number.saturating_sub(1))
            .is_some_and(|l| l.contains(key))
        {
            return source.path.strip_prefix(&directory).ok().map(PathBuf::from);
        }
    }
    None
}

pub fn parse_output(output: &str, root: &Path) -> Vec<Problem> {
    let mut problems: Vec<Problem> = Vec::new();
    let lines: Vec<_> = output.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let mut severity = "error".to_string();
        let mut file = root.file_name().map(PathBuf::from).unwrap_or_default();
        let mut number = None;
        let message = if let Some(m) =
            regex(r"^(error|warning): (?:([^\s:]+\.\w+):(?:(\d+):)?\s*)?(.*)$").captures(line)
        {
            severity = m[1].into();
            if let Some(f) = m.get(2) {
                file = f.as_str().into();
            }
            number = m.get(3).and_then(|v| v.as_str().parse().ok());
            let mut msg = m[4].to_string();
            if let Some(m) =
                regex(r"^(?:LaTeX|Package|Class)(?: (\S+))? Warning: (.*)$").captures(&msg)
            {
                msg = m[2].into();
            }
            if [
                "halted on potentially-recoverable",
                "the Tectonic backend",
                "something bad happened inside",
                "engine had an unrecoverable error",
                "could not represent character",
                "you may need to load the `fontspec`",
                "choose a different font that covers",
                "the external tool exited",
                "its stderr was",
            ]
            .iter()
            .any(|s| msg.contains(s))
            {
                continue;
            }
            msg
        } else if let Some(m) =
            regex(r"^([^:]+\.(?:tex|sty|cls|bib|ltx|tikz)):(\d+): (.*)$").captures(line)
        {
            file = m[1].into();
            number = m[2].parse().ok();
            m[3].to_string()
        } else if let Some(m) =
            regex(r"^(?:\[\d+\] \S+ )?(WARN|ERROR) - (?:Error: )?(.*)$").captures(line)
        {
            if &m[1] == "WARN" {
                severity = "warning".into();
            }
            if m[2].contains("control file version") {
                format!(
                    "biber: {} Tectonic 0.17 necesita biber 2.17 en {}",
                    &m[2],
                    tools().display()
                )
            } else {
                format!("biber: {}", &m[2])
            }
        } else if let Some(msg) = line.strip_prefix("! ") {
            for ahead in lines[i..].iter().take(12) {
                if let Some(m) = regex(r"^l\.(\d+) ").captures(ahead) {
                    number = m[1].parse().ok();
                    break;
                }
            }
            msg.into()
        } else if let Some(m) =
            regex(r"^(?:LaTeX|Package|Class)(?: (\S+))? Warning: (.*)$").captures(line)
        {
            severity = "warning".into();
            let mut msg = m[2].to_string();
            while i < lines.len()
                && !lines[i].trim().is_empty()
                && !msg.trim_end().ends_with('.')
                && msg.len() < 300
            {
                msg.push(' ');
                msg.push_str(regex(r"^\(\S+\)\s+").replace(lines[i], "").trim());
                i += 1;
            }
            msg
        } else if let Some(m) =
            regex(r"^((?:Over|Under)full \\[hv]box .*?) (?:in paragraph )?at lines? (\d+)")
                .captures(line)
        {
            severity = "info".into();
            number = m[2].parse().ok();
            m[1].into()
        } else {
            continue;
        };
        let message = message.trim().trim_start_matches('!').trim().to_string();
        if message.is_empty()
            || [
                "Rerun to get",
                "There were undefined references",
                "Label(s) may have changed",
                "Please (re)run Biber",
            ]
            .iter()
            .any(|s| message.contains(s))
        {
            continue;
        }
        if message.contains("nullfont") || message.contains("accessing absolute path") {
            severity = "info".into();
        }
        if number.is_none() {
            number = regex(r"on input line (\d+)")
                .captures(&message)
                .and_then(|m| m[1].parse().ok());
        }
        if let Some(n) = number
            && let Some(m) = regex(r"^(?:Citation|Reference) [`']([^']+)'").captures(&message)
            && let Some(found) = locate(root, &m[1], n)
        {
            file = found;
        }
        if let Some(p) = problems.iter_mut().find(|p| {
            p.severity == severity
                && p.message.trim_end_matches('.') == message.trim_end_matches('.')
                && (p.line.is_none() || number.is_none() || (p.file == file && p.line == number))
        }) {
            if p.line.is_none() && number.is_some() {
                p.line = number;
                p.file = file;
            }
            continue;
        }
        problems.push(Problem {
            severity,
            message,
            file,
            line: number,
        });
    }
    problems.sort_by_key(|p| match p.severity.as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    });
    problems
}

fn capture<R: Read + Send + 'static>(mut pipe: R) -> thread::JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buf = [0; 8192];
        loop {
            let n = pipe.read(&mut buf)?;
            if n == 0 {
                break;
            }
            if bytes.len() < 8 * 1024 * 1024 {
                bytes.extend_from_slice(&buf[..n]);
            }
        }
        Ok(bytes)
    })
}

fn run(mut cmd: Command, deadline: Instant, cancel: &AtomicBool) -> io::Result<(i32, String)> {
    if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
        return Ok((124, "error: Operación cancelada o tiempo agotado".into()));
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let out = capture(child.stdout.take().unwrap());
    let err = capture(child.stderr.take().unwrap());
    let mut timed_out = false;
    let code = loop {
        if let Some(status) = child.try_wait()? {
            break status.code().unwrap_or(1);
        }
        if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
            #[cfg(unix)]
            {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .status();
            }
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break 124;
        }
        thread::sleep(Duration::from_millis(30));
    };
    let mut output = String::from_utf8_lossy(
        &out.join()
            .map_err(|_| io::Error::other("Falló la lectura del motor"))??,
    )
    .into_owned();
    output.push('\n');
    output.push_str(&String::from_utf8_lossy(
        &err.join()
            .map_err(|_| io::Error::other("Falló la lectura del motor"))??,
    ));
    if timed_out {
        output.push_str("\nerror: Compilación cancelada o superior a 240 s\n");
    }
    Ok((code, output))
}

pub fn compile(
    root: PathBuf,
    engine: String,
    executable: PathBuf,
    cancel: Arc<AtomicBool>,
) -> io::Result<CompileResult> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(240);
    let pdf = root.with_extension("pdf");
    let before = fs::metadata(&pdf).and_then(|m| m.modified()).ok();
    let (mut code, mut output) = run(
        build_command(&engine, &executable, &root),
        deadline,
        &cancel,
    )?;
    let mut diagnostics = output.clone();
    for extension in ["log", "blg"] {
        if engine == "tectonic"
            && let Ok(bytes) = fs::read(root.with_extension(extension))
        {
            diagnostics.push('\n');
            diagnostics.push_str(&String::from_utf8_lossy(&bytes));
        }
    }
    if engine == "tectonic"
        && code != 0
        && output.contains("Running external tool biber")
        && output.contains("(os error 2)")
    {
        diagnostics = diagnostics.replace(
            "error: No such file or directory (os error 2)",
            &format!(
                "error: No encontré biber. Tectonic 0.17 necesita biber 2.17 en {} o en el PATH",
                tools().display()
            ),
        );
    }
    if ["pdflatex", "xelatex", "lualatex"].contains(&engine.as_str()) && code == 0 {
        let aux = fs::read_to_string(root.with_extension("aux")).unwrap_or_default();
        let bibliography = if root.with_extension("bcf").exists() {
            Some("biber")
        } else if aux.contains("\\bibdata") && aux.contains("\\citation") {
            Some("bibtex")
        } else {
            None
        };
        let mut bibliography_log = String::new();
        if let Some(name) = bibliography {
            let tool = which(name);
            if tool.is_none() {
                code = 1;
                let message =
                    format!("\nerror: La bibliografía necesita {name}, pero no está instalado.\n");
                output.push_str(&message);
                diagnostics.push_str(&message);
            }
            if let Some(tool) = tool {
                let mut cmd = Command::new(tool);
                cmd.arg(root.file_stem().unwrap())
                    .current_dir(root.parent().unwrap());
                let (c, log) = run(cmd, deadline, &cancel)?;
                output.push_str(&log);
                bibliography_log = log.clone();
                diagnostics.push_str(&log);
                code = c;
            }
        }
        // Dos pasadas después de BibTeX/biber resuelven las citas y sus referencias.
        for _ in 0..2 {
            if code != 0 {
                break;
            }
            let (c, log) = run(
                build_command(&engine, &executable, &root),
                deadline,
                &cancel,
            )?;
            code = c;
            output.push_str(&log);
            diagnostics = format!("{log}\n{bibliography_log}");
        }
    }
    let after = fs::metadata(&pdf).and_then(|m| m.modified()).ok();
    let produced = after.is_some() && after != before;
    let mut problems = parse_output(&diagnostics, &root);
    if code != 0 && !problems.iter().any(|p| p.severity == "error") {
        problems.insert(
            0,
            Problem {
                severity: "error".into(),
                message: format!("{engine} terminó con código {code}"),
                file: root.file_name().unwrap().into(),
                line: None,
            },
        );
    }
    let ok = code == 0 && produced;
    if code == 0 && !ok {
        problems.insert(
            0,
            Problem {
                severity: "error".into(),
                message: "El motor terminó sin generar un PDF nuevo".into(),
                file: root.clone(),
                line: None,
            },
        );
    }
    Ok(CompileResult {
        ok,
        engine,
        root,
        pdf: if ok || produced { Some(pdf) } else { None },
        duration: started.elapsed().as_secs_f64(),
        output,
        problems,
    })
}

pub fn utility(command: Command) -> Result<String, String> {
    let (code, output) = run(
        command,
        Instant::now() + Duration::from_secs(30),
        &AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    if code == 0 {
        Ok(output)
    } else {
        Err(output.trim().to_string())
    }
}

/// Página (desde 0) y posición en puntos PDF de una línea del código.
pub fn sync_forward(pdf: &Path, source: &Path, line: usize) -> Result<(usize, f32, f32), String> {
    synctex::Index::load(pdf)?
        .forward(source, line)
        .ok_or_else(|| "SyncTeX no encontró esta línea en el PDF".into())
}

/// Archivo y línea del punto del PDF, en puntos desde arriba a la izquierda.
pub fn sync_back(pdf: &Path, page: usize, x: f32, y: f32) -> Result<(PathBuf, usize), String> {
    synctex::Index::load(pdf)?
        .backward(page, x, y)
        .ok_or_else(|| "SyncTeX no encontró el código de este punto".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_and_root_directive() {
        let output = "error: main.tex:24: Undefined control sequence\n! Undefined control sequence.\nl.24 \\foo\nwarning: main.tex:12: aviso\nerror: the XeTeX engine had an unrecoverable error\n./cap/uno.tex:7: Missing $ inserted.\nLaTeX Warning: Reference x undefined on input line 9.\nOverfull \\hbox (4pt too wide) in paragraph at lines 14--15";
        let p = parse_output(output, Path::new("main.tex"));
        assert_eq!(
            p.iter()
                .map(|p| (p.severity.as_str(), p.line))
                .collect::<Vec<_>>(),
            vec![
                ("error", Some(24)),
                ("error", Some(7)),
                ("warning", Some(12)),
                ("warning", Some(9)),
                ("info", Some(14))
            ]
        );
        assert_eq!(p[1].file, PathBuf::from("./cap/uno.tex"));
        let dir = std::env::temp_dir().join(format!("miyu-root-{}", std::process::id()));
        fs::create_dir_all(dir.join("cap")).unwrap();
        let main = dir.join("main.tex");
        fs::write(&main, "\\documentclass{article}").unwrap();
        assert_eq!(
            find_root(&dir.join("cap/uno.tex"), "% !TEX root = ../main.tex"),
            main.canonicalize().unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn biber_and_citation_location() {
        let dir = std::env::temp_dir().join(format!("miyu-cita-{}", std::process::id()));
        fs::create_dir_all(dir.join("cap")).unwrap();
        let main = dir.join("main.tex");
        fs::write(&main, "\\documentclass{report}\n\\include{cap/uno}\n").unwrap();
        fs::write(
            dir.join("cap/uno.tex"),
            "Texto\n\\textcite{perez2020} dice\n",
        )
        .unwrap();
        let output = "LaTeX Warning: Citation 'perez2020' on page 2 undefined on input line 2.\n\nPackage biblatex Warning: The starred command is\n(biblatex)                deprecated.\n\n[93] Biber.pm:130> WARN - I didn't find a database entry for 'perez2020' (section 0)\nERROR - Error: Found biblatex control file version 3.8, expected version 3.11.\nerror: the external tool exited with error code 2";
        let p = parse_output(output, &main);
        assert_eq!(p.len(), 4);
        assert_eq!(p[0].severity, "error");
        assert!(p[0].message.contains("biber 2.17"));
        assert_eq!(
            (p[1].file.clone(), p[1].line),
            ("cap/uno.tex".into(), Some(2))
        );
        assert_eq!(p[2].message, "The starred command is deprecated.");
        assert!(p[3].message.starts_with("biber: I didn't find"));
        fs::remove_dir_all(dir).unwrap();
    }
}
