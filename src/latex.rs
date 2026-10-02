use std::{
    collections::{BTreeSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
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
pub fn code(text: &str) -> String {
    let lines: Vec<_> = text.split('\n').map(str::to_string).collect();
    let spans = highlight::tokenize(&lines);
    lines
        .iter()
        .zip(spans)
        .map(|(line, spans)| {
            let mut chars: Vec<_> = line.chars().collect();
            for span in spans {
                if matches!(span.tok, highlight::Tok::Comment | highlight::Tok::Verbatim) {
                    chars[span.start..span.end].fill(' ');
                }
            }
            chars.into_iter().collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    let mut targets = Vec::new();
    for source in sources {
        let clean = code(&source.text);
        for m in regex(r"\\label\s*\{([^}]+)\}").captures_iter(&clean) {
            let start = m.get(0).unwrap().start();
            targets.push(Target {
                path: source.path.clone(),
                row: clean[..start].matches('\n').count(),
                col: clean[..start].rsplit('\n').next().unwrap().chars().count(),
                label: m[1].into(),
                detail: "Etiqueta".into(),
            });
        }
    }
    targets
}

pub fn citations(sources: &[Source]) -> Vec<Target> {
    let mut targets = Vec::new();
    for source in sources {
        let text = if source.path.extension().is_some_and(|e| e == "bib") {
            source.text.clone()
        } else {
            code(&source.text)
        };
        let entries: Vec<_> = regex(
            r"(?im)^\s*@([a-z]+)\s*[({]\s*([^,\s})]+)\s*,|\\bibitem(?:\[[^\]]*\])?\{([^}]+)\}",
        )
        .captures_iter(&text)
        .collect();
        for (i, entry) in entries.iter().enumerate() {
            if entry.get(1).is_some_and(|kind| {
                ["comment", "string", "preamble"].contains(&kind.as_str().to_lowercase().as_str())
            }) {
                continue;
            }
            let start = entry.get(0).unwrap().start();
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
                row: text[..start].matches('\n').count(),
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
    }
    targets
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
}
