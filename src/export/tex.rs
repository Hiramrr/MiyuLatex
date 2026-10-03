//! Markdown a un documento LaTeX completo, listo para compilar.

use std::path::{Path, PathBuf};

use pulldown_cmark::{Alignment, Event, Parser, Tag, TagEnd};

use super::{options, percent_decode};

/// Documento LaTeX y las imágenes que cita: cada una es el nombre con que
/// aparece en el texto y el archivo de origen, para copiarlas junto a él.
pub struct Converted {
    pub text: String,
    pub images: Vec<(String, PathBuf)>,
}

const PREAMBLE: &str = r"\documentclass[11pt,a4paper]{article}
\usepackage{iftex}
\ifPDFTeX
  \usepackage[T1]{fontenc}
  \usepackage[utf8]{inputenc}
\else
  \usepackage{fontspec}
\fi
\usepackage[margin=2.5cm]{geometry}
\usepackage{amsmath,amssymb}
\usepackage{graphicx}
\usepackage{array}
\usepackage[normalem]{ulem}
\usepackage[colorlinks=true,urlcolor=blue,linkcolor=blue]{hyperref}
\setlength{\parindent}{0pt}
\setlength{\parskip}{0.6em}
\makeatletter
\def\maxwidth{\ifdim\Gin@nat@width>\linewidth\linewidth\else\Gin@nat@width\fi}
\def\maxheight{\ifdim\Gin@nat@height>0.8\textheight 0.8\textheight\else\Gin@nat@height\fi}
\makeatother
\setkeys{Gin}{width=\maxwidth,height=\maxheight,keepaspectratio}
";

/// Convierte `markdown`. `base` es la carpeta del `.md`, donde se buscan las
/// imágenes con ruta relativa.
pub fn markdown_to_latex(markdown: &str, base: Option<&Path>) -> Converted {
    let mut tex = Tex {
        base,
        stack: vec![String::new()],
        ..Default::default()
    };
    let mut options = options();
    options |= pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION;
    for event in Parser::new_ext(markdown, options) {
        tex.event(event);
    }
    let mut body = tex.stack.swap_remove(0);
    for (label, text) in &tex.notes {
        body = body.replace(&marker(label), &format!("\\footnote{{{}}}", text.trim()));
    }
    // Una referencia sin definición no deja rastro.
    while let Some(start) = body.find('\u{1}') {
        let end = body[start + 1..]
            .find('\u{1}')
            .map_or(body.len(), |e| start + 2 + e);
        body.replace_range(start..end, "");
    }
    Converted {
        text: format!(
            "{PREAMBLE}\\begin{{document}}\n\n{}\n\n\\end{{document}}\n",
            body.trim_end()
        ),
        images: tex.images,
    }
}

fn marker(label: &str) -> String {
    format!("\u{1}{label}\u{1}")
}

#[derive(Default)]
struct Tex<'a> {
    base: Option<&'a Path>,
    /// Texto de salida; tablas, imágenes y notas escriben en un nivel nuevo
    /// hasta cerrarse.
    stack: Vec<String>,
    images: Vec<(String, PathBuf)>,
    notes: Vec<(String, String)>,
    note: Vec<String>,
    image: Option<(String, Option<String>)>,
    lists: Vec<bool>,
    alignments: Vec<Alignment>,
    rows: Vec<Vec<String>>,
    in_meta: bool,
    in_code: bool,
}

