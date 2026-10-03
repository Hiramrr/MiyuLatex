//! Bibliografía: avisos sobre los archivos `.bib` y citas descargadas por
//! DOI o identificador de arXiv.

use std::collections::{HashMap, HashSet};

use crate::{
    editor::regex,
    latex::{self, Source, Target},
};

/// Campos que BibTeX exige a cada tipo; las alternativas van separadas por `|`.
const REQUIRED: &[(&str, &[&str])] = &[
    (
        "article",
        &["author", "title", "journal|journaltitle", "year|date"],
    ),
    (
        "book",
        &["author|editor", "title", "publisher", "year|date"],
    ),
    (
        "inproceedings",
        &["author", "title", "booktitle", "year|date"],
    ),
    (
        "incollection",
        &["author", "title", "booktitle", "year|date"],
    ),
    (
        "phdthesis",
        &["author", "title", "school|institution", "year|date"],
    ),
    (
        "mastersthesis",
        &["author", "title", "school|institution", "year|date"],
    ),
    (
        "techreport",
        &["author", "title", "institution", "year|date"],
    ),
];

fn is_bib(source: &Source) -> bool {
    source.path.extension().is_some_and(|e| e == "bib")
}

/// Claves duplicadas, campos obligatorios que faltan y entradas que ningún
/// documento cita. La etiqueta de cada aviso es su mensaje.
pub fn problems(sources: &[Source]) -> Vec<Target> {
    let mut cited = HashSet::new();
    let mut everything = false;
    for source in sources.iter().filter(|s| !is_bib(s)) {
        let clean = latex::code(&source.text);
        for m in regex(r"\\[A-Za-z]*cite[A-Za-z]*\*?\s*(?:\[[^\]]*\]\s*)*\{([^{}]*)\}")
            .captures_iter(&clean)
        {
            for key in m[1].split(',') {
                everything |= key.trim() == "*";
                cited.insert(key.trim().to_string());
            }
        }
    }
    let mut found = Vec::new();
    let mut seen: HashMap<String, std::path::PathBuf> = HashMap::new();
    for source in sources.iter().filter(|s| is_bib(s)) {
        let text = &source.text;
        let entries: Vec<_> = regex(r"(?im)^\s*@([a-z]+)\s*[({]\s*([^,\s})]+)\s*,")
            .captures_iter(text)
            .collect();
        for (i, entry) in entries.iter().enumerate() {
            let start = entry.get(0).unwrap().start();
            let end = entries
                .get(i + 1)
                .map_or(text.len(), |m| m.get(0).unwrap().start());
            let kind = entry[1].to_lowercase();
            if ["comment", "string", "preamble"].contains(&kind.as_str()) {
                continue;
            }
            let key = &entry[2];
            let mut warn = |message: String| {
                found.push(Target {
                    path: source.path.clone(),
                    row: text[..start].matches('\n').count()
                        + text[start..entry.get(1).unwrap().start()]
                            .matches('\n')
                            .count(),
                    col: 0,
                    label: message,
                    detail: key.to_string(),
                });
            };
            if let Some(first) = seen.get(key) {
                let file = first.file_name().unwrap_or_default().to_string_lossy();
                warn(format!("«{key}» está repetida; ya aparece en {file}"));
            } else {
                seen.insert(key.to_string(), source.path.clone());
            }
            let block = text[start..end].to_lowercase();
            let has = |field: &str| {
                regex::Regex::new(&format!(r"\b{field}\s*="))
                    .unwrap()
                    .is_match(&block)
            };
            let missing: Vec<_> = REQUIRED
                .iter()
                .find(|(name, _)| *name == kind)
                .map_or(&["title"][..], |(_, fields)| fields)
                .iter()
                .filter(|options| !options.split('|').any(has))
                .map(|options| options.split('|').next().unwrap())
                .collect();
            if !missing.is_empty() {
                warn(format!("«{key}» (@{kind}) no tiene {}", missing.join(", ")));
            }
            if !everything && !cited.contains(key) {
                warn(format!("«{key}» no se cita en ningún documento"));
            }
        }
    }
    found
}

