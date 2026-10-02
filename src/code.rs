//! Edición pensada para código: operaciones sobre líneas, corchetes
//! emparejados, sangría detectada, completado por palabras y esquema de
//! símbolos.

use std::collections::HashMap;

use super::{Completion, Editor, Pos, byte_col, regex};
use crate::format::Format;

/// Sangría con que está escrito un archivo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    Tabs,
    Spaces(usize),
}

/// Columnas que ocupa un tabulador al pintarlo (el valor de egui).
pub const TAB_COLUMNS: usize = 4;
/// Filas que se recorren como mucho al buscar el corchete emparejado.
const BRACKET_ROWS: usize = 5000;
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Columnas de sangría de una línea, con los tabuladores expandidos.
pub fn indent_columns(line: &str) -> usize {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { TAB_COLUMNS } else { 1 })
        .sum()
}

/// La sangría que usa el texto, si tiene la suficiente para deducirla.
pub fn detect_indent(lines: &[String]) -> Option<Indent> {
    let (mut tabs, mut spaces) = (0, 0);
    let mut steps = [0usize; 9];
    let mut previous = 0;
    for line in lines.iter().take(2000).filter(|l| !l.trim().is_empty()) {
        if line.starts_with('\t') {
            tabs += 1;
            previous = 0;
            continue;
        }
        let width = line.len() - line.trim_start_matches(' ').len();
        spaces += usize::from(width > 0);
        // Solo cuenta lo que se entra: al salir se pueden cerrar varios niveles.
        if width > previous && (2..=8).contains(&(width - previous)) {
            steps[width - previous] += 1;
        }
        previous = width;
    }
    if tabs > spaces {
        return Some(Indent::Tabs);
    }
    let (step, count) = steps
        .iter()
        .enumerate()
        .rev()
        .max_by_key(|(_, count)| **count)?;
    (*count > 0).then_some(Indent::Spaces(step))
}

fn keywords(language: &str) -> &'static [&'static str] {
    match language {
        "Rust" => &[
            "async", "await", "break", "const", "continue", "crate", "enum", "extern", "false",
            "impl", "loop", "match", "move", "pub", "return", "self", "Self", "static", "struct",
            "super", "trait", "true", "type", "unsafe", "where", "while", "String", "Option",
            "Result", "Some", "None", "usize", "Vec",
        ],
        "Python" => &[
            "and",
            "assert",
            "async",
            "await",
            "break",
            "class",
            "continue",
            "def",
            "elif",
            "else",
            "except",
            "False",
            "finally",
            "for",
            "from",
            "global",
            "import",
            "lambda",
            "None",
            "nonlocal",
            "not",
            "pass",
            "raise",
            "return",
            "True",
            "try",
            "while",
            "with",
            "yield",
            "print",
            "range",
            "self",
            "isinstance",
            "enumerate",
        ],
        "JavaScript" | "TypeScript" | "TypeScriptReact" | "JavaScript (Babel)" => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "import",
            "instanceof",
            "interface",
            "let",
            "new",
            "null",
            "return",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "undefined",
            "while",
            "yield",
            "console",
        ],
        "Go" => &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
            "error",
            "string",
            "false",
            "true",
            "nil",
        ],
        "C" | "C++" | "Objective-C" | "Objective-C++" => &[
            "auto",
            "bool",
            "break",
            "case",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "double",
            "else",
            "enum",
            "extern",
            "false",
            "float",
            "for",
            "include",
            "inline",
            "long",
            "namespace",
            "nullptr",
            "private",
            "protected",
            "public",
            "return",
            "short",
            "signed",
            "sizeof",
            "static",
            "struct",
            "switch",
            "template",
            "true",
            "typedef",
            "typename",
            "union",
            "unsigned",
            "using",
            "virtual",
            "void",
            "volatile",
            "while",
        ],
        "Java" | "C#" | "Kotlin" | "Scala" | "Dart" | "Swift" | "PHP" => &[
            "abstract",
            "boolean",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "else",
            "enum",
            "extends",
            "false",
            "final",
            "finally",
            "for",
            "function",
            "implements",
            "import",
            "interface",
            "new",
            "null",
            "override",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "static",
            "string",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "void",
            "while",
        ],
        _ => &[],
    }
}

