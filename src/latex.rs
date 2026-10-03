use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    rc::Rc,
    thread::LocalKey,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{compiler, config, editor::regex, highlight};

#[derive(Clone)]
pub struct Source {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Clone)]
pub struct Target {
    pub path: PathBuf,
    pub row: usize,
    pub col: usize,
    pub label: String,
    pub detail: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Project {
    pub main: Option<PathBuf>,
    pub engine: String,
}

impl Project {
    pub fn load(directory: &Path) -> Self {
        let mut project: Self = fs::read(directory.join(".miyu/project.json"))
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        if project
            .main
            .as_ref()
            .is_some_and(|p| p.components().any(|c| !matches!(c, Component::Normal(_))))
        {
            project.main = None;
        }
        project
    }

    pub fn save(&self, directory: &Path) -> io::Result<()> {
        config::atomic_write(
            &directory.join(".miyu/project.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }
}

pub fn is_source(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        ["tex", "bib", "sty", "cls", "ltx", "tikz"]
            .iter()
            .any(|e| ext.eq_ignore_ascii_case(e))
    })
}

/// Conserva líneas y columnas, sin comentarios ni bloques de código literal.
/// Texto sin comentarios ni código literal (se rellenan con espacios para
/// conservar posiciones). Se recuerda por contenido: completar y el panel de
/// referencias lo piden en cada tecla o cuadro con el mismo texto.
pub fn code(text: &str) -> Rc<str> {
    memo(&CODE, text, || clean(text).into())
}

fn clean(text: &str) -> String {
    let lines: Vec<_> = text.split('\n').map(str::to_string).collect();
    let spans = highlight::tokenize(&lines);
    let mut out = String::with_capacity(text.len());
    for (i, (line, spans)) in lines.iter().zip(spans).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let hidden: Vec<_> = spans
            .iter()
            .filter(|span| matches!(span.tok, highlight::Tok::Comment | highlight::Tok::Verbatim))
            .collect();
        if hidden.is_empty() {
            out.push_str(line);
            continue;
        }
        for (col, c) in line.chars().enumerate() {
            let blank = hidden
                .iter()
                .any(|span| (span.start..span.end).contains(&col));
            out.push(if blank { ' ' } else { c });
        }
    }
    out
}

type Memo<T> = LocalKey<RefCell<HashMap<u64, T>>>;

thread_local! {
    static CODE: RefCell<HashMap<u64, Rc<str>>> = Default::default();
    static LABELS: RefCell<HashMap<u64, Rc<[Target]>>> = Default::default();
    static CITATIONS: RefCell<HashMap<u64, Rc<[Target]>>> = Default::default();
}

/// Resultado de `make` recordado por la huella de `key`. Se vacía al pasar
/// de unas decenas de entradas: solo interesa no repetir el último cálculo.
fn memo<T: Clone, K: std::hash::Hash + ?Sized>(
    slot: &'static Memo<T>,
    key: &K,
    make: impl FnOnce() -> T,
) -> T {
    let key = eframe::egui::util::hash(key);
    if let Some(found) = slot.with_borrow(|map| map.get(&key).cloned()) {
        return found;
    }
    let value = make();
    slot.with_borrow_mut(|map| {
        if map.len() >= 32 {
            map.clear();
        }
        map.insert(key, value.clone());
    });
    value
}

pub fn resolve_file(root: &Path, current: &Path, name: &str, extension: &str) -> Option<PathBuf> {
    let name = name.trim().trim_matches('"');
    if name.is_empty() || name.contains(['\\', '#', '$']) {
        return None;
    }
    let mut relative = PathBuf::from(name);
    if relative.extension().is_none() {
        relative.set_extension(extension);
    }
    [root.parent(), current.parent()]
        .into_iter()
        .flatten()
        .find_map(|dir| {
            dir.join(&relative)
                .canonicalize()
                .ok()
                .filter(|p| p.is_file())
        })
}

pub fn sources(root: &Path, overlays: &[Source]) -> Vec<Source> {
    let mut paths = vec![root.canonicalize().unwrap_or_else(|_| root.into())];
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    let mut i = 0;
    // ponytail: hasta 500 fuentes, usar un índice incremental para proyectos mayores.
    while i < paths.len() && result.len() < 500 {
        let path = paths[i].clone();
        i += 1;
        if !seen.insert(path.clone()) {
            continue;
        }
        let text = overlays
            .iter()
            .find(|s| s.path == path || s.path.canonicalize().is_ok_and(|p| p == path))
            .map(|s| s.text.clone())
            .or_else(|| fs::read_to_string(&path).ok());
        let Some(text) = text else { continue };
        if path.extension().is_some_and(|ext| ext != "bib") {
            let clean = code(&text);
            for m in regex(r"\\(input|include|subfile|bibliography|addbibresource)(?:\s*\[[^\]]*\])?\s*\{([^}]+)\}|\\(input)\s+([^\s{}]+)").captures_iter(&clean) {
                let command = m.get(1).or_else(|| m.get(3)).unwrap().as_str();
                let names = m.get(2).or_else(|| m.get(4)).unwrap().as_str();
                let extension = if matches!(command, "bibliography" | "addbibresource") { "bib" } else { "tex" };
                for name in names.split(',') {
                    if let Some(file) = resolve_file(root, &path, name, extension)
                        && !seen.contains(&file) && !paths.contains(&file)
                    {
                        paths.push(file);
                    }
                }
            }
        }
        result.push(Source { path, text });
    }
    result
}

pub fn labels(sources: &[Source]) -> Vec<Target> {
    sources
        .iter()
        .flat_map(|source| {
            memo(&LABELS, &(&source.path, &source.text), || {
                source_labels(source)
            })
            .to_vec()
        })
        .collect()
}

fn source_labels(source: &Source) -> Rc<[Target]> {
    let clean = code(&source.text);
    let mut targets = Vec::new();
    let (mut row, mut seen) = (0, 0);
    for m in regex(r"\\label\s*\{([^}]+)\}").captures_iter(&clean) {
        let start = m.get(0).unwrap().start();
        row += clean[seen..start].matches('\n').count();
        seen = start;
        targets.push(Target {
            path: source.path.clone(),
            row,
            col: clean[..start].rsplit('\n').next().unwrap().chars().count(),
            label: m[1].into(),
            detail: "Etiqueta".into(),
        });
    }
    targets.into()
}

pub fn citations(sources: &[Source]) -> Vec<Target> {
    sources
        .iter()
        .flat_map(|source| {
            memo(&CITATIONS, &(&source.path, &source.text), || {
                source_citations(source)
            })
            .to_vec()
        })
        .collect()
}

fn source_citations(source: &Source) -> Rc<[Target]> {
    let mut targets = Vec::new();
    let text: Rc<str> = if source.path.extension().is_some_and(|e| e == "bib") {
        source.text.as_str().into()
    } else {
        code(&source.text)
    };
    let entries: Vec<_> =
        regex(r"(?im)^\s*@([a-z]+)\s*[({]\s*([^,\s})]+)\s*,|\\bibitem(?:\[[^\]]*\])?\{([^}]+)\}")
            .captures_iter(&text)
            .collect();
    let (mut row, mut seen) = (0, 0);
    for (i, entry) in entries.iter().enumerate() {
        let start = entry.get(0).unwrap().start();
        row += text[seen..start].matches('\n').count();
        seen = start;
        if entry.get(1).is_some_and(|kind| {
            ["comment", "string", "preamble"].contains(&kind.as_str().to_lowercase().as_str())
        }) {
            continue;
        }
        let end = entries
            .get(i + 1)
            .map_or(text.len(), |m| m.get(0).unwrap().start());
        let detail = regex(r#"(?im)\b(title|author|year|journal)\s*=\s*[{"]([^\n]+)"#)
            .captures_iter(&text[start..end])
            .map(|m| {
                m[2].trim()
                    .trim_end_matches([',', '}', '"'])
                    .replace(['{', '}'], "")
            })
            .collect::<Vec<_>>()
            .join(" · ");
        targets.push(Target {
            path: source.path.clone(),
            row,
            col: 0,
            label: entry
                .get(2)
                .or_else(|| entry.get(3))
                .unwrap()
                .as_str()
                .into(),
            detail,
        });
    }
    targets.into()
}

/// Un comando con sus argumentos opcionales y el contenido de sus llaves.
const COMMAND: &str = r"\\([A-Za-z]+)\*?\s*(?:\[[^\]]*\]\s*)*\{([^{}]*)\}";

/// Lo que nombra un comando LaTeX y a dónde lleva.
#[derive(Debug, PartialEq)]
pub enum Reference {
    Label(String),
    Citation(String),
    /// Nombre del archivo y extensiones con que probar si no trae una.
    File(String, &'static [&'static str]),
}

/// Referencia, cita o archivo del comando que rodea la columna `col` de `line`.
pub fn reference_at(line: &str, col: usize) -> Option<Reference> {
    let at = crate::editor::byte_col(line, col);
    regex(COMMAND)
        .captures_iter(line)
        .find_map(|m| {
            let keys = m.get(2).unwrap();
            if !(keys.start()..=keys.end()).contains(&at) {
                return None;
            }
            // Con varias claves separadas por comas, la que está bajo el cursor.
            let mut start = keys.start();
            let key = keys
                .as_str()
                .split(',')
                .find(|key| {
                    let end = start + key.len();
                    let found = at <= end;
                    start = end + 1;
                    found
                })?
                .trim();
            if key.is_empty() {
                return None;
            }
            let command = m[1].to_lowercase();
            Some(match command.as_str() {
                "input" | "include" | "subfile" => Reference::File(key.into(), &["tex"]),
                "bibliography" | "addbibresource" => Reference::File(key.into(), &["bib"]),
                "includegraphics" => Reference::File(key.into(), &["pdf", "png", "jpg", "jpeg"]),
                _ if command.contains("cite") => Reference::Citation(key.into()),
                "label" => Reference::Label(key.into()),
                _ if command.ends_with("ref") => Reference::Label(key.into()),
                _ => return None,
            })
        })
}

/// `text` con la etiqueta `old` cambiada por `new` en su `\label` y en los
/// comandos que la citan, y cuántas veces aparecía. Los comentarios y el
/// código literal no cambian.
pub fn rename_label(text: &str, old: &str, new: &str) -> (String, usize) {
    let clean = code(text);
    let mut out = String::with_capacity(text.len());
    let mut count = 0;
    for (i, (line, clean)) in text.split('\n').zip(clean.split('\n')).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // Bytes de la línea ya copiados.
        let mut done = 0;
        for m in regex(COMMAND).captures_iter(clean) {
            let command = m[1].to_lowercase();
            if command != "label" && !command.ends_with("ref") {
                continue;
            }
            let keys = m.get(2).unwrap();
            let mut start = keys.start();
            for key in keys.as_str().split(',') {
                if key.trim() == old {
                    // `clean` conserva las columnas, no los bytes.
                    let lead = key.len() - key.trim_start().len();
                    let col = clean[..start + lead].chars().count();
                    let from = crate::editor::byte_col(line, col);
                    let to = crate::editor::byte_col(line, col + old.chars().count());
                    out.push_str(&line[done..from]);
                    out.push_str(new);
                    done = to;
                    count += 1;
                }
                start += key.len() + 1;
            }
        }
        out.push_str(&line[done..]);
    }
    (out, count)
}