/// Dirección que devuelve en BibTeX un DOI o un identificador de arXiv, y si
/// hay que pedirle ese formato en la cabecera.
pub fn lookup(identifier: &str) -> Option<(String, bool)> {
    let id = identifier.trim();
    if let Some(doi) = regex(r"(?i)\b(10\.\d{4,9}/[^\s]+)").captures(id) {
        let doi = doi[1].trim_end_matches(['.', ',', ';']);
        return Some((format!("https://doi.org/{doi}"), true));
    }
    regex(r"(?i)^(?:https?://arxiv\.org/(?:abs|pdf)/|arxiv:)?\s*(\d{4}\.\d{4,5}|[a-z\-]+(?:\.[a-z]{2})?/\d{7})(?:v\d+)?(?:\.pdf)?$")
        .captures(id)
        .map(|m| (format!("https://arxiv.org/bibtex/{}", &m[1]), false))
}

/// Clave de la primera entrada de un texto BibTeX.
pub fn key(entry: &str) -> Option<String> {
    regex(r"(?i)^\s*@[a-z]+\s*\{\s*([^,\s{}]+)\s*,")
        .captures(entry)
        .map(|m| m[1].to_string())
}

/// Una entrada que llega en una sola línea, con un campo por línea.
pub fn tidy(entry: &str) -> String {
    let entry = entry.trim();
    if entry.contains('\n') {
        return entry
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
    }
    let body = entry.strip_suffix('}').unwrap_or(entry).trim_end();
    let mut out = String::with_capacity(entry.len() + 32);
    let mut depth = 0;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        out.push(c);
        // Las comas del primer nivel separan los campos.
        if c == ',' && depth == 1 {
            while chars.peek() == Some(&' ') {
                chars.next();
            }
            out.push_str("\n  ");
        }
    }
    out.push_str("\n}");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(name: &str, text: &str) -> Source {
        Source {
            path: name.into(),
            text: text.into(),
        }
    }

    #[test]
    fn warns_about_duplicates_missing_fields_and_unused_entries() {
        let bib = "@article{uno,\n  author = {A},\n  title = {T},\n  journal = {J},\n  year = 2020\n}\n\n@book{dos,\n  title = {T}\n}\n@Article{uno,\n  author={A}, title={T}, journaltitle={J}, date={2020}\n}\n@comment{nada, x}\n@misc{tres, title = {T}}\n";
        let tex = "\\cite{uno} \\parencite[p.~2]{tres, uno} % \\cite{dos}";
        let found = problems(&[source("main.tex", tex), source("refs.bib", bib)]);
        let messages: Vec<_> = found.iter().map(|t| (t.row, t.label.as_str())).collect();
        assert_eq!(
            messages,
            [
                (7, "«dos» (@book) no tiene author, publisher, year"),
                (7, "«dos» no se cita en ningún documento"),
                (10, "«uno» está repetida; ya aparece en refs.bib"),
            ]
        );
        // \nocite{*} usa todas las entradas.
        let found = problems(&[source("main.tex", "\\nocite{*}"), source("refs.bib", bib)]);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn recognises_identifiers_and_tidies_entries() {
        let doi = Some(("https://doi.org/10.1145/359576.359579".to_string(), true));
        assert_eq!(lookup(" 10.1145/359576.359579 "), doi);
        assert_eq!(lookup("https://doi.org/10.1145/359576.359579."), doi);
        assert_eq!(lookup("doi:10.1145/359576.359579"), doi);
        let arxiv = Some(("https://arxiv.org/bibtex/1706.03762".to_string(), false));
        assert_eq!(lookup("1706.03762"), arxiv);
        assert_eq!(lookup("arXiv:1706.03762v5"), arxiv);
        assert_eq!(lookup("https://arxiv.org/abs/1706.03762"), arxiv);
        assert_eq!(
            lookup("hep-th/9901001"),
            Some(("https://arxiv.org/bibtex/hep-th/9901001".to_string(), false))
        );
        assert_eq!(lookup("un título cualquiera"), None);
        assert_eq!(lookup("--output /tmp/x"), None);
        let line = " @article{Backus_1978, title={Can {A}, b}, year={1978}, month=Aug }";
        assert_eq!(key(line).as_deref(), Some("Backus_1978"));
        assert_eq!(
            tidy(line),
            "@article{Backus_1978,\n  title={Can {A}, b},\n  year={1978},\n  month=Aug\n}"
        );
        assert_eq!(
            tidy("@misc{a,\n  title={T}, \n}\n"),
            "@misc{a,\n  title={T},\n}"
        );
        assert_eq!(key("<html>"), None);
    }
}