/// Expresiones que reconocen una definición; `k` es su clase y `n` su nombre.
fn definitions(language: &str) -> &'static [&'static str] {
    match language {
        "Rust" => &[
            r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|default)\s+)*(?:extern\s+"[^"]*"\s+)?(?P<k>fn|struct|enum|trait|mod|type|union|macro_rules!)\s+(?P<n>\w+)"#,
            r"^\s*(?:unsafe\s+)?(?P<k>impl)(?:<[^>]*>)?\s+(?P<n>[^{]+?)\s*(?:\{|where\b|$)",
        ],
        "Python" => &[r"^\s*(?:async\s+)?(?P<k>def|class)\s+(?P<n>\w+)"],
        "JavaScript" | "TypeScript" | "TypeScriptReact" | "JavaScript (Babel)" => &[
            r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?(?:async\s+)?(?P<k>function\*?|class|interface|enum|type|namespace)\s+(?P<n>[\w$]+)",
            r"^\s*(?:export\s+)?(?P<k>const|let|var)\s+(?P<n>[\w$]+)\s*(?::[^=]+)?=\s*(?:async\s*)?(?:function\b|\([^)]*\)\s*(?::[^=]+)?=>|[\w$]+\s*=>)",
        ],
        "Go" => &[
            r"^(?P<k>func)\s+(?:\([^)]*\)\s*)?(?P<n>\w+)",
            r"^(?P<k>type)\s+(?P<n>\w+)",
        ],
        "Ruby" => &[r"^\s*(?P<k>def|class|module)\s+(?P<n>[\w.:?!]+)"],
        "Lua" => &[r"^\s*(?:local\s+)?(?P<k>function)\s+(?P<n>[\w.:]+)"],
        "Bourne Again Shell (bash)" | "Shell-Unix-Generic" => &[
            r"^\s*(?P<k>function)\s+(?P<n>[\w-]+)",
            r"^\s*(?P<n>[\w-]+)\s*\(\)",
        ],
        "SQL" => &[
            r#"(?i)^\s*create\s+(?:or\s+replace\s+)?(?:temp(?:orary)?\s+)?(?:unique\s+)?(?P<k>table|view|function|procedure|trigger|index)\s+(?:if\s+not\s+exists\s+)?(?P<n>[\w."`]+)"#,
        ],
        "Makefile" => &[r"^(?P<n>[\w./-]+)\s*:(?:[^=]|$)"],
        "TOML" | "INI" => &[r"^\s*\[+(?P<n>[^\]]+)\]"],
        _ => &[
            r"^\s*(?:(?:public|private|protected|internal|static|final|abstract|open|sealed|data|export|inline|virtual|override|partial|async)\s+)*(?P<k>class|struct|interface|enum|namespace|trait|object|protocol|extension|func|fun|function|def|module)\s+(?P<n>\w+)",
        ],
    }
}

/// Funciones al estilo de C: un tipo, un nombre y un paréntesis, sin `;`.
fn c_function(line: &str) -> Option<String> {
    const NOT_TYPES: [&str; 14] = [
        "return", "else", "new", "throw", "case", "delete", "await", "yield", "goto", "using",
        "typedef", "if", "while", "for",
    ];
    let found =
        regex(r"^\s*(?:[A-Za-z_][\w:<>,\*&\[\]]*[\s\*&]+)+(?P<n>[A-Za-z_~][\w:~]*)\s*\([^;]*$")
            .captures(line)?;
    let first = line.split_whitespace().next()?;
    let name = &found["n"];
    (!NOT_TYPES.contains(&first) && !NOT_TYPES.contains(&name) && name != "switch")
        .then(|| format!("{name}()"))
}

/// Definiciones del archivo como esquema: fila, nivel y título.
pub fn symbols(lines: &[String], language: &str, unit: usize) -> Vec<(usize, usize, String)> {
    let patterns = definitions(language);
    let c_like = matches!(
        language,
        "C" | "C++" | "Objective-C" | "Objective-C++" | "Java" | "C#" | "Dart"
    );
    let mut found = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let title = patterns
            .iter()
            .find_map(|pattern| {
                let m = regex(pattern).captures(line)?;
                let name = m.name("n")?.as_str().trim();
                Some(match m.name("k") {
                    Some(kind) => format!("{} {name}", kind.as_str().to_lowercase()),
                    None => name.to_string(),
                })
            })
            .or_else(|| c_like.then(|| c_function(line)).flatten());
        if let Some(title) = title {
            let level = 2 + (indent_columns(line) / unit.max(1)).min(6);
            found.push((row, level, title));
        }
    }
    found
}