pub fn table(rows: usize, columns: usize, alignment: char) -> String {
    let columns = columns.clamp(1, 20);
    let rows = rows.clamp(1, 100);
    let alignment = if ['l', 'c', 'r'].contains(&alignment) {
        alignment
    } else {
        'l'
    };
    let mut body = format!(
        "\\begin{{tabular}}{{{}}}\n\\hline\n",
        alignment.to_string().repeat(columns)
    );
    for row in 0..rows {
        let cells = (0..columns)
            .map(|column| if row == 0 && column == 0 { "$0" } else { " " })
            .collect::<Vec<_>>()
            .join(" & ");
        body.push_str(&format!("{cells} \\\\\n"));
        if row == 0 {
            body.push_str("\\hline\n");
        }
    }
    body.push_str("\\hline\n\\end{tabular}");
    body
}

pub fn estimated_words(sources: &[Source]) -> usize {
    sources.iter().filter(|s| s.path.extension().is_some_and(|e| e != "bib"))
        .map(|source| {
            let text = source.text.split_once("\\begin{document}").map_or(source.text.as_str(), |(_, body)| body);
            let mut state = highlight::State::default();
            let clean = text.lines().map(|line| {
                let (spans, next) = highlight::tokenize_line(line, &state);
                state = next;
                let mut chars: Vec<_> = line.chars().collect();
                for span in spans {
                    if matches!(span.tok, highlight::Tok::Comment | highlight::Tok::Verbatim | highlight::Tok::Math | highlight::Tok::MathDelim | highlight::Tok::MathCommand) {
                        chars[span.start..span.end].fill(' ');
                    }
                }
                chars.into_iter().collect::<String>()
            }).collect::<Vec<_>>().join("\n");
            let clean = regex(r"\\(?:begin|end|label|[cC]ite\w*|ref|eqref|pageref|autoref|input|include|subfile|includegraphics|addbibresource|bibliography|bibliographystyle)(?:\[[^\]]*\])*\s*\{[^}]*\}").replace_all(&clean, " ");
            let clean = regex(r"\\[a-zA-Z@]+\*?(?:\[[^\]]*\])?").replace_all(&clean, " ");
            clean.split(|c: char| !c.is_alphabetic() && c != '\'').filter(|word| word.chars().any(char::is_alphabetic)).count()
        }).sum()
}

