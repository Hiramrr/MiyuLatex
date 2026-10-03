//! Markdown a un HTML autónomo: el CSS va incrustado y, si hace falta, las
//! imágenes locales también.

use std::{collections::HashMap, fs, path::Path};

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Parser, Tag, TagEnd};

use super::{base64, options, percent_decode};

const STYLE: &str = r#"
:root { color-scheme: light dark; --fg: #1f2328; --bg: #ffffff; --muted: #656d76; --line: #d0d7de; --code: #f3f4f6; --link: #0969da; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #e6edf3; --bg: #0d1117; --muted: #8b949e; --line: #30363d; --code: #161b22; --link: #58a6ff; }
}
body { margin: 0; background: var(--bg); color: var(--fg); font: 16px/1.65 -apple-system, "Segoe UI", Helvetica, Arial, sans-serif; }
main { max-width: 46rem; margin: 0 auto; padding: 2.5rem 1.25rem 4rem; }
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 2rem 0 0.8rem; }
h1 { font-size: 2em; padding-bottom: 0.3em; border-bottom: 1px solid var(--line); }
h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid var(--line); }
a { color: var(--link); }
img { max-width: 100%; height: auto; }
code, pre { font: 0.9em ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; background: var(--code); border-radius: 6px; }
code { padding: 0.15em 0.35em; }
pre { padding: 0.9rem 1rem; overflow-x: auto; line-height: 1.45; }
pre code { padding: 0; background: none; }
blockquote { margin: 1rem 0; padding: 0 1rem; color: var(--muted); border-left: 4px solid var(--line); }
table { border-collapse: collapse; margin: 1rem 0; display: block; overflow-x: auto; }
th, td { border: 1px solid var(--line); padding: 0.4rem 0.8rem; }
th { background: var(--code); }
hr { border: 0; border-top: 1px solid var(--line); margin: 2rem 0; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.3em; }
li > input[type="checkbox"] { margin-right: 0.5em; }
.math { font-family: "STIX Two Math", "Cambria Math", serif; }
div.math { text-align: center; margin: 1rem 0; overflow-x: auto; }
.footnote { font-size: 0.9em; color: var(--muted); }
dt { font-weight: 600; margin-top: 0.6rem; }
@media print { main { max-width: none; padding: 0; } }
"#;

/// Documento HTML de `markdown`. Con `embed_from` (la carpeta del `.md`), las
/// imágenes con ruta relativa se incrustan como `data:`; sin ella se dejan
/// tal cual, que funciona si el HTML se guarda junto al Markdown.
pub fn markdown_to_html(markdown: &str, fallback_title: &str, embed_from: Option<&Path>) -> String {
    let mut renderer = Html {
        embed_from,
        stack: vec![String::new()],
        ..Default::default()
    };
    for event in Parser::new_ext(markdown, options()) {
        renderer.event(event);
    }
    let title = renderer
        .title
        .or(renderer.meta_title)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| fallback_title.to_owned());
    format!(
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n{}</main>\n</body>\n</html>\n",
        escape(&title),
        renderer.stack.swap_remove(0)
    )
}

#[derive(Default)]
struct Html<'a> {
    embed_from: Option<&'a Path>,
    /// Texto de salida; los encabezados, imágenes y metadatos escriben en un
    /// nivel nuevo hasta cerrarse.
    stack: Vec<String>,
    heading: Option<(usize, Option<String>)>,
    image: Option<(String, String)>,
    alignments: Vec<Alignment>,
    in_head: bool,
    column: usize,
    in_code: bool,
    in_meta: bool,
    meta: String,
    title: Option<String>,
    meta_title: Option<String>,
    used_ids: HashMap<String, usize>,
    notes: Vec<String>,
}