impl Editor {
    /// Sangría del archivo: la detectada en el código, o la de las preferencias.
    pub fn indent_style(&self) -> Indent {
        if let Format::Code(name) = &self.format {
            if let Some(found) = self.indent {
                return found;
            }
            if matches!(name.as_str(), "Makefile" | "Go") {
                return Indent::Tabs;
            }
        }
        Indent::Spaces(self.tab)
    }
    /// Texto de un nivel de sangría.
    pub fn unit(&self) -> String {
        match self.indent_style() {
            Indent::Tabs => "\t".into(),
            Indent::Spaces(width) => " ".repeat(width),
        }
    }
    /// Columnas que ocupa en pantalla un nivel de sangría.
    pub fn unit_columns(&self) -> usize {
        match self.indent_style() {
            Indent::Tabs => TAB_COLUMNS,
            Indent::Spaces(width) => width.max(1),
        }
    }
    /// Primera y última fila que toca la selección.
    pub fn rows(&self) -> (usize, usize) {
        let (a, b) = self.selection();
        let last = if b.col == 0 && b.row > a.row {
            b.row - 1
        } else {
            b.row
        };
        (a.row, last)
    }
    fn width(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }
    fn leading(&self, row: usize) -> String {
        self.lines[row]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect()
    }
    /// Coloca el cursor (y el ancla) sin editar el texto.
    fn place(&mut self, cursor: Pos, anchor: Option<Pos>) {
        self.cursor = cursor;
        self.anchor = anchor.filter(|anchor| *anchor != cursor);
        self.selected = None;
        self.completions.clear();
    }

    /// Sube o baja las líneas de la selección.
    pub fn move_lines(&mut self, up: bool) -> bool {
        let (first, last) = self.rows();
        if !self.format.editable() || (up && first == 0) || (!up && last + 1 >= self.lines.len()) {
            return false;
        }
        let (cursor, anchor) = (self.cursor, self.anchor);
        let (from, to) = if up {
            (first - 1, last)
        } else {
            (first, last + 1)
        };
        let mut block = self.lines[from..=to].to_vec();
        if up {
            block.rotate_left(1);
        } else {
            block.rotate_right(1);
        }
        self.replace(
            Pos::new(from, 0),
            Pos::new(to, self.width(to)),
            &block.join("\n"),
        );
        let shift = |p: Pos| Pos::new(if up { p.row - 1 } else { p.row + 1 }, p.col);
        self.place(shift(cursor), anchor.map(shift));
        true
    }
    /// Repite debajo las líneas de la selección y deja el cursor en la copia.
    pub fn duplicate_lines(&mut self) {
        if !self.format.editable() {
            return;
        }
        let (first, last) = self.rows();
        let (cursor, anchor) = (self.cursor, self.anchor);
        let block = self.lines[first..=last].join("\n");
        let end = Pos::new(last, self.width(last));
        self.replace(end, end, &format!("\n{block}"));
        let shift = |p: Pos| Pos::new(p.row + last - first + 1, p.col);
        self.place(shift(cursor), anchor.map(shift));
    }
    /// Las líneas de la selección, con su salto final.
    pub fn line_text(&self) -> String {
        let (first, last) = self.rows();
        format!("{}\n", self.lines[first..=last].join("\n"))
    }
    /// Borra enteras las líneas de la selección.
    pub fn delete_lines(&mut self) {
        if !self.format.editable() {
            return;
        }
        let (first, last) = self.rows();
        let col = self.cursor.col;
        if last + 1 < self.lines.len() {
            self.replace(Pos::new(first, 0), Pos::new(last + 1, 0), "");
        } else if first > 0 {
            let above = Pos::new(first - 1, self.width(first - 1));
            self.replace(above, Pos::new(last, self.width(last)), "");
        } else {
            self.replace(Pos::new(0, 0), Pos::new(last, self.width(last)), "");
        }
        let row = first.min(self.lines.len() - 1);
        self.place(Pos::new(row, col.min(self.width(row))), None);
    }
    /// Selecciona la línea entera; repetido, añade la siguiente.
    pub fn select_line(&mut self) {
        let (a, b) = self.selection();
        let whole = a.col == 0 && b.col == 0 && b.row > a.row;
        let last = if whole { b.row } else { self.rows().1 };
        let end = if last + 1 < self.lines.len() {
            Pos::new(last + 1, 0)
        } else {
            self.end()
        };
        self.place(end, Some(Pos::new(a.row, 0)));
    }
    /// Abre una línea nueva debajo (o encima) sin partir la actual.
    pub fn open_line(&mut self, above: bool) {
        if !self.format.editable() {
            return;
        }
        let (first, last) = self.rows();
        if above {
            let indent = self.leading(first);
            let start = Pos::new(first, 0);
            self.replace(start, start, &format!("{indent}\n"));
            self.place(Pos::new(first, indent.chars().count()), None);
        } else {
            self.place(Pos::new(last, self.width(last)), None);
            self.newline();
        }
    }
    /// Inicio inteligente: va al primer carácter de la línea y, si ya está ahí, a la columna 0.
    pub fn smart_home(&mut self, select: bool) {
        let first = self.leading(self.cursor.row).chars().count();
        let col = if self.cursor.col == first { 0 } else { first };
        let anchor = select.then(|| self.anchor.unwrap_or(self.cursor));
        self.place(Pos::new(self.cursor.row, col), anchor);
    }
    /// Inserta sangría en el cursor, hasta la siguiente parada si son espacios.
    pub fn indent_cursor(&mut self) {
        let text = match self.indent_style() {
            Indent::Tabs => "\t".to_string(),
            Indent::Spaces(width) => {
                let width = width.max(1);
                " ".repeat(width - self.selection().0.col % width)
            }
        };
        self.insert(&text);
    }