impl Tex<'_> {
    fn push(&mut self, text: &str) {
        self.stack
            .last_mut()
            .expect("nivel de salida")
            .push_str(text);
    }
    fn event(&mut self, event: Event) {
        if self.in_meta {
            if matches!(event, Event::End(TagEnd::MetadataBlock(_))) {
                self.in_meta = false;
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some((alt, _)) = &mut self.image {
                    alt.push_str(&text);
                } else if self.in_code {
                    // Dentro de `verbatim` nada se escapa.
                    self.push(&text.replace("\\end{verbatim}", "\\end {verbatim}"));
                } else {
                    self.push(&escape(&text));
                }
            }
            Event::Code(text) => {
                if let Some((alt, _)) = &mut self.image {
                    alt.push_str(&text);
                } else {
                    self.push(&format!("\\texttt{{{}}}", escape(&text)));
                }
            }
            Event::InlineMath(math) => self.push(&format!("${math}$")),
            Event::DisplayMath(math) => self.push(&format!("\\[{math}\\]")),
            Event::Html(html) | Event::InlineHtml(html) => {
                let tag = html.trim().to_lowercase();
                if tag.starts_with("<br") {
                    self.push("\\newline ");
                }
            }
            Event::FootnoteReference(label) => self.push(&marker(&label)),
            Event::SoftBreak => self.push("\n"),
            Event::HardBreak => self.push("\\\\\n"),
            Event::Rule => self.push("\\par\\noindent\\rule{\\linewidth}{0.4pt}\\par\n\n"),
            Event::TaskListMarker(done) => self.push(if done {
                "\\makebox[1.4em][l]{$\\boxtimes$}"
            } else {
                "\\makebox[1.4em][l]{$\\square$}"
            }),
        }
    }
    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.stack.push(String::new()),
            Tag::Heading { .. } => self.stack.push(String::new()),
            Tag::BlockQuote(_) => self.push("\\begin{quote}\n"),
            Tag::CodeBlock(_) => {
                self.in_code = true;
                self.push("{\\small\n\\begin{verbatim}\n");
            }
            Tag::HtmlBlock => {}
            Tag::List(start) => {
                self.lists.push(start.is_some());
                match start {
                    Some(1) => self.push("\\begin{enumerate}\n"),
                    Some(n) => self.push(&format!(
                        "\\begin{{enumerate}}\n\\setcounter{{enumi}}{{{}}}\n",
                        n.saturating_sub(1)
                    )),
                    None => self.push("\\begin{itemize}\n"),
                }
            }
            Tag::Item => self.push("\\item "),
            Tag::FootnoteDefinition(label) => {
                self.note.push(label.to_string());
                self.stack.push(String::new());
            }
            Tag::DefinitionList => self.push("\\begin{description}\n"),
            Tag::DefinitionListTitle => self.push("\\item["),
            Tag::DefinitionListDefinition => {}
            Tag::Table(alignments) => {
                self.alignments = alignments;
                self.rows.clear();
            }
            Tag::TableHead | Tag::TableRow => self.rows.push(Vec::new()),
            Tag::TableCell => self.stack.push(String::new()),
            Tag::Emphasis => self.push("\\emph{"),
            Tag::Strong => self.push("\\textbf{"),
            Tag::Strikethrough => self.push("\\sout{"),
            Tag::Superscript => self.push("\\textsuperscript{"),
            Tag::Subscript => self.push("\\textsubscript{"),
            Tag::Link { dest_url, .. } => {
                if dest_url.contains("://") || dest_url.starts_with("mailto:") {
                    self.push(&format!("\\href{{{}}}{{", escape_url(&dest_url)));
                } else {
                    // Los enlaces a otros archivos o a anclas no sirven en el PDF.
                    self.push("{");
                }
            }
            Tag::Image { dest_url, .. } => {
                let name = self.copy_image(&dest_url);
                self.image = Some((String::new(), name));
            }
            Tag::MetadataBlock(_) => self.in_meta = true,
        }
    }
    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let text = self.stack.pop().unwrap_or_default();
                if !text.trim().is_empty() {
                    self.push(&format!("{}\n\n", text.trim_end()));
                }
            }
            TagEnd::Heading(level) => {
                let text = self.stack.pop().unwrap_or_default();
                let command = match level as usize {
                    1 => "section",
                    2 => "subsection",
                    3 => "subsubsection",
                    4 => "paragraph",
                    _ => "subparagraph",
                };
                self.push(&format!("\\{command}*{{{}}}\n\n", text.trim()));
            }
            TagEnd::BlockQuote(_) => self.push("\\end{quote}\n\n"),
            TagEnd::CodeBlock => {
                self.in_code = false;
                self.push("\\end{verbatim}\n}\n\n");
            }
            TagEnd::HtmlBlock => {}
            TagEnd::List(_) => {
                let ordered = self.lists.pop().unwrap_or(false);
                self.push(if ordered {
                    "\\end{enumerate}\n\n"
                } else {
                    "\\end{itemize}\n\n"
                });
            }
            TagEnd::Item => self.push("\n"),
            TagEnd::FootnoteDefinition => {
                let text = self.stack.pop().unwrap_or_default();
                if let Some(label) = self.note.pop() {
                    self.notes.push((label, text));
                }
            }
            TagEnd::DefinitionList => self.push("\\end{description}\n\n"),
            TagEnd::DefinitionListTitle => self.push("] "),
            TagEnd::DefinitionListDefinition => self.push("\n"),
            TagEnd::Table => {
                let table = self.table();
                self.push(&table);
            }
            TagEnd::TableHead | TagEnd::TableRow => {}
            TagEnd::TableCell => {
                let text = self.stack.pop().unwrap_or_default();
                if let Some(row) = self.rows.last_mut() {
                    row.push(text.trim().to_owned());
                }
            }
            TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Superscript
            | TagEnd::Subscript
            | TagEnd::Link => self.push("}"),
            TagEnd::Image => {
                if let Some((alt, name)) = self.image.take() {
                    match name {
                        Some(name) => {
                            let caption = if alt.trim().is_empty() {
                                String::new()
                            } else {
                                format!("\\\\[2pt]{{\\small\\emph{{{}}}}}", escape(alt.trim()))
                            };
                            self.push(&format!(
                                "\\begin{{center}}\\includegraphics{{{name}}}{caption}\\end{{center}}"
                            ));
                        }
                        None => self.push(&format!(
                            "\\emph{{[imagen no disponible: {}]}}",
                            escape(&alt)
                        )),
                    }
                }
            }
            TagEnd::MetadataBlock(_) => {}
        }
    }
    /// Copia lógica de una imagen: LaTeX solo lee PNG, JPEG y PDF, y las
    /// remotas no se descargan.
    fn copy_image(&mut self, dest: &str) -> Option<String> {
        let base = self.base?;
        if dest.contains("://") || dest.starts_with("data:") {
            return None;
        }
        let path = base.join(percent_decode(dest));
        let extension = path.extension()?.to_string_lossy().to_lowercase();
        if !["png", "jpg", "jpeg", "pdf"].contains(&extension.as_str()) || !path.is_file() {
            return None;
        }
        // El nombre original puede tener espacios o caracteres que LaTeX no admite.
        let name = format!("img-{}.{extension}", self.images.len() + 1);
        self.images.push((name.clone(), path));
        Some(name)
    }
    fn table(&mut self) -> String {
        let rows = std::mem::take(&mut self.rows);
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        if columns == 0 {
            return String::new();
        }
        let widest: usize = (0..columns)
            .map(|c| {
                rows.iter()
                    .map(|r| r.get(c).map_or(0, |t| t.chars().count()))
                    .max()
                    .unwrap_or(0)
            })
            .sum();
        // Con mucho texto las columnas pasan a párrafos para que quepan.
        let wrap = widest > 70;
        let spec: String = (0..columns)
            .map(|c| {
                let align = self.alignments.get(c).copied().unwrap_or(Alignment::None);
                if wrap {
                    let before = match align {
                        Alignment::Center => "\\centering",
                        Alignment::Right => "\\raggedleft",
                        _ => "\\raggedright",
                    };
                    format!(
                        ">{{{before}\\arraybackslash}}p{{\\dimexpr0.9\\linewidth/{columns}\\relax}}|"
                    )
                } else {
                    match align {
                        Alignment::Center => "c|".into(),
                        Alignment::Right => "r|".into(),
                        _ => "l|".into(),
                    }
                }
            })
            .collect();
        let mut out =
            format!("\\begin{{center}}\n\\small\n\\begin{{tabular}}{{|{spec}}}\n\\hline\n");
        for (i, row) in rows.iter().enumerate() {
            let mut cells = row.clone();
            cells.resize(columns, String::new());
            out.push_str(&cells.join(" & "));
            out.push_str(" \\\\\n\\hline\n");
            if i == 0 {
                out.push_str("\\hline\n");
            }
        }
        out.push_str("\\end{tabular}\n\\end{center}\n\n");
        out
    }
}

/// Texto con los caracteres especiales de LaTeX neutralizados.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '%' => out.push_str("\\%"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '^' => out.push_str("\\textasciicircum{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            _ => out.push(c),
        }
    }
    out
}

/// Destino de `\href`, donde `%` y `#` también necesitan barra.
fn escape_url(url: &str) -> String {
    url.replace('\\', "/")
        .replace('%', "\\%")
        .replace('#', "\\#")
        .replace('&', "\\&")
        .replace('{', "%7B")
        .replace('}', "%7D")
}
