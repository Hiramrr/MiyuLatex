//! Exportación de documentos: Markdown a HTML, PDF y EPUB, y de un artículo
//! LaTeX a una presentación Beamer. Todo es texto en memoria o procesos
//! externos; la interfaz lo ejecuta en un hilo.

mod beamer;
mod html;
#[cfg(test)]
mod tests;
mod tex;

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use pulldown_cmark::Options;

use crate::{compiler, config};

pub use beamer::beamer_from_latex;
pub use html::markdown_to_html;
pub use tex::markdown_to_latex;

/// Las mismas extensiones que la vista previa, más matemáticas y metadatos.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

/// Pandoc, si está instalado. Se busca una vez: la interfaz lo consulta al
/// dibujar los menús.
pub fn pandoc() -> Option<&'static Path> {
    static PANDOC: OnceLock<Option<PathBuf>> = OnceLock::new();
    PANDOC.get_or_init(|| compiler::which("pandoc")).as_deref()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `%20` y similares de un destino de enlace o imagen.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Carpeta temporal propia que se borra al soltarla.
struct Scratch(PathBuf);

impl Scratch {
    fn new(purpose: &str) -> io::Result<Self> {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "miyu-{purpose}-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Escribe `markdown` como HTML en `destination`. Las imágenes relativas se
/// incrustan salvo que el HTML quede en la misma carpeta que el Markdown.
pub fn export_html(
    markdown: &str,
    source_dir: Option<&Path>,
    title: &str,
    destination: &Path,
) -> Result<(), String> {
    let destination_dir = destination
        .parent()
        .map(|p| p.canonicalize().unwrap_or(p.into()));
    let same = match (source_dir, &destination_dir) {
        (Some(a), Some(b)) => a.canonicalize().is_ok_and(|a| a == *b),
        _ => false,
    };
    let embed = if same { None } else { source_dir };
    let html = markdown_to_html(markdown, title, embed);
    config::atomic_write(destination, html.as_bytes())
        .map_err(|e| format!("No pude guardar {}: {e}", destination.display()))
}

/// Convierte `markdown` a LaTeX y lo compila con el primer motor instalado.
/// Devuelve el nombre del motor usado.
pub fn export_pdf(
    markdown: &str,
    source_dir: Option<&Path>,
    destination: &Path,
) -> Result<String, String> {
    if markdown.trim().is_empty() {
        return Err("El documento está vacío: no hay nada que exportar a PDF".into());
    }
    let converted = markdown_to_latex(markdown, source_dir);
    let scratch = Scratch::new("export-pdf").map_err(|e| e.to_string())?;
    let root = scratch.0.join("documento.tex");
    fs::write(&root, &converted.text).map_err(|e| e.to_string())?;
    for (name, source) in &converted.images {
        fs::copy(source, scratch.0.join(name))
            .map_err(|e| format!("No pude copiar la imagen {}: {e}", source.display()))?;
    }
    let (engine, executable) = compiler::select_engine(&root, "auto").map_err(|_| {
        "No encontré un motor LaTeX para generar el PDF. Instala Tectonic (brew install tectonic) o una distribución TeX.".to_owned()
    })?;
    let result = compiler::compile(
        root,
        engine.clone(),
        executable,
        Arc::new(AtomicBool::new(false)),
    )
    .map_err(|e| e.to_string())?;
    let pdf = result.pdf.clone().filter(|_| result.ok).ok_or_else(|| {
        let errors: Vec<_> = result
            .problems
            .iter()
            .filter(|p| p.severity == "error")
            .take(3)
            .map(|p| p.message.as_str())
            .collect();
        format!("{engine} no pudo generar el PDF: {}", errors.join(" · "))
    })?;
    let bytes = fs::read(&pdf).map_err(|e| e.to_string())?;
    config::atomic_write(destination, &bytes)
        .map_err(|e| format!("No pude guardar {}: {e}", destination.display()))?;
    Ok(engine)
}

/// Orden de Pandoc que convierte `input` en `output`. Corre en `directory`,
/// de modo que las imágenes relativas del Markdown se encuentren.
fn epub_command(
    pandoc: &Path,
    input: &Path,
    output: &Path,
    directory: &Path,
    title: Option<&str>,
) -> Command {
    let mut command = Command::new(pandoc);
    command
        .arg(input)
        .arg("--from=markdown")
        .arg("--to=epub")
        .arg("--output")
        .arg(output)
        .current_dir(directory);
    if let Some(title) = title {
        command.arg(format!("--metadata=title:{title}"));
    }
    command
}

/// Genera un EPUB con Pandoc.
pub fn export_epub(
    pandoc: &Path,
    markdown: &str,
    source_dir: Option<&Path>,
    title: &str,
    destination: &Path,
) -> Result<(), String> {
    let scratch = Scratch::new("export-epub").map_err(|e| e.to_string())?;
    let input = scratch.0.join("documento.md");
    let output = scratch.0.join("documento.epub");
    fs::write(&input, markdown).map_err(|e| e.to_string())?;
    // El título del documento, si lo declara en su cabecera, manda sobre el nombre.
    let own_title = markdown.starts_with("---") && markdown.contains("\ntitle:");
    let directory = source_dir.unwrap_or(&scratch.0);
    let command = epub_command(
        pandoc,
        &input,
        &output,
        directory,
        (!own_title).then_some(title),
    );
    compiler::utility(command).map_err(|e| format!("Pandoc falló: {e}"))?;
    let bytes = fs::read(&output).map_err(|e| e.to_string())?;
    config::atomic_write(destination, &bytes)
        .map_err(|e| format!("No pude guardar {}: {e}", destination.display()))
}

/// Crea junto a `source` el `.tex` Beamer de su contenido, sin tocar
/// archivos existentes: si el nombre está ocupado usa el siguiente.
pub fn create_presentation(source: &Path, latex: &str) -> Result<PathBuf, String> {
    use io::Write;
    let stem = source
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let text = beamer_from_latex(latex, &stem)?;
    let directory = source.parent().unwrap_or(Path::new("."));
    for n in 1..1000 {
        let name = if n == 1 {
            format!("{stem}-presentacion.tex")
        } else {
            format!("{stem}-presentacion-{n}.tex")
        };
        let path = directory.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                return file
                    .write_all(text.as_bytes())
                    .map(|_| path.clone())
                    .map_err(|e| {
                        let _ = fs::remove_file(&path);
                        format!("No pude escribir {}: {e}", path.display())
                    });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(format!("No pude crear {}: {e}", path.display())),
        }
    }
    Err("Ya hay demasiadas presentaciones junto a este documento".into())
}