    /// Columnas de la palabra que toca `pos`.
    pub fn word_at(&self, pos: Pos) -> Option<(usize, usize)> {
        let chars: Vec<char> = self.lines[pos.row].chars().collect();
        let mut start = pos.col.min(chars.len());
        let mut end = start;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        while end < chars.len() && is_word(chars[end]) {
            end += 1;
        }
        (start < end).then_some((start, end))
    }
    /// Sin selección, selecciona la palabra del cursor; con ella, salta a su
    /// siguiente aparición.
    pub fn select_next(&mut self) -> bool {
        let (a, b) = self.selection();
        if a == b {
            let Some((start, end)) = self.word_at(a) else {
                return false;
            };
            self.place(Pos::new(a.row, end), Some(Pos::new(a.row, start)));
            return true;
        }
        let needle = self.selected();
        if needle.contains('\n') {
            return false;
        }
        let count = self.lines.len();
        // La fila del cursor se mira dos veces: tras él y, al dar la vuelta, antes.
        for step in 0..=count {
            let row = (b.row + step) % count;
            let line = &self.lines[row];
            let from = if step == 0 { byte_col(line, b.col) } else { 0 };
            if let Some(at) = line[from..].find(&needle) {
                let start = line[..from + at].chars().count();
                let found = Pos::new(row, start);
                if found == a {
                    return false;
                }
                self.place(Pos::new(row, start + needle.chars().count()), Some(found));
                return true;
            }
        }
        false
    }

