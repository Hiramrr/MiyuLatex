use std::{path::Path, sync::OnceLock};

use egui_extras::syntax_highlighting::SyntectSettings;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Format {
    Latex,
    Markdown,
    Code(String),
    Text,
    Pdf,
    Image,
}

pub fn syntax_settings() -> &'static SyntectSettings {
    static SETTINGS: OnceLock<SyntectSettings> = OnceLock::new();
    // Las gramáticas de syntect más las de bat, que añaden lenguajes recientes.
    SETTINGS.get_or_init(|| SyntectSettings {
        ps: two_face::syntax::extra_newlines(),
        ..Default::default()
    })
}

impl Format {
    pub fn detect(path: &Path) -> Self {
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        match ext.as_str() {
            "tex" | "sty" | "cls" | "ltx" | "tikz" => Self::Latex,
            "md" | "markdown" | "mdown" | "mkd" => Self::Markdown,
            "pdf" => Self::Pdf,
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => Self::Image,
            _ => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let syntaxes = &syntax_settings().ps;
                syntaxes
                    .find_syntax_by_extension(&ext)
                    .or_else(|| syntaxes.find_syntax_by_extension(&name))
                    .filter(|s| s.name != "Plain Text")
                    .map_or(Self::Text, |s| Self::Code(s.name.clone()))
            }
        }
    }

    pub fn editable(&self) -> bool {
        !matches!(self, Self::Pdf | Self::Image)
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Latex => "LaTeX",
            Self::Markdown => "Markdown",
            Self::Code(name) => name,
            Self::Text => "Texto",
            Self::Pdf => "PDF · solo lectura",
            Self::Image => "Imagen · solo lectura",
        }
    }

    pub fn comment(&self) -> Option<(&str, &str)> {
        match self {
            Self::Latex => Some(("%", "")),
            Self::Markdown => Some(("<!--", "-->")),
            Self::Code(name) => match name.as_str() {
                "Python" | "Ruby" | "Shell-Unix-Generic" | "YAML" | "TOML" | "Makefile"
                | "Dockerfile" | "Perl" | "R" => Some(("#", "")),
                "SQL" | "Lua" | "Haskell" => Some(("--", "")),
                "HTML" | "XML" => Some(("<!--", "-->")),
                "CSS" => Some(("/*", "*/")),
                "Rust" | "JavaScript" | "TypeScript" | "C" | "C++" | "C#" | "Java" | "Go"
                | "Swift" | "Objective-C" | "Objective-C++" | "PHP" | "Scala" | "Kotlin"
                | "Dart" => Some(("//", "")),
                "LaTeX" | "BibTeX" | "MATLAB" | "Erlang" => Some(("%", "")),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn listed(path: &Path) -> bool {
        !matches!(Self::detect(path), Self::Text)
            || matches!(
                path.extension().and_then(|s| s.to_str()),
                Some(
                    "txt"
                        | "csv"
                        | "tsv"
                        | "log"
                        | "toml"
                        | "ini"
                        | "conf"
                        | "ts"
                        | "tsx"
                        | "jsx"
                        | "vue"
                )
            )
            || matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some("LICENSE" | "Dockerfile" | "Makefile" | "CMakeLists.txt")
            )
    }
}

pub fn markdown_outline(text: &str) -> Vec<(usize, usize, String)> {
    let mut outline = Vec::new();
    let mut heading = None;
    // Fila del último título, para contar saltos solo desde ahí.
    let (mut row, mut counted) = (0, 0);
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                row += text[counted..range.start]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count();
                counted = range.start;
                heading = Some((row, level as usize + 1, String::new()));
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, title)) = &mut heading {
                    title.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(item) = heading.take() {
                    outline.push(item);
                }
            }
            _ => {}
        }
    }
    outline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_languages_have_a_grammar() {
        for (file, name) in [
            ("main.ts", "TypeScript"),
            ("App.tsx", "TypeScriptReact"),
            ("Main.kt", "Kotlin"),
            ("App.swift", "Swift"),
            ("Cargo.toml", "TOML"),
            ("main.rs", "Rust"),
            ("main.py", "Python"),
        ] {
            assert_eq!(
                Format::detect(Path::new(file)),
                Format::Code(name.into()),
                "{file}"
            );
        }
    }
}