pub fn history_directory(path: &Path) -> PathBuf {
    let mut hash = DefaultHasher::new();
    path.canonicalize()
        .unwrap_or_else(|_| path.into())
        .hash(&mut hash);
    config::directory()
        .join("history")
        .join(format!("{:016x}", hash.finish()))
}

pub fn versions(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = fs::read_dir(history_directory(path))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "tex"))
        .collect();
    files.sort();
    files.reverse();
    files
}

pub fn checkpoint(path: &Path, text: &str) -> io::Result<()> {
    let files = versions(path);
    if files
        .first()
        .is_some_and(|p| fs::read_to_string(p).is_ok_and(|old| old == text))
    {
        return Ok(());
    }
    let directory = history_directory(path);
    fs::create_dir_all(&directory)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(format!("{stamp:024}.tex")))?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    // ponytail: últimas 100 versiones por archivo, usar Git para historial ilimitado.
    for old in files.iter().skip(99) {
        fs::remove_file(old)?;
    }
    Ok(())
}

fn archive_files(directory: &Path, files: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
    if depth > 32 {
        return Err(io::Error::other(
            "El proyecto supera 32 niveles de carpetas",
        ));
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            ".git" | "target" | "node_modules" | "__pycache__" | "dist" | ".DS_Store"
        ) || (directory.ends_with(".miyu") && name != "project.json")
        {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            archive_files(&entry.path(), files, depth + 1)?;
        } else if kind.is_file() {
            let path = entry.path();
            let generated = compiler::AUX
                .iter()
                .any(|ext| path.to_string_lossy().ends_with(&format!(".{ext}")));
            if !generated {
                files.push(path);
            }
        }
        if files.len() > 5000 {
            return Err(io::Error::other("El proyecto supera 5000 archivos"));
        }
    }
    Ok(())
}