    fn escaped(&self, previous: Option<char>) -> bool {
        self.format == Format::Latex && previous == Some('\\')
    }
    /// El cierre sin pareja más cercano desde `from` hacia delante. Solo cuentan
    /// los corchetes escapados (`\{`) o solo los normales, según `escaped`.
    fn scan_close(&self, from: Pos, open: char, close: char, escaped: bool) -> Option<Pos> {
        let mut depth = 0usize;
        let last = self.lines.len().min(from.row + BRACKET_ROWS);
        for row in from.row..last {
            let skip = if row == from.row { from.col } else { 0 };
            let mut previous = None;
            for (col, c) in self.lines[row].chars().enumerate() {
                if col >= skip && self.escaped(previous) == escaped {
                    if c == open {
                        depth += 1;
                    } else if c == close {
                        if depth == 0 {
                            return Some(Pos::new(row, col));
                        }
                        depth -= 1;
                    }
                }
                previous = Some(c);
            }
        }
        None
    }
    /// La apertura sin pareja más cercana antes de `from`.
    fn scan_open(&self, from: Pos, open: char, close: char, escaped: bool) -> Option<Pos> {
        let mut depth = 0usize;
        let first = from.row.saturating_sub(BRACKET_ROWS);
        for row in (first..=from.row).rev() {
            let chars: Vec<char> = self.lines[row].chars().collect();
            let end = if row == from.row {
                from.col.min(chars.len())
            } else {
                chars.len()
            };
            for col in (0..end).rev() {
                let c = chars[col];
                if (c != open && c != close)
                    || self.escaped(col.checked_sub(1).map(|before| chars[before])) != escaped
                {
                    continue;
                }
                if c == close {
                    depth += 1;
                } else if depth == 0 {
                    return Some(Pos::new(row, col));
                } else {
                    depth -= 1;
                }
            }
        }
        None
    }
    fn pair_of(&self, at: Pos) -> Option<(Pos, Pos)> {
        let c = self.lines[at.row].chars().nth(at.col)?;
        let before = at
            .col
            .checked_sub(1)
            .map(|col| self.char_at(Pos::new(at.row, col)));
        let escaped = self.escaped(before);
        for (open, close) in PAIRS {
            if c == open {
                let after = Pos::new(at.row, at.col + 1);
                return self
                    .scan_close(after, open, close, escaped)
                    .map(|m| (at, m));
            }
            if c == close {
                return self.scan_open(at, open, close, escaped).map(|m| (at, m));
            }
        }
        None
    }
    /// El corchete junto al cursor y su pareja.
    pub fn matching_bracket(&self) -> Option<(Pos, Pos)> {
        if let Some((revision, cursor, found)) = self.bracket.get()
            && revision == self.revision
            && cursor == self.cursor
        {
            return found;
        }
        let before = (self.cursor.col > 0).then(|| self.previous(self.cursor));
        let found = [Some(self.cursor), before]
            .into_iter()
            .flatten()
            .find_map(|at| self.pair_of(at));
        self.bracket.set(Some((self.revision, self.cursor, found)));
        found
    }
    /// Lleva el cursor a la pareja del corchete que tiene al lado.
    pub fn jump_bracket(&mut self) -> bool {
        let Some((_, other)) = self.matching_bracket() else {
            return false;
        };
        self.place(other, None);
        true
    }
    /// Al escribir un cierre en una línea en blanco, lo alinea con su apertura.
    pub(super) fn closes_block(&mut self, c: char) -> bool {
        let Some((open, _)) = PAIRS.into_iter().find(|(_, close)| *close == c) else {
            return false;
        };
        let (a, b) = self.selection();
        let line = &self.lines[a.row];
        let before = &line[..byte_col(line, a.col)];
        if !matches!(self.format, Format::Code(_))
            || a != b
            || before.is_empty()
            || !before.trim().is_empty()
            || self.char_at(a) == c
        {
            return false;
        }
        let Some(opener) = self.scan_open(a, open, c, false) else {
            return false;
        };
        let indent = self.leading(opener.row);
        self.replace(Pos::new(a.row, 0), a, &format!("{indent}{c}"));
        true
    }

