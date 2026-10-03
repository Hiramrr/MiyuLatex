//! De un documento LaTeX a una presentación Beamer: una diapositiva de título
//! y una por cada `\section` o `\subsection`, con un extracto de su texto.

use crate::{editor::regex, latex};

/// Paquetes del documento original que no estorban en una presentación.
const KEPT: &[&str] = &[
    "babel",
    "inputenc",
    "amsthm",
    "mathtools",
    "textcomp",
    "lmodern",
    "microtype",
    "siunitx",
];
const ITEMS: usize = 4;
const ITEM_CHARS: usize = 120;
const PARAGRAPH_CHARS: usize = 260;

/// Presentación Beamer de `source`, el texto de un `.tex`. `fallback_title`
/// se usa si el documento no declara `\title`.
pub fn beamer_from_latex(source: &str, fallback_title: &str) -> Result<String, String> {
    // Sin comentarios ni código literal, que no deben llegar a las diapositivas.
    let clean = latex::code(source);
    let clean: &str = &clean;
    if regex(r"\\documentclass\s*(?:\[[^\]]*\])?\s*\{beamer\}").is_match(clean) {
        return Err("Este documento ya es una presentación Beamer".into());
    }
    let begin = clean
        .find("\\begin{document}")
        .ok_or("No encontré \\begin{document}: abre el documento principal")?;
    let preamble = &clean[..begin];
    let body = &clean[begin + "\\begin{document}".len()..];
    let body = body
        .find("\\end{document}")
        .map_or(body, |end| &body[..end]);
    let title = command(preamble, "title")
        .or_else(|| command(body, "title"))
        .unwrap_or_else(|| fallback_title.to_owned());
    let author = command(preamble, "author").or_else(|| command(body, "author"));
    let date = command(preamble, "date").or_else(|| command(body, "date"));

    let frames = frames(body);
    if frames.is_empty() {
        return Err(
            "No encontré ningún \\section ni \\subsection que convertir en diapositiva".into(),
        );
    }
    let mut out =
        String::from("\\documentclass[aspectratio=169]{beamer}\n\\usepackage[T1]{fontenc}\n");
    let (packages, macros) = kept_preamble(preamble);
    for line in packages {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("\\usepackage{amsmath, amssymb}\n\\usetheme{Madrid}\n");
    for line in macros {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!("\n\\title{{{title}}}\n"));
    if let Some(author) = author {
        out.push_str(&format!("\\author{{{author}}}\n"));
    }
    out.push_str(&format!(
        "\\date{{{}}}\n",
        date.as_deref().unwrap_or("\\today")
    ));
    out.push_str("\n\\begin{document}\n\n\\begin{frame}\n    \\titlepage\n\\end{frame}\n");
    for frame in frames {
        out.push('\n');
        out.push_str(&frame);
    }
    out.push_str("\n\\end{document}\n");
    Ok(out)
}

/// `\usepackage` conservados y definiciones de una línea del preámbulo.
fn kept_preamble(preamble: &str) -> (Vec<String>, Vec<String>) {
    let mut packages = Vec::new();
    let mut macros = Vec::new();
    for line in preamble.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if let Some(found) =
            regex(r"^\\usepackage\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}\s*$").captures(trimmed)
        {
            let names: Vec<_> = found[1].split(',').map(str::trim).collect();
            if names.iter().all(|n| KEPT.contains(n)) {
                packages.push(trimmed.to_owned());
            }
        } else if regex(r"^\\(?:newcommand|providecommand|DeclareMathOperator|newtheorem)\*?\s*\{")
            .is_match(trimmed)
            && balanced(trimmed)
        {
            macros.push(trimmed.to_owned());
        }
    }
    (packages, macros)
}

struct Heading {
    level: &'static str,
    title: String,
    /// Dónde empieza el texto de la sección y dónde acaba.
    content: (usize, usize),
}

