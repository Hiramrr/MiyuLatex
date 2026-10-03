//! Vista previa de una fórmula: se compila sola, con el preámbulo del
//! documento, y se rasteriza su PDF.

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use eframe::egui::ColorImage;

use crate::{compiler, editor::regex, preview};

/// Entornos cuyo contenido es matemático.
const ENVIRONMENTS: &[&str] = &[
    "equation",
    "align",
    "alignat",
    "gather",
    "multline",
    "flalign",
    "eqnarray",
    "displaymath",
    "math",
];

/// Posición tras el cierre `close` de una fórmula que empieza en `from`,
/// saltando lo escapado con `\`.
fn closing(text: &str, from: usize, close: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if text[i..].starts_with(close) {
            return Some(i + close.len());
        }
        // Un párrafo en blanco termina cualquier fórmula sin cerrar.
        if text[i..].starts_with("\n\n") {
            return None;
        }
        i += if bytes[i] == b'\\' { 2 } else { 1 };
        while !text.is_char_boundary(i.min(text.len())) {
            i += 1;
        }
    }
    None
}

/// Fórmula que contiene el byte `at`, con sus delimitadores.
pub fn math_at(text: &str, at: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i <= at {
        let rest = &text[i..];
        let end = match bytes[i] {
            b'%' => {
                i += rest.find('\n').unwrap_or(rest.len());
                continue;
            }
            b'$' if rest.starts_with("$$") => closing(text, i + 2, "$$"),
            b'$' => closing(text, i + 1, "$"),
            b'\\' if rest.starts_with("\\[") => closing(text, i + 2, "\\]"),
            b'\\' if rest.starts_with("\\(") => closing(text, i + 2, "\\)"),
            b'\\' if rest.starts_with("\\begin{") => {
                let name = rest[7..].split('}').next().unwrap_or_default();
                if ENVIRONMENTS.contains(&name.trim_end_matches('*')) {
                    let close = format!("\\end{{{name}}}");
                    rest.find(&close).map(|found| i + found + close.len())
                } else {
                    i += 7;
                    continue;
                }
            }
            b'\\' => {
                // Lo escapado, como \$ o \%, no abre nada.
                i += 2;
                while !text.is_char_boundary(i.min(text.len())) {
                    i += 1;
                }
                continue;
            }
            _ => {
                i += 1;
                while !text.is_char_boundary(i.min(text.len())) {
                    i += 1;
                }
                continue;
            }
        };
        match end {
            Some(end) if at < end => return Some(&text[i..end]),
            Some(end) => i = end,
            None => i += 1,
        }
    }
    None
}

/// Documento mínimo que solo contiene `math`. Con `root`, usa su preámbulo
/// para que valgan los paquetes y las macros del documento.
pub fn document(root: Option<&str>, math: &str) -> String {
    let preamble = root
        .and_then(|text| text.split_once("\\begin{document}"))
        .map(|(preamble, _)| {
            regex(r"\\documentclass\s*(?:\[[^\]]*\])?\s*\{[^}]*\}")
                .replace(preamble, "")
                .into_owned()
        })
        .unwrap_or_default();
    let packages = if preamble.contains("amsmath") {
        ""
    } else {
        "\\usepackage{amsmath,amssymb}\n"
    };
    format!(
        "\\documentclass[preview,border=4pt]{{standalone}}\n{packages}{}\n\\begin{{document}}\n{math}\n\\end{{document}}\n",
        preamble.trim()
    )
}