    /// Columna donde empieza y texto de la palabra a medio escribir.
    pub(super) fn word_prefix(&self) -> Option<(usize, String)> {
        let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
        let end = self.cursor.col.min(chars.len());
        if chars.get(end).is_some_and(|c| is_word(*c)) {
            return None;
        }
        let mut start = end;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        (end - start >= 2 && !chars[start].is_numeric())
            .then(|| (start, chars[start..end].iter().collect()))
    }
    /// Sugiere las palabras del documento (y las del lenguaje) que empiezan
    /// como la que se está escribiendo, las más cercanas primero.
    pub(super) fn complete_words(&mut self) {
        let Some((start, prefix)) = self.word_prefix() else {
            return;
        };
        if self.quiet == Some((self.revision, self.cursor)) {
            return;
        }
        let here = self.offset(Pos::new(self.cursor.row, start));
        let text = &self.text;
        let mut nearest: HashMap<&str, usize> = HashMap::new();
        for (at, _) in text.match_indices(&prefix) {
            if at == here || text[..at].chars().next_back().is_some_and(is_word) {
                continue;
            }
            let end = text[at..]
                .find(|c: char| !is_word(c))
                .map_or(text.len(), |n| at + n);
            if end - at > prefix.len() {
                let distance = at.abs_diff(here);
                nearest
                    .entry(&text[at..end])
                    .and_modify(|d| *d = distance.min(*d))
                    .or_insert(distance);
            }
        }
        let mut words: Vec<(usize, String, &str)> = nearest
            .into_iter()
            .map(|(word, distance)| (distance, word.to_string(), "Palabra del documento"))
            .collect();
        words.sort();
        if let Format::Code(language) = &self.format {
            for keyword in keywords(language) {
                if keyword.len() > prefix.len()
                    && keyword.starts_with(&prefix)
                    && !words.iter().any(|(_, word, _)| word == *keyword)
                {
                    words.push((usize::MAX, keyword.to_string(), "Palabra del lenguaje"));
                }
            }
        }
        self.completions = words
            .into_iter()
            .take(30)
            .map(|(_, word, detail)| Completion {
                label: word.clone(),
                insert: word,
                detail: detail.into(),
                start,
                kind: "word".into(),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust(text: &str) -> Editor {
        Editor::untitled(text.into(), "main.rs")
    }

    #[test]
    fn line_operations_keep_the_selection() {
        let mut e = rust("uno\ndos\ntres");
        e.goto(1, 2);
        assert!(e.move_lines(true));
        assert_eq!(e.text(), "dos\nuno\ntres");
        assert_eq!(e.cursor, Pos::new(0, 2));
        assert!(!e.move_lines(true));
        e.anchor = Some(Pos::new(0, 0));
        e.cursor = Pos::new(1, 3);
        assert!(e.move_lines(false));
        assert_eq!(e.text(), "tres\ndos\nuno");
        assert_eq!(e.selection(), (Pos::new(1, 0), Pos::new(2, 3)));
        assert!(!e.move_lines(false));
        e.duplicate_lines();
        assert_eq!(e.text(), "tres\ndos\nuno\ndos\nuno");
        assert_eq!(e.selection(), (Pos::new(3, 0), Pos::new(4, 3)));
        assert_eq!(e.line_text(), "dos\nuno\n");
        e.delete_lines();
        assert_eq!(e.text(), "tres\ndos\nuno");
        assert_eq!(e.cursor, Pos::new(2, 3));
        e.undo(false);
        assert_eq!(e.text(), "tres\ndos\nuno\ndos\nuno");
        e.goto(0, 1);
        e.select_line();
        assert_eq!(e.selected(), "tres\n");
        e.select_line();
        assert_eq!(e.selected(), "tres\ndos\n");
        e.delete_lines();
        assert_eq!(e.text(), "uno\ndos\nuno");
        e.goto(0, 0);
        assert!(e.select_next());
        assert_eq!(e.selected(), "uno");
        assert!(e.select_next());
        assert_eq!(e.selection(), (Pos::new(2, 0), Pos::new(2, 3)));
        assert!(e.select_next());
        assert_eq!(e.selection(), (Pos::new(0, 0), Pos::new(0, 3)));
    }

    #[test]
    fn brackets_indentation_and_new_lines() {
        let mut e = rust("fn f() {\n    if x {\n        g(a[1]);\n    }\n}");
        e.goto(0, 8);
        assert_eq!(e.matching_bracket(), Some((Pos::new(0, 7), Pos::new(4, 0))));
        assert!(e.jump_bracket());
        assert_eq!(e.cursor, Pos::new(4, 0));
        assert!(e.jump_bracket());
        assert_eq!(e.cursor, Pos::new(0, 7));
        e.goto(2, 13);
        assert_eq!(
            e.matching_bracket(),
            Some((Pos::new(2, 13), Pos::new(2, 11)))
        );
        e.goto(2, 3);
        assert_eq!(e.matching_bracket(), None);
        e.smart_home(false);
        assert_eq!(e.cursor.col, 8);
        e.smart_home(true);
        assert_eq!(e.selection(), (Pos::new(2, 0), Pos::new(2, 8)));
        e.goto(2, 5);
        e.open_line(false);
        assert_eq!(e.lines[3], "        ");
        assert_eq!(e.cursor, Pos::new(3, 8));
        // Un cierre en una línea en blanco vuelve a la sangría de su apertura.
        e.smart_char('}');
        assert_eq!(e.lines[3], "    }");
        e.undo(false);
        e.goto(1, 6);
        e.open_line(true);
        assert_eq!(e.lines[1], "    ");
        assert_eq!(e.cursor, Pos::new(1, 4));

        let mut tex = Editor::new("\\{ a {b} \\}".into(), None);
        tex.goto(0, 5);
        assert_eq!(
            tex.matching_bracket(),
            Some((Pos::new(0, 5), Pos::new(0, 7)))
        );
        tex.goto(0, 1);
        assert_eq!(
            tex.matching_bracket(),
            Some((Pos::new(0, 1), Pos::new(0, 10)))
        );

        let mut tabs = Editor::untitled(
            "all:\n\tcc main.c\n\nclean:\n\trm -f a.out".into(),
            "Makefile",
        );
        assert_eq!(tabs.indent_style(), Indent::Tabs);
        tabs.goto(1, 0);
        tabs.rewrite_lines(false, false);
        assert_eq!(tabs.lines[1], "\t\tcc main.c");
        tabs.rewrite_lines(false, true);
        assert_eq!(tabs.lines[1], "\tcc main.c");
        let mut two = Editor::untitled("a:\n  b:\n    c: 1\n  d: 2".into(), "datos.yaml");
        assert_eq!(two.indent_style(), Indent::Spaces(2));
        two.goto(3, 1);
        two.indent_cursor();
        assert_eq!(two.lines[3], "   d: 2");
        let mut py = Editor::untitled("def f(x):\n    return x".into(), "f.py");
        py.goto(1, 12);
        py.newline();
        assert_eq!(py.cursor, Pos::new(2, 0));
        py.goto(0, 9);
        py.newline();
        assert_eq!(py.cursor, Pos::new(1, 4));
    }

    #[test]
    fn word_completion_prefers_nearby_words() {
        let mut e = rust("let contador = 1;\nlet contexto = 2;\nco");
        e.goto(2, 2);
        e.update_completion();
        let labels: Vec<_> = e.completions.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["contexto", "contador", "const", "continue"]);
        e.accept_completion();
        assert_eq!(e.lines[2], "contexto");
        // Tras aceptar no vuelve a sugerir hasta que se siga escribiendo.
        e.update_completion();
        assert!(e.completions.is_empty());
        e.insert("\ncont");
        e.update_completion();
        assert_eq!(e.completions[0].label, "contexto");
        e.insert("exto");
        e.update_completion();
        assert!(e.completions.is_empty());
        e.goto(0, 6);
        e.update_completion();
        assert!(e.completions.is_empty());
    }

    #[test]
    fn symbols_by_language() {
        let titles = |text: &str, name: &str| -> Vec<(usize, usize, String)> {
            Editor::untitled(text.into(), name).outline().to_vec()
        };
        assert_eq!(
            titles(
                "pub struct A;\nimpl<T> Tr for A {\n    pub(crate) async fn go(&self) {}\n}\nfn main() {}",
                "a.rs"
            ),
            [
                (0, 2, "struct A".into()),
                (1, 2, "impl Tr for A".into()),
                (2, 3, "fn go".into()),
                (4, 2, "fn main".into())
            ]
        );
        assert_eq!(
            titles("class A:\n    def f(self):\n        pass\n", "a.py"),
            [(0, 2, "class A".into()), (1, 3, "def f".into())]
        );
        assert_eq!(
            titles(
                "export const f = async (x) => x;\nfunction g() {}\nconst n = 1;",
                "a.js"
            ),
            [(0, 2, "const f".into()), (1, 2, "function g".into())]
        );
        assert_eq!(
            titles(
                "#include <stdio.h>\nstatic int add(int a, int b) {\n    return add(a,\n        b);\n}\nint main(void)\n{\n    if (x) {\n    }\n    else if (y) {}\n    printf(\"%d\", 1);\n}",
                "a.c"
            ),
            [(1, 2, "add()".into()), (5, 2, "main()".into())]
        );
    }
}
