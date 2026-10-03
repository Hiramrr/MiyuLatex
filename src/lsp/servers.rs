//! Qué servidor de lenguaje corresponde a cada archivo y dónde está su raíz.

use std::path::{Path, PathBuf};

use crate::compiler;

#[derive(Clone)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone)]
pub struct Spec {
    /// Nombre mostrado; con la raíz identifica una instancia.
    pub name: String,
    /// Se usa el primero que exista.
    pub candidates: Vec<Launch>,
    /// Archivos que marcan la raíz del proyecto de este lenguaje.
    pub markers: Vec<String>,
    /// Extensión y `languageId` de LSP.
    pub languages: Vec<(String, String)>,
}

impl Spec {
    fn new(
        name: &str,
        candidates: &[(&str, &[&str])],
        markers: &[&str],
        langs: &[(&str, &str)],
    ) -> Self {
        let own = |s: &&str| s.to_string();
        Self {
            name: name.into(),
            candidates: candidates
                .iter()
                .map(|(program, args)| Launch {
                    program: program.to_string(),
                    args: args.iter().map(own).collect(),
                })
                .collect(),
            markers: markers.iter().map(own).collect(),
            languages: langs
                .iter()
                .map(|(ext, id)| (ext.to_string(), id.to_string()))
                .collect(),
        }
    }

    /// Ejecutable y argumentos del primer candidato instalado.
    pub fn resolve(&self) -> Option<(PathBuf, Vec<String>)> {
        self.candidates.iter().find_map(|c| {
            let program = if c.program.contains('/') {
                Some(PathBuf::from(&c.program)).filter(|p| p.is_file())
            } else {
                compiler::which(&c.program)
            };
            program.map(|p| (p, c.args.clone()))
        })
    }

    pub fn language(&self, path: &Path) -> Option<&str> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        self.languages
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, id)| id.as_str())
    }

    /// Carpeta más cercana al archivo, sin salir del proyecto, que tiene un
    /// marcador; si no hay, el proyecto (o la carpeta del archivo si es ajeno).
    pub fn root(&self, path: &Path, project: &Path) -> PathBuf {
        let limit = if path.starts_with(project) {
            project
        } else {
            path.parent().unwrap_or(project)
        };
        let mut dir = path.parent();
        while let Some(current) = dir {
            if !current.starts_with(limit) {
                break;
            }
            if self.markers.iter().any(|m| current.join(m).exists()) {
                return current.to_path_buf();
            }
            dir = current.parent();
        }
        limit.to_path_buf()
    }
}

pub fn default_table() -> Vec<Spec> {
    vec![
        Spec::new(
            "rust-analyzer",
            &[("rust-analyzer", &[])],
            &["Cargo.toml"],
            &[("rs", "rust")],
        ),
        Spec::new(
            "pyright",
            &[("pyright-langserver", &["--stdio"]), ("pylsp", &[])],
            &[
                "pyproject.toml",
                "pyrightconfig.json",
                "setup.py",
                "setup.cfg",
            ],
            &[("py", "python"), ("pyi", "python")],
        ),
        Spec::new(
            "gopls",
            &[("gopls", &[])],
            &["go.work", "go.mod"],
            &[("go", "go")],
        ),
        Spec::new(
            "clangd",
            &[("clangd", &[])],
            &["compile_commands.json", "compile_flags.txt", ".clangd"],
            &[
                ("c", "c"),
                ("h", "c"),
                ("cc", "cpp"),
                ("cpp", "cpp"),
                ("cxx", "cpp"),
                ("hpp", "cpp"),
                ("hh", "cpp"),
                ("hxx", "cpp"),
            ],
        ),
        Spec::new(
            "texlab",
            &[("texlab", &[])],
            &[".texlabroot", "texlab.toml", ".latexmkrc"],
            &[
                ("tex", "latex"),
                ("sty", "latex"),
                ("cls", "latex"),
                ("ltx", "latex"),
                ("bib", "bibtex"),
            ],
        ),
    ]
}

/// `file://` con los bytes no seguros en porcentaje.
pub fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = rest.get(i + 1..i + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(String::from_utf8(out).ok()?);
    // Los servidores devuelven rutas sin resolver enlaces; los documentos abiertos sí.
    Some(path.canonicalize().unwrap_or(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_round_trip_with_spaces_and_accents() {
        let dir = std::env::temp_dir().join(format!("miyu-uri-{}", std::process::id()));
        let file = dir.join("mi carpeta ñ/a#b.c");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "").unwrap();
        let uri = file_uri(&file);
        assert!(uri.starts_with("file:///") && !uri.contains(' ') && uri.contains("%20"));
        assert_eq!(path_from_uri(&uri).unwrap(), file.canonicalize().unwrap());
        assert_eq!(path_from_uri("http://x"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn table_covers_the_requested_languages() {
        let table = default_table();
        let find = |file: &str| {
            table
                .iter()
                .find(|s| s.language(Path::new(file)).is_some())
                .map(|s| s.name.as_str())
        };
        assert_eq!(find("a.rs"), Some("rust-analyzer"));
        assert_eq!(find("a.py"), Some("pyright"));
        assert_eq!(find("a.go"), Some("gopls"));
        assert_eq!(find("a.C"), Some("clangd"));
        assert_eq!(find("a.cpp"), Some("clangd"));
        assert_eq!(find("a.tex"), Some("texlab"));
        assert_eq!(find("a.md"), None);
    }

    #[test]
    fn root_is_the_nearest_marker_inside_the_project() {
        let dir = std::env::temp_dir().join(format!("miyu-root-{}", std::process::id()));
        let inner = dir.join("crates/a/src");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(dir.join("crates/a/Cargo.toml"), "").unwrap();
        let spec = &default_table()[0];
        assert_eq!(spec.root(&inner.join("lib.rs"), &dir), dir.join("crates/a"));
        assert_eq!(spec.root(&dir.join("crates/x.rs"), &dir), dir);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