/// Compila `source` en una carpeta temporal y devuelve su PDF. `folder` es
/// la carpeta del documento, donde se buscan los archivos que incluya.
fn compile(
    source: &str,
    engine: &(String, PathBuf),
    folder: Option<&Path>,
) -> Result<Vec<u8>, String> {
    // Cada compilación tiene su carpeta: puede haber otra en marcha.
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let work = std::env::temp_dir().join(format!(
        "miyu-ecuacion-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let pdf = work.join("ecuacion.pdf");
    std::fs::write(work.join("ecuacion.tex"), source).map_err(|e| e.to_string())?;
    let (name, executable) = engine;
    let mut command = Command::new(executable);
    command.current_dir(&work);
    if name == "tectonic" {
        command.args(["-X", "compile"]);
        if let Some(folder) = folder {
            command
                .arg("-Z")
                .arg(format!("search-path={}", folder.display()));
        }
    } else {
        if name == "latexmk" {
            command.arg("-pdf");
        }
        command.args(["-interaction=nonstopmode", "-halt-on-error"]);
        if let Some(folder) = folder {
            command.env("TEXINPUTS", format!("{}:", folder.display()));
        }
    }
    command.arg("ecuacion.tex");
    let output = compiler::utility(command);
    let data = std::fs::read(&pdf);
    let _ = std::fs::remove_dir_all(&work);
    match data {
        Ok(data) if !data.is_empty() => Ok(data),
        _ => {
            let output = output.unwrap_or_else(|e| e);
            // La primera línea de error es la que explica el fallo.
            let reason = output
                .lines()
                .find(|line| line.starts_with("error:") || line.starts_with('!'))
                .or(output.lines().find(|line| !line.trim().is_empty()))
                .unwrap_or("el motor no dejó ningún mensaje");
            Err(reason.trim().to_string())
        }
    }
}

/// Imagen de la fórmula y si hubo que prescindir del preámbulo del documento.
pub fn render(
    math: &str,
    root: Option<&str>,
    engine: &(String, PathBuf),
    folder: Option<&Path>,
    pixels: f32,
    invert: bool,
) -> Result<(ColorImage, bool), String> {
    let (data, plain) = match compile(&document(root, math), engine, folder) {
        Ok(data) => (data, false),
        // El preámbulo puede depender de la clase del documento: se prueba sin él.
        Err(error) if root.is_some() => match compile(&document(None, math), engine, folder) {
            Ok(data) => (data, true),
            Err(_) => return Err(error),
        },
        Err(error) => return Err(error),
    };
    preview::render_first(data, pixels, invert).map(|image| (image, plain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_formula_under_the_cursor() {
        let text = "Año: $a+b$ y \\$5 % $no$\n\\[\n  x^2\n\\]\nñ \\begin{align*}\n a &= b\n\\end{align*} \\begin{itemize} $$c$$ \\(d\\) $sin\n\ncerrar";
        let at = |needle: &str| math_at(text, text.find(needle).unwrap());
        assert_eq!(at("a+b"), Some("$a+b$"));
        assert_eq!(at("x^2"), Some("\\[\n  x^2\n\\]"));
        assert_eq!(
            at("a &= b"),
            Some("\\begin{align*}\n a &= b\n\\end{align*}")
        );
        assert_eq!(at("c$$"), Some("$$c$$"));
        assert_eq!(at("d\\)"), Some("\\(d\\)"));
        // Ni el texto, ni lo escapado, ni los comentarios, ni lo que no se cierra.
        assert_eq!(at("Año"), None);
        assert_eq!(at("5 %"), None);
        assert_eq!(at("no$"), None);
        assert_eq!(at("itemize"), None);
        assert_eq!(at("cerrar"), None);
        assert_eq!(at("sin"), None);
        assert_eq!(math_at(text, text.len()), None);
    }

    #[test]
    fn builds_a_standalone_document() {
        let root = "% !TEX program = tectonic\n\\documentclass[11pt]{article}\n\\usepackage{amsmath}\n\\newcommand{\\R}{\\mathbb{R}}\n\\begin{document}\nTexto\n\\end{document}";
        assert_eq!(
            document(Some(root), "$\\R$"),
            "\\documentclass[preview,border=4pt]{standalone}\n% !TEX program = tectonic\n\n\\usepackage{amsmath}\n\\newcommand{\\R}{\\mathbb{R}}\n\\begin{document}\n$\\R$\n\\end{document}\n"
        );
        assert!(
            document(None, "$x$").contains("\\usepackage{amsmath,amssymb}\n\n\\begin{document}")
        );
    }

    #[test]
    fn renders_a_formula_with_the_installed_engine() {
        let Some(engine) = compiler::engines().into_iter().next() else {
            return;
        };
        let root = "\\documentclass{article}\n\\newcommand{\\R}{\\mathbb{R}}\n\\usepackage{amssymb}\n\\begin{document}\n\\end{document}";
        let (image, plain) =
            render("$x \\in \\R^2$", Some(root), &engine, None, 2.0, false).unwrap();
        assert!(!plain);
        assert!(
            image.width() > 40 && image.width() < 600,
            "{:?}",
            image.size
        );
        assert!(
            image.height() > 10 && image.height() < 200,
            "{:?}",
            image.size
        );
        // Un preámbulo que no compila con standalone se sustituye por el mínimo.
        let broken =
            "\\documentclass{beamer}\n\\usetheme{NoExiste}\n\\begin{document}\n\\end{document}";
        let (_, plain) = render("$x$", Some(broken), &engine, None, 2.0, false).unwrap();
        assert!(plain);
        let error = render("$\\comandoquenoexiste$", None, &engine, None, 2.0, false).unwrap_err();
        assert!(!error.is_empty());
    }
}