impl Html<'_> {
    fn out(&mut self) -> &mut String {
        self.stack.last_mut().expect("nivel de salida")
    }
    fn push(&mut self, text: &str) {
        self.out().push_str(text);
    }
    fn note_number(&mut self, label: &str) -> usize {
        match self.notes.iter().position(|n| n == label) {
            Some(i) => i + 1,
            None => {
                self.notes.push(label.to_owned());
                self.notes.len()
            }
        }
    }
    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.in_meta {
                    self.meta.push_str(&text);
                } else if let Some((alt, _)) = &mut self.image {
                    alt.push_str(&text);
                } else {
                    self.push(&escape(&text));
                }
            }
            Event::Code(text) => {
                if let Some((alt, _)) = &mut self.image {
                    alt.push_str(&text);
                } else {
                    self.push(&format!("<code>{}</code>", escape(&text)));
                }
            }
            Event::InlineMath(math) => {
                self.push(&format!(
                    "<code class=\"math\">\\({}\\)</code>",
                    escape(&math)
                ));
            }
            Event::DisplayMath(math) => {
                self.push(&format!(
                    "<div class=\"math\"><code>\\[{}\\]</code></div>",
                    escape(&math)
                ));
            }
            Event::Html(html) | Event::InlineHtml(html) => self.push(&html),
            Event::FootnoteReference(label) => {
                let n = self.note_number(&label);
                let id = slug(&label);
                self.push(&format!(
                    "<sup><a href=\"#fn-{id}\" id=\"fnref-{id}\">{n}</a></sup>"
                ));
            }
            Event::SoftBreak => self.push("\n"),
            Event::HardBreak => self.push("<br>\n"),
            Event::Rule => self.push("<hr>\n"),
            Event::TaskListMarker(done) => self.push(if done {
                "<input type=\"checkbox\" disabled checked> "
            } else {
                "<input type=\"checkbox\" disabled> "
            }),
        }
    }
    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.push("<p>"),
            Tag::Heading { level, id, .. } => {
                self.heading = Some((level as usize, id.map(|i| i.to_string())));
                self.stack.push(String::new());
            }
            Tag::BlockQuote(_) => self.push("<blockquote>\n"),
            Tag::CodeBlock(kind) => {
                self.in_code = true;
                match kind {
                    CodeBlockKind::Fenced(lang) if !lang.trim().is_empty() => {
                        let lang = lang.split_whitespace().next().unwrap_or_default();
                        self.push(&format!("<pre><code class=\"language-{}\">", escape(lang)));
                    }
                    _ => self.push("<pre><code>"),
                }
            }
            Tag::HtmlBlock => {}
            Tag::List(Some(1)) => self.push("<ol>\n"),
            Tag::List(Some(start)) => self.push(&format!("<ol start=\"{start}\">\n")),
            Tag::List(None) => self.push("<ul>\n"),
            Tag::Item => self.push("<li>"),
            Tag::FootnoteDefinition(label) => {
                let n = self.note_number(&label);
                let id = slug(&label);
                self.push(&format!(
                    "<div class=\"footnote\" id=\"fn-{id}\"><sup>{n}</sup> "
                ));
            }
            Tag::DefinitionList => self.push("<dl>\n"),
            Tag::DefinitionListTitle => self.push("<dt>"),
            Tag::DefinitionListDefinition => self.push("<dd>"),
            Tag::Table(alignments) => {
                self.alignments = alignments;
                self.push("<table>\n");
            }
            Tag::TableHead => {
                self.in_head = true;
                self.column = 0;
                self.push("<thead>\n<tr>");
            }
            Tag::TableRow => {
                self.column = 0;
                self.push("<tr>");
            }
            Tag::TableCell => {
                let style = match self.alignments.get(self.column) {
                    Some(Alignment::Left) => " style=\"text-align:left\"",
                    Some(Alignment::Center) => " style=\"text-align:center\"",
                    Some(Alignment::Right) => " style=\"text-align:right\"",
                    _ => "",
                };
                self.push(if self.in_head { "<th" } else { "<td" });
                self.push(&format!("{style}>"));
            }
            Tag::Emphasis => self.push("<em>"),
            Tag::Strong => self.push("<strong>"),
            Tag::Strikethrough => self.push("<del>"),
            Tag::Superscript => self.push("<sup>"),
            Tag::Subscript => self.push("<sub>"),
            Tag::Link {
                dest_url, title, ..
            } => {
                let dest = if dest_url
                    .trim_start()
                    .to_lowercase()
                    .starts_with("javascript:")
                {
                    "#"
                } else {
                    &dest_url
                };
                let title = if title.is_empty() {
                    String::new()
                } else {
                    format!(" title=\"{}\"", escape(&title))
                };
                self.push(&format!("<a href=\"{}\"{title}>", escape(dest)));
            }
            Tag::Image {
                dest_url, title, ..
            } => {
                let src = self.image_source(&dest_url);
                let title = if title.is_empty() {
                    String::new()
                } else {
                    format!(" title=\"{}\"", escape(&title))
                };
                self.image = Some((String::new(), format!("src=\"{}\"{title}", escape(&src))));
            }
            Tag::MetadataBlock(_) => self.in_meta = true,
        }
    }
    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.push("</p>\n"),
            TagEnd::Heading(_) => {
                let content = self.stack.pop().unwrap_or_default();
                let (level, explicit) = self.heading.take().unwrap_or((1, None));
                let plain = strip_tags(&content);
                if level == 1 && self.title.is_none() {
                    self.title = Some(plain.clone());
                }
                let base = explicit.unwrap_or_else(|| slug(&plain));
                let count = self.used_ids.entry(base.clone()).or_insert(0);
                let id = if *count == 0 {
                    base
                } else {
                    format!("{base}-{count}")
                };
                *count += 1;
                self.push(&format!("<h{level} id=\"{id}\">{content}</h{level}>\n"));
            }
            TagEnd::BlockQuote(_) => self.push("</blockquote>\n"),
            TagEnd::CodeBlock => {
                self.in_code = false;
                self.push("</code></pre>\n");
            }
            TagEnd::HtmlBlock => {}
            TagEnd::List(ordered) => self.push(if ordered { "</ol>\n" } else { "</ul>\n" }),
            TagEnd::Item => self.push("</li>\n"),
            TagEnd::FootnoteDefinition => self.push("</div>\n"),
            TagEnd::DefinitionList => self.push("</dl>\n"),
            TagEnd::DefinitionListTitle => self.push("</dt>\n"),
            TagEnd::DefinitionListDefinition => self.push("</dd>\n"),
            TagEnd::Table => self.push("</tbody>\n</table>\n"),
            TagEnd::TableHead => {
                self.in_head = false;
                self.push("</tr>\n</thead>\n<tbody>\n");
            }
            TagEnd::TableRow => self.push("</tr>\n"),
            TagEnd::TableCell => {
                self.column += 1;
                self.push(if self.in_head { "</th>" } else { "</td>" });
            }
            TagEnd::Emphasis => self.push("</em>"),
            TagEnd::Strong => self.push("</strong>"),
            TagEnd::Strikethrough => self.push("</del>"),
            TagEnd::Superscript => self.push("</sup>"),
            TagEnd::Subscript => self.push("</sub>"),
            TagEnd::Link => self.push("</a>"),
            TagEnd::Image => {
                if let Some((alt, attributes)) = self.image.take() {
                    self.push(&format!("<img {attributes} alt=\"{}\">", escape(&alt)));
                }
            }
            TagEnd::MetadataBlock(_) => {
                self.in_meta = false;
                self.meta_title = self.meta.lines().find_map(|line| {
                    let value = line.strip_prefix("title:")?.trim();
                    Some(value.trim_matches(['"', '\'']).to_owned())
                });
                self.meta.clear();
            }
        }
    }
    /// Imagen relativa convertida en `data:` cuando se pide y se puede leer.
    fn image_source(&self, dest: &str) -> String {
        let Some(base) = self.embed_from else {
            return dest.to_owned();
        };
        if dest.contains(':') || dest.starts_with('/') || dest.starts_with('#') {
            return dest.to_owned();
        }
        let path = base.join(percent_decode(dest));
        let mime = match path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("svg") => "image/svg+xml",
            Some("bmp") => "image/bmp",
            _ => return dest.to_owned(),
        };
        // Un archivo enorme haría un HTML inmanejable: se deja la ruta.
        if fs::metadata(&path).is_ok_and(|m| m.len() > 20 * 1024 * 1024) {
            return dest.to_owned();
        }
        match fs::read(&path) {
            Ok(bytes) => format!("data:{mime};base64,{}", base64(&bytes)),
            Err(_) => dest.to_owned(),
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Texto de un fragmento HTML ya escapado, sin las etiquetas.
fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// Identificador de un encabezado, como el de GitHub.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else if c.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        "seccion".into()
    } else {
        out
    }
}