fn frames(body: &str) -> Vec<String> {
    let all = regex(r"\\(chapter|part|section|subsection|subsubsection)\*?\s*(?:\[[^\]]*\])?\s*\{");
    let mut found = Vec::new();
    for m in all.captures_iter(body) {
        let whole = m.get(0).unwrap();
        let (text, end) =
            balanced_group(body, whole.end() - 1).unwrap_or((String::new(), whole.end()));
        found.push((whole.start(), m[1].to_owned(), clean_title(&text), end));
    }
    let mut headings = Vec::new();
    for (i, (_, kind, title, end)) in found.iter().enumerate() {
        let level = match kind.as_str() {
            "section" => "section",
            "subsection" => "subsection",
            _ => continue,
        };
        let next = found.get(i + 1).map_or(body.len(), |f| f.0);
        headings.push(Heading {
            level,
            title: title.clone(),
            content: (*end, next),
        });
    }
    headings
        .into_iter()
        .filter(|h| !h.title.is_empty())
        .map(|h| {
            let (content, fragile) = excerpt(&body[h.content.0..h.content.1]);
            let options = if fragile { "[fragile]" } else { "" };
            format!(
                "\\{}{{{}}}\n\\begin{{frame}}{options}{{{}}}\n{content}\\end{{frame}}\n",
                h.level, h.title, h.title
            )
        })
        .collect()
}

fn clean_title(title: &str) -> String {
    let title = regex(r"\\label\s*\{[^}]*\}").replace_all(title, "");
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cuerpo de la diapositiva y si lleva código literal (que exige `fragile`).
fn excerpt(segment: &str) -> (String, bool) {
    let fragile = segment.contains("\\verb");
    let segment = regex(r"\\label\s*\{[^}]*\}").replace_all(segment, "");
    let segment = remove_command(&segment, "footnote");
    if let Some(found) = regex(r"\\begin\{(itemize|enumerate)\}").captures(&segment) {
        let environment = found[1].to_string();
        let start = found.get(0).unwrap().end();
        let items = list_items(&segment[start..]);
        if !items.is_empty() {
            let mut out = format!("    \\begin{{{environment}}}\n");
            for item in items.iter().take(ITEMS) {
                out.push_str(&format!("        \\item {}\n", shorten(item, ITEM_CHARS)));
            }
            out.push_str(&format!("    \\end{{{environment}}}\n"));
            return (out, fragile);
        }
    }
    let paragraph = segment
        .split("\n\n")
        .flat_map(|p| p.split("\n \n"))
        .map(str::trim)
        .find(|p| {
            !p.is_empty()
                && !p.contains("\\begin{")
                && !p.starts_with("$$")
                && !regex(r"^(?:\\(?:label|index|centering|vspace|hspace|newpage|clearpage|includegraphics|input|include|bibliography|caption|maketitle|tableofcontents|item)\b|\\\[)").is_match(p)
        });
    match paragraph {
        Some(p) => (format!("    {}\n", shorten(p, PARAGRAPH_CHARS)), fragile),
        None => (String::new(), fragile),
    }
}

/// Elementos del primer nivel de la lista cuyo contenido es `text`.
fn list_items(text: &str) -> Vec<String> {
    let all = regex(r"\\(begin|end)\{(?:itemize|enumerate|description)\}|\\item\b");
    let mut depth = 0usize;
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let mut end = text.len();
    for m in all.captures_iter(text) {
        let whole = m.get(0).unwrap();
        match m.get(1).map(|g| g.as_str()) {
            Some("begin") => depth += 1,
            Some(_) => {
                if depth == 0 {
                    end = whole.start();
                    break;
                }
                depth -= 1;
            }
            None if depth == 0 => starts.push((whole.start(), whole.end())),
            None => {}
        }
    }
    let mut items = Vec::new();
    for (i, (_, from)) in starts.iter().enumerate() {
        let to = starts.get(i + 1).map_or(end, |s| s.0);
        let mut item = &text[*from..to];
        // Una sublista no cabe en un extracto.
        if let Some(nested) = regex(r"\\begin\{(?:itemize|enumerate|description)\}").find(item) {
            item = &item[..nested.start()];
        }
        let item = item.split_whitespace().collect::<Vec<_>>().join(" ");
        if !item.is_empty() {
            items.push(item);
        }
    }
    items
}

/// Argumento entre llaves del primer `\title`, `\author` o `\date`.
fn command(text: &str, name: &str) -> Option<String> {
    let pattern = match name {
        "title" => regex(r"\\title\s*(?:\[[^\]]*\])?\s*\{"),
        "author" => regex(r"\\author\s*(?:\[[^\]]*\])?\s*\{"),
        _ => regex(r"\\date\s*(?:\[[^\]]*\])?\s*\{"),
    };
    let found = pattern.find(text)?;
    let (group, _) = balanced_group(text, found.end() - 1)?;
    let group = group.split_whitespace().collect::<Vec<_>>().join(" ");
    (!group.is_empty()).then_some(group)
}

/// El grupo `{...}` que abre `text[open]`: su contenido y dónde termina.
fn balanced_group(text: &str, open: usize) -> Option<(String, usize)> {
    let mut depth = 0usize;
    let mut previous = '\0';
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' if previous != '\\' => depth += 1,
            '}' if previous != '\\' => {
                depth -= 1;
                if depth == 0 {
                    return Some((text[open + 1..open + i].to_owned(), open + i + 1));
                }
            }
            _ => {}
        }
        previous = c;
    }
    None
}