pub fn export_zip(project: &Path, destination: &Path) -> io::Result<()> {
    use zip::{ZipWriter, write::SimpleFileOptions};
    let mut files = Vec::new();
    archive_files(project, &mut files, 0)?;
    let temp = destination.with_extension(format!("{}.zip.tmp", std::process::id()));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        let mut archive = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut total = 0;
        for path in files {
            if path == destination || path == temp {
                continue;
            }
            let mut input = fs::File::open(&path)?;
            total += input.metadata()?.len();
            if total > 500 * 1024 * 1024 {
                return Err(io::Error::other("El proyecto supera 500 MiB"));
            }
            let name = path
                .strip_prefix(project)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            archive.start_file(name, options)?;
            io::copy(&mut input, &mut archive)?;
        }
        archive.finish()?.sync_all()?;
        fs::rename(&temp, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

pub fn import_zip(archive: &Path, destination: &Path) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(archive)?)?;
    if archive.len() > 5000 {
        return Err(io::Error::other("El ZIP supera 5000 archivos"));
    }
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        if Path::new(&name).components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) || name.contains(':')
            || entry.enclosed_name().is_none()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(io::Error::other(
                "El ZIP contiene una ruta insegura o un enlace simbólico",
            ));
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| io::Error::other("ZIP demasiado grande"))?;
        if entry.size() > 100 * 1024 * 1024 || total > 500 * 1024 * 1024 {
            return Err(io::Error::other(
                "El ZIP supera 100 MiB por archivo o 500 MiB en total",
            ));
        }
    }
    // Solo se extrae a una carpeta nueva. Un error elimina esa extracción parcial.
    fs::create_dir(destination)?;
    let result = (|| {
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let output = destination.join(entry.name().replace('\\', "/"));
            if entry.is_dir() {
                fs::create_dir_all(output)?;
            } else {
                fs::create_dir_all(output.parent().unwrap())?;
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output)?;
                let expected = entry.size();
                let written = io::copy(&mut (&mut entry).take(expected + 1), &mut file)?;
                if written != expected {
                    return Err(io::Error::other("Tamaño de archivo ZIP incorrecto"));
                }
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(destination);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_under_the_cursor() {
        let line = "Ver \\eqref{eq:uno} y \\cite[p.~3]{knuth, lamport} en \\input{cap/dos}.";
        let at = |text: &str| reference_at(line, line.find(text).unwrap() + 1);
        assert_eq!(at("eq:uno"), Some(Reference::Label("eq:uno".into())));
        assert_eq!(at("knuth"), Some(Reference::Citation("knuth".into())));
        assert_eq!(at("lamport"), Some(Reference::Citation("lamport".into())));
        assert_eq!(at("cap/dos"), Some(Reference::File("cap/dos".into(), &["tex"])));
        assert_eq!(at("Ver"), None);
        assert_eq!(at("p.~3"), None);
        // Las columnas cuentan caracteres, no bytes.
        assert_eq!(
            reference_at("ñandú \\ref{sec:año}", 14),
            Some(Reference::Label("sec:año".into()))
        );
        assert_eq!(reference_at("\\section{Título}", 10), None);
        assert_eq!(
            reference_at("\\label{sec:a}", 8),
            Some(Reference::Label("sec:a".into()))
        );
    }

    #[test]
    fn renames_a_label_and_its_references() {
        let text = "ñ \\label{a} \\ref{a} \\cref{b, a,ab} % ñ \\ref{a}\r\n\\eqref{ab} \\cite{a} \\ref {a}";
        let (renamed, count) = rename_label(text, "a", "sec:año");
        assert_eq!(count, 4);
        assert_eq!(
            renamed,
            "ñ \\label{sec:año} \\ref{sec:año} \\cref{b, sec:año,ab} % ñ \\ref{a}\r\n\\eqref{ab} \\cite{a} \\ref {sec:año}"
        );
        assert_eq!(rename_label(text, "zz", "x"), (text.to_string(), 0));
    }

    #[test]
    fn project_sources_citations_history_and_safe_zip() {
        let dir = std::env::temp_dir().join(format!("miyu-project-{}", std::process::id()));
        fs::create_dir_all(dir.join("chapters")).unwrap();
        fs::create_dir_all(dir.join("refs")).unwrap();
        let root = dir.join("main.tex");
        fs::write(&root, "\\documentclass{article}\n% \\input{missing}\n\\input{chapters/one}\n\\addbibresource{refs/data.bib}").unwrap();
        let chapter = dir.join("chapters/one.tex");
        fs::write(&chapter, "\\label{old}\n\\input{main}").unwrap();
        fs::write(dir.join("refs/data.bib"), "@article{key,\n title = {Título},\n author = {Persona},\n year = {2026}\n}\n@comment{ignore, nope}").unwrap();
        let overlays = [Source { path: chapter.clone(), text: "\\label{new}\n% \\label{fake}\n\\begin{verbatim}\n\\label{literal}\n\\end{verbatim}".into() }];
        let sources = sources(&root, &overlays);
        assert_eq!(sources.len(), 3);
        assert_eq!(
            labels(&sources)
                .iter()
                .map(|t| t.label.as_str())
                .collect::<Vec<_>>(),
            ["new"]
        );
        let citations = citations(&sources);
        assert_eq!(citations.len(), 1);
        assert!(citations[0].detail.contains("Título"));
        assert!(table(2, 3, 'c').contains("{ccc}"));
        checkpoint(&chapter, "old").unwrap();
        checkpoint(&chapter, "new").unwrap();
        checkpoint(&chapter, "new").unwrap();
        assert_eq!(versions(&chapter).len(), 2);
        let zip = dir.join("project.zip");
        export_zip(&dir, &zip).unwrap();
        let extracted = dir.join("imported");
        import_zip(&zip, &extracted).unwrap();
        assert_eq!(
            fs::read_to_string(extracted.join("refs/data.bib")).unwrap(),
            sources[2].text
        );
        assert!(!extracted.join("project.zip").exists());
        assert!(import_zip(&zip, &extracted).is_err());
        let evil = dir.join("evil.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&evil).unwrap());
        writer
            .start_file("../escape.tex", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"no").unwrap();
        writer.finish().unwrap();
        assert!(import_zip(&evil, &dir.join("unsafe")).is_err());
        assert!(!dir.join("unsafe").exists());
        fs::remove_dir_all(history_directory(&chapter)).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn words_positions_and_less_common_commands() {
        let source = |path: &str, text: &str| Source { path: path.into(), text: text.into() };
        let document = source(
            "main.tex",
            "\\documentclass{article}\n\\title{Sin contar}\n\\begin{document}\nHola mundo, l'été. % comentario aquí\n\\section{Introducción} texto con $x + y$ fórmula \\cite{a} y \\ref{b}.\n\\begin{verbatim}\nno cuenta\n\\end{verbatim}\n\\end{document}",
        );
        let bib = source("refs.bib", "@book{a, title = {Muchas palabras aquí}}");
        assert_eq!(estimated_words(&[document, bib]), 8);

        let labels = labels(&[source("a.tex", "a\n  \\label{uno} \\label{dos}\n\\label{tres}")]);
        assert_eq!(
            labels.iter().map(|t| (t.label.as_str(), t.row, t.col)).collect::<Vec<_>>(),
            [("uno", 1, 2), ("dos", 1, 14), ("tres", 2, 0)]
        );

        let items = citations(&[source("b.tex", "Texto\n\\bibitem[Pérez]{perez2020} Algo\n\\bibitem{otro}")]);
        assert_eq!(
            items.iter().map(|t| (t.label.as_str(), t.row)).collect::<Vec<_>>(),
            [("perez2020", 1), ("otro", 2)]
        );

        assert_eq!(table(0, 0, 'x'), "\\begin{tabular}{l}\n\\hline\n$0 \\\\\n\\hline\n\\hline\n\\end{tabular}");

        let dir = std::env::temp_dir().join(format!("miyu-input-{}", std::process::id()));
        fs::create_dir_all(dir.join("cap")).unwrap();
        let root = dir.join("main.tex");
        fs::write(&root, "\\input cap/dos\n\\bibliography{uno, cap/tres}").unwrap();
        for name in ["cap/dos.tex", "uno.bib", "cap/tres.bib"] {
            fs::write(dir.join(name), "").unwrap();
        }
        let found = sources(&root, &[]);
        let names: Vec<_> = found.iter().map(|s| s.path.strip_prefix(dir.canonicalize().unwrap()).unwrap().to_owned()).collect();
        assert_eq!(names, ["main.tex", "cap/dos.tex", "uno.bib", "cap/tres.bib"].map(PathBuf::from));
        fs::remove_dir_all(dir).unwrap();
    }
}
