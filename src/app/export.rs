//! Exportar el documento activo: Markdown a HTML, PDF y EPUB, y de LaTeX a
//! una presentación Beamer.

use super::*;
use crate::export;

/// Lo que una exportación necesita del documento activo. Sale del editor, no
/// del disco, para que los cambios sin guardar también cuenten.
struct Source {
    text: String,
    /// Carpeta del `.md`, donde viven sus imágenes relativas.
    directory: Option<PathBuf>,
    stem: String,
}

pub(super) const EPUB_HELP: &str = "Genera un EPUB con Pandoc. Instala Pandoc (brew install pandoc o pandoc.org) y reinicia MiyuLaTeX para activarlo.";

impl App {
    /// El documento activo es Markdown y no hay otra operación en curso.
    pub(super) fn can_export_markdown(&self) -> bool {
        self.editor().format == Format::Markdown && self.tool_rx.is_none()
    }
    pub(super) fn can_export_epub(&self) -> bool {
        self.can_export_markdown() && export::pandoc().is_some()
    }
    pub(super) fn can_make_presentation(&self) -> bool {
        self.editor().format == Format::Latex && self.editor().path.is_some()
    }
    fn export_source(&mut self) -> Option<Source> {
        if self.editor().format != Format::Markdown {
            self.message = "Esta exportación es para documentos Markdown".into();
            return None;
        }
        if self.tool_rx.is_some() {
            self.message = "Espera a que termine la operación en curso".into();
            return None;
        }
        let editor = self.editor();
        let name = editor
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&editor.suggested_name));
        Some(Source {
            text: editor.source().to_owned(),
            directory: editor
                .path
                .as_ref()
                .and_then(|p| p.parent())
                .map(Path::to_path_buf),
            stem: name
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        })
    }
    /// Pregunta dónde guardar `source` con la extensión `extension`.
    fn export_destination(
        source: &Source,
        title: &str,
        label: &str,
        extension: &str,
    ) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title(title)
            .set_file_name(format!("{}.{extension}", source.stem))
            .add_filter(label, &[extension]);
        if let Some(directory) = &source.directory {
            dialog = dialog.set_directory(directory);
        }
        let destination = dialog.save_file()?;
        Some(if destination.extension().is_none() {
            destination.with_extension(extension)
        } else {
            destination
        })
    }
    pub(super) fn export_html(&mut self, ctx: &egui::Context) {
        let Some(source) = self.export_source() else {
            return;
        };
        let Some(destination) = Self::export_destination(
            &source,
            "Exportar Markdown como HTML",
            "Página HTML",
            "html",
        ) else {
            return;
        };
        self.message = "Exportando a HTML…".into();
        self.start_tool(ctx, move || {
            export::export_html(
                &source.text,
                source.directory.as_deref(),
                &source.stem,
                &destination,
            )?;
            Ok(ToolResult::Message(format!(
                "Exportado a {}",
                destination.display()
            )))
        });
    }
    pub(super) fn export_markdown_pdf(&mut self, ctx: &egui::Context) {
        let Some(source) = self.export_source() else {
            return;
        };
        let Some(destination) =
            Self::export_destination(&source, "Exportar Markdown como PDF", "PDF", "pdf")
        else {
            return;
        };
        self.message = "Exportando a PDF…".into();
        self.start_tool(ctx, move || {
            export::export_pdf(&source.text, source.directory.as_deref(), &destination)?;
            Ok(ToolResult::Message(format!(
                "Exportado a {}",
                destination.display()
            )))
        });
    }
    pub(super) fn export_epub(&mut self, ctx: &egui::Context) {
        let Some(pandoc) = export::pandoc().map(Path::to_path_buf) else {
            self.message = "Exportar a EPUB necesita Pandoc: instálalo y reinicia MiyuLaTeX".into();
            return;
        };
        let Some(source) = self.export_source() else {
            return;
        };
        let Some(destination) =
            Self::export_destination(&source, "Exportar Markdown como EPUB", "Libro EPUB", "epub")
        else {
            return;
        };
        self.message = "Exportando a EPUB…".into();
        self.start_tool(ctx, move || {
            export::export_epub(
                &pandoc,
                &source.text,
                source.directory.as_deref(),
                &source.stem,
                &destination,
            )?;
            Ok(ToolResult::Message(format!(
                "Exportado a {}",
                destination.display()
            )))
        });
    }
    /// Crea junto al `.tex` activo una presentación Beamer y la abre.
    pub(super) fn create_presentation(&mut self) {
        let Some(path) = self
            .editor()
            .path
            .clone()
            .filter(|_| self.editor().format == Format::Latex)
        else {
            self.message = "Guarda el documento LaTeX antes de crear su presentación".into();
            return;
        };
        match export::create_presentation(&path, self.editor().source()) {
            Ok(created) => {
                self.files = project_files(&self.project);
                self.refresh_sources();
                match self.open(&created) {
                    Ok(()) => {
                        self.message = format!("Presentación creada en {}", created.display());
                    }
                    Err(e) => self.message = e,
                }
            }
            Err(e) => self.message = e,
        }
    }
}