/// `text` sin los `\name{...}` que contiene.
fn remove_command(text: &str, name: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    let needle = format!("\\{name}");
    while let Some(at) = rest.find(&needle) {
        let after = &rest[at + needle.len()..];
        let skipped = after.len() - after.trim_start().len();
        if after.trim_start().starts_with('{')
            && let Some((_, end)) = balanced_group(after, skipped)
        {
            out.push_str(&rest[..at]);
            rest = &after[end..];
        } else {
            out.push_str(&rest[..at + needle.len()]);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Llaves, `$` y entornos cerrados: lo que un recorte no debe dejar abierto.
fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    let mut dollars = 0;
    let mut previous = '\0';
    for c in text.chars() {
        match c {
            '{' if previous != '\\' => depth += 1,
            '}' if previous != '\\' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            '$' if previous != '\\' => dollars += 1,
            _ => {}
        }
        previous = c;
    }
    depth == 0
        && dollars % 2 == 0
        && text.matches("\\begin{").count() == text.matches("\\end{").count()
        && text.matches("\\(").count() == text.matches("\\)").count()
        && text.matches("\\[").count() == text.matches("\\]").count()
}

/// `text` recortado a `max` caracteres en un límite de palabra que no deje
/// matemáticas ni grupos abiertos.
fn shorten(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        return text;
    }
    let mut cut: String = text.chars().take(max).collect();
    // Solo se corta entre palabras, salvo que no haya ninguna.
    let next_is_space = text.chars().nth(max).is_some_and(char::is_whitespace);
    if !next_is_space {
        match cut.rfind(' ') {
            Some(i) => cut.truncate(i),
            None => return cut_hard(&cut),
        }
    }
    while !balanced(&cut)
        || cut.ends_with('\\')
        || cut
            .rsplit(' ')
            .next()
            .is_some_and(|w| w.starts_with('\\') && !w.contains('{'))
    {
        match cut.rfind(' ') {
            Some(i) => cut.truncate(i),
            None => return String::new(),
        }
    }
    let cut = cut.trim_end_matches([',', ';', ':', ' ']);
    format!("{cut}\\ldots{{}}")
}

fn cut_hard(text: &str) -> String {
    if balanced(text) && !text.contains('\\') {
        format!("{text}\\ldots{{}}")
    } else {
        String::new()
    }
}
