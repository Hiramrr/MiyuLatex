use std::{
    any::TypeId,
    cell::{Cell, OnceCell},
    collections::BTreeMap,
    fs, io,
    ops::Range,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, Instant},
};

use eframe::egui::{self, text::CharIndex};
use regex::Regex;
use serde::Deserialize;

use crate::{
    config,
    format::{self, Format},
    highlight,
    latex::{self, Source},
    syntax::Syntax,
};

#[path = "code.rs"]
pub mod code;

#[derive(Deserialize)]
pub struct Command {
    pub name: String,
    pub snippet: String,
    pub help: String,
}
#[derive(Deserialize)]
pub struct Symbol {
    #[serde(rename = "glyph")]
    pub char: String,
    #[serde(rename = "command")]
    pub latex: String,
    pub name: String,
    pub group: String,
}
#[derive(Deserialize)]
pub struct Template {
    pub title: String,
    pub description: String,
    pub filename: String,
    #[serde(rename = "body")]
    pub text: String,
}
#[derive(Deserialize)]
pub struct Catalog {
    pub commands: Vec<Command>,
    pub symbols: Vec<Symbol>,
    pub templates: Vec<Template>,
    pub environments: BTreeMap<String, String>,
    pub env_args: BTreeMap<String, String>,
    pub list_envs: Vec<String>,
}
pub fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("snippets.json")).expect("catálogo integrado")
    })
}

pub fn regex(pattern: &'static str) -> &'static Regex {
    static REGEXES: OnceLock<std::sync::Mutex<BTreeMap<&'static str, &'static Regex>>> =
        OnceLock::new();
    let mut map = REGEXES.get_or_init(Default::default).lock().unwrap();
    map.entry(pattern)
        .or_insert_with(|| Box::leak(Box::new(Regex::new(pattern).expect("expresión integrada"))))
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Pos {
    pub row: usize,
    pub col: usize,
}
impl Pos {
    pub fn new(row: usize, col: usize) -> Self {
        Self { row, col }
    }
}

#[derive(Clone)]
struct Snapshot {
    text: String,
    cursor: Pos,
    anchor: Option<Pos>,
}

#[derive(Clone)]
pub struct Completion {
    pub label: String,
    pub insert: String,
    pub detail: String,
    pub start: usize,
    pub kind: String,
}

pub struct Editor {
    pub path: Option<PathBuf>,
    pub suggested_name: String,
    pub format: Format,
    pub lines: Vec<String>,
    /// Las líneas unidas con `\n`, siempre al día.
    text: String,
    /// Cambia con cada edición del texto.
    pub revision: u64,
    pub syntax: Syntax,
    pub saved: String,
    pub cursor: Pos,
    pub anchor: Option<Pos>,
    /// Se calcula al pedirlo y se descarta con cada edición.
    outline: OnceCell<Vec<(usize, usize, String)>>,
    pub matches: Vec<(Pos, Pos)>,
    pub query: String,
    /// Opciones de la búsqueda: mayúsculas exactas, palabra completa y expresión regular.
    pub search_case: bool,
    pub search_word: bool,
    pub search_regex: bool,
    pub completions: Vec<Completion>,
    pub completion_index: usize,
    /// Espacios por nivel de sangría.
    pub tab: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    line_ending: &'static str,
    /// El widget de texto editó el documento desde la última consulta.
    touched: bool,
    /// Última selección recibida del widget: revisión e índices de carácter.
    selected: Option<(u64, usize, usize)>,
    /// Sangría detectada al abrir el archivo; solo se aplica al código.
    indent: Option<code::Indent>,
    /// Dónde y cuándo acabó la última letra tecleada: las seguidas se deshacen juntas.
    typing: Option<(Pos, Instant)>,
    /// Corchete emparejado ya calculado para una revisión y un cursor.
    #[allow(clippy::type_complexity)]
    bracket: Cell<Option<(u64, Pos, Option<(Pos, Pos)>)>>,
    /// Tras aceptar una palabra no se sugiere nada hasta que cambie algo.
    quiet: Option<(u64, Pos)>,
}

pub fn byte_col(line: &str, col: usize) -> usize {
    line.char_indices().nth(col).map_or(line.len(), |(i, _)| i)
}

impl Editor {
    pub fn new(text: String, path: Option<PathBuf>) -> Self {
        let format = path.as_deref().map_or(Format::Latex, Format::detect);
        let line_ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let normalized = text.replace("\r\n", "\n");
        let lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
        let mut e = Self {
            path,
            suggested_name: "sin-titulo.tex".into(),
            syntax: Syntax::new(&format, &lines),
            format,
            indent: code::detect_indent(&lines),
            lines,
            text: normalized,
            revision: 0,
            saved: text,
            cursor: Pos::default(),
            anchor: None,
            outline: OnceCell::new(),
            matches: Vec::new(),
            query: String::new(),
            search_case: false,
            search_word: false,
            search_regex: false,
            completions: Vec::new(),
            completion_index: 0,
            tab: 4,
            undo: Vec::new(),
            redo: Vec::new(),
            line_ending,
            touched: false,
            selected: None,
            typing: None,
            bracket: Cell::new(None),
            quiet: None,
        };
        e.refresh();
        e
    }
    pub fn untitled(text: String, filename: &str) -> Self {
        let mut editor = Self::new(text, None);
        editor.suggested_name = filename.into();
        editor.set_format(Format::detect(std::path::Path::new(filename)));
        editor
    }
    fn set_format(&mut self, format: Format) {
        if self.format != format {
            self.syntax = Syntax::new(&format, &self.lines);
            self.format = format;
        }
        self.refresh();
    }
    pub fn index(&self, pos: Pos) -> usize {
        self.lines[..pos.row]
            .iter()
            .map(|s| s.chars().count() + 1)
            .sum::<usize>()
            + pos.col
    }
    pub fn position(&self, mut index: usize) -> Pos {
        for (row, line) in self.lines.iter().enumerate() {
            let count = line.chars().count();
            if index <= count {
                return Pos::new(row, index);
            }
            index -= count + 1;
        }
        self.end()
    }
    pub fn set_text(&mut self, text: &str) {
        if !self.format.editable() || self.text == text {
            return;
        }
        self.remember();
        self.load(text);
        self.goto(self.cursor.row, self.cursor.col);
    }
    /// Selección que informa el widget de texto, en índices de carácter.
    pub fn select(&mut self, primary: usize, secondary: usize) {
        let selected = Some((self.revision, primary, secondary));
        if self.selected != selected {
            self.selected = selected;
            self.cursor = self.position(primary);
            self.anchor = (primary != secondary).then(|| self.position(secondary));
        }
    }
    /// Si el widget de texto editó el documento desde la última consulta.
    pub fn take_touched(&mut self) -> bool {
        std::mem::take(&mut self.touched)
    }
    /// Avanza el resaltado pendiente y devuelve si aún queda.
    pub fn highlight(&mut self) -> bool {
        // El de LaTeX es rápido y el esquema sale de él; el de syntect se
        // reparte entre cuadros para no detener la interfaz.
        let budget = (self.format != Format::Latex).then_some(Duration::from_millis(5));
        self.syntax.advance(&self.lines, budget)
    }
    pub fn outline(&self) -> &[(usize, usize, String)] {
        self.outline.get_or_init(|| match self.format {
            Format::Markdown => format::markdown_outline(&self.text),
            Format::Latex => self.latex_outline(),
            Format::Code(ref language) => code::symbols(&self.lines, language, self.unit_columns()),
            _ => Vec::new(),
        })
    }
    fn latex_outline(&self) -> Vec<(usize, usize, String)> {
        let mut outline = Vec::new();
        for (row, spans) in self.syntax.sections(&self.lines) {
            let chars: Vec<_> = self.lines[row].chars().collect();
            for span in spans {
                if span.tok != highlight::Tok::Section {
                    continue;
                }
                if let Some((a, b)) = highlight::arg_span(&chars, span.end) {
                    let command: String = chars[span.start + 1..span.end].iter().collect();
                    let level = highlight::SECTION_COMMANDS
                        .iter()
                        .position(|s| *s == command.trim_end_matches('*'))
                        .unwrap_or(2);
                    let title: String = chars[a..b].iter().collect();
                    let title = regex(r"\\[a-zA-Z@]+\*?(\[[^\]]*\])?")
                        .replace_all(&title, "")
                        .replace(['{', '}'], "");
                    outline.push((row, level, title));
                }
            }
        }
        outline
    }
    pub fn backspace(&mut self) {
        let (a, b) = self.selection();
        if a != b {
            self.replace(a, b, "");
            return;
        }
        let before = self.previous(a);
        let pair = match (self.char_at(before), self.char_at(a)) {
            ('(', ')') | ('[', ']') | ('{', '}') => true,
            ('$', '$') => self.format == Format::Latex,
            ('"', '"') | ('\'', '\'') => matches!(self.format, Format::Code(_)),
            _ => false,
        };
        self.replace(before, if pair { self.next(a) } else { a }, "");
    }
    pub fn text(&self) -> String {
        self.text.clone()
    }
    /// El texto completo, sin copiarlo.
    pub fn source(&self) -> &str {
        &self.text
    }
    fn disk_text(&self) -> String {
        if self.line_ending == "\n" {
            self.text.clone()
        } else {
            self.text.replace('\n', self.line_ending)
        }
    }
    pub fn dirty(&self) -> bool {
        // Se consulta en cada cuadro: compara sin construir el texto de disco.
        self.format.editable()
            && if self.line_ending == "\n" {
                self.text != self.saved
            } else {
                !self.text.split('\n').eq(self.saved.split(self.line_ending))
            }
    }
    pub fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or(self.suggested_name.clone(), |n| {
                n.to_string_lossy().into_owned()
            })
    }
    pub fn end(&self) -> Pos {
        Pos::new(
            self.lines.len() - 1,
            self.lines.last().unwrap().chars().count(),
        )
    }
    fn offset(&self, p: Pos) -> usize {
        self.lines[..p.row]
            .iter()
            .map(|s| s.len() + 1)
            .sum::<usize>()
            + byte_col(&self.lines[p.row], p.col)
    }
    pub fn selection(&self) -> (Pos, Pos) {
        let a = self.anchor.unwrap_or(self.cursor);
        (a.min(self.cursor), a.max(self.cursor))
    }
    pub fn selected(&self) -> String {
        let (a, b) = self.selection();
        self.text[self.offset(a)..self.offset(b)].into()
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            cursor: self.cursor,
            anchor: self.anchor,
        }
    }
    fn remember(&mut self) {
        // ponytail: instantáneas hasta 16 MiB, usar un rope si hacen falta documentos mayores.
        self.typing = None;
        self.undo.push(self.snapshot());
        let mut bytes = self.undo.iter().map(|s| s.text.len()).sum::<usize>();
        while self.undo.len() > 1 && (bytes > 16 * 1024 * 1024 || self.undo.len() > 200) {
            bytes -= self.undo.remove(0).text.len();
        }
        self.redo.clear();
    }
    fn restore(&mut self, s: Snapshot) {
        self.load(&s.text);
        self.cursor = s.cursor;
        self.anchor = s.anchor;
        self.typing = None;
    }
    /// Sustituye el tramo `[a, b)` por `text` sin tocar el historial ni el cursor.
    fn splice(&mut self, a: Pos, b: Pos, text: &str) {
        self.text
            .replace_range(self.offset(a)..self.offset(b), text);
        let head = &self.lines[a.row][..byte_col(&self.lines[a.row], a.col)];
        let tail = &self.lines[b.row][byte_col(&self.lines[b.row], b.col)..];
        let rows: Vec<String> = format!("{head}{text}{tail}")
            .split('\n')
            .map(str::to_string)
            .collect();
        self.syntax.edit(a.row, b.row - a.row + 1, &rows);
        self.lines.splice(a.row..=b.row, rows);
        self.refresh();
    }
    /// Sustituye todo el texto conservando las líneas que no cambian.
    fn load(&mut self, text: &str) {
        let new: Vec<&str> = text.split('\n').collect();
        let same = |(old, new): &(&String, &&str)| old.as_str() == **new;
        let head = self.lines.iter().zip(&new).take_while(same).count();
        let tail = self.lines[head..]
            .iter()
            .rev()
            .zip(new[head..].iter().rev())
            .take_while(same)
            .count();
        let removed = self.lines.len() - head - tail;
        let rows: Vec<String> = new[head..new.len() - tail]
            .iter()
            .map(|line| line.to_string())
            .collect();
        self.text.clear();
        self.text.push_str(text);
        self.syntax.edit(head, removed, &rows);
        self.lines.splice(head..head + removed, rows);
        self.refresh();
    }
    /// Edición que llega del widget de texto: un paso de historial por cuadro,
    /// y uno solo para las letras de una palabra tecleadas seguidas.
    fn widget_edit(&mut self, a: Pos, b: Pos, text: &str) {
        if !self.format.editable() || (a == b && text.is_empty()) {
            return;
        }
        let mut chars = text.chars();
        let typed = match (chars.next(), chars.next()) {
            (Some(c), None) if a == b && c != '\n' => Some(c),
            _ => None,
        };
        let continues = typed.is_some_and(|c| !c.is_whitespace())
            && self
                .typing
                .is_some_and(|(at, when)| at == a && when.elapsed() < Duration::from_secs(1));
        if !self.touched {
            if continues {
                self.redo.clear();
            } else {
                self.remember();
            }
            self.touched = true;
        }
        self.splice(a, b, text);
        self.typing = typed.map(|_| (Pos::new(a.row, a.col + 1), Instant::now()));
    }
    pub fn undo(&mut self, redo: bool) {
        if !self.format.editable() {
            return;
        }
        let s = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        if let Some(s) = s {
            let now = self.snapshot();
            if redo {
                self.undo.push(now)
            } else {
                self.redo.push(now)
            }
            self.restore(s);
        }
    }
    pub fn replace(&mut self, a: Pos, b: Pos, text: &str) {
        if !self.format.editable() {
            return;
        }
        self.remember();
        self.splice(a, b, text);
        let parts: Vec<_> = text.split('\n').collect();
        self.cursor = Pos::new(
            a.row + parts.len() - 1,
            if parts.len() == 1 {
                a.col + text.chars().count()
            } else {
                parts.last().unwrap().chars().count()
            },
        );
        self.anchor = None;
    }
    pub fn insert(&mut self, text: &str) {
        let (a, b) = self.selection();
        self.replace(a, b, text);
    }
    pub fn snippet(&mut self, snippet: &str, a: Pos, b: Pos) {
        let indent: String = self.lines[a.row]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let text = snippet.replace('\n', &format!("\n{indent}"));
        let marker = text.find("$0");
        let clean = text.replacen("$0", "", 1);
        self.replace(a, b, &clean);
        if let Some(i) = marker {
            let head = &text[..i];
            let rows = head.matches('\n').count();
            self.cursor = Pos::new(
                a.row + rows,
                if rows == 0 {
                    a.col + head.chars().count()
                } else {
                    head.rsplit('\n').next().unwrap().chars().count()
                },
            );
        }
    }
    pub fn wrap(&mut self, before: &str, after: &str) {
        if !self.format.editable() {
            return;
        }
        let (a, b) = self.selection();
        let selected = self.selected();
        self.replace(a, b, &format!("{before}{selected}{after}"));
        if a == b {
            self.cursor = Pos::new(a.row, a.col + before.chars().count());
        }
    }
    pub fn emphasize(&mut self, bold: bool) {
        match self.format {
            Format::Latex => self.wrap(if bold { "\\textbf{" } else { "\\textit{" }, "}"),
            Format::Markdown => {
                let marker = if bold { "**" } else { "*" };
                self.wrap(marker, marker);
            }
            _ => {}
        }
    }
    /// Adopta el texto que otro programa dejó en disco; se puede deshacer.
    pub fn reload(&mut self, disk: String) {
        self.line_ending = if disk.contains("\r\n") { "\r\n" } else { "\n" };
        self.set_text(&disk.replace("\r\n", "\n"));
        self.saved = disk;
    }
    /// El archivo se renombró en disco sin cambiar su contenido.
    pub fn renamed(&mut self, path: PathBuf) {
        let format = Format::detect(&path);
        self.path = Some(path);
        self.set_format(format);
    }
    pub fn save(&mut self, path: Option<PathBuf>) -> io::Result<()> {
        if !self.format.editable() {
            return Err(io::Error::other("Este archivo se abre en modo de lectura"));
        }
        let path = path.map(|p| p.canonicalize().unwrap_or(p));
        let destination = path
            .as_ref()
            .or(self.path.as_ref())
            .ok_or_else(|| io::Error::other("Elige un nombre de archivo"))?;
        let format = Format::detect(destination);
        if !format.editable() {
            return Err(io::Error::other(
                "Guarda el texto con una extensión de texto, no como PDF o imagen",
            ));
        }
        if destination.exists() {
            let disk = fs::read_to_string(destination)?;
            if self.path.as_ref() != Some(destination) || disk != self.saved {
                return Err(io::Error::other(
                    "El archivo cambió en disco o ya existe. Usa Guardar como con otro nombre.",
                ));
            }
        }
        let text = self.disk_text();
        if latex::is_source(destination) && destination.exists() && text != self.saved {
            latex::checkpoint(destination, &self.saved)?;
        }
        config::atomic_write(destination, text.as_bytes())?;
        self.path = Some(destination.clone());
        self.saved = text;
        self.set_format(format);
        Ok(())
    }
    /// Pone al día lo que depende del texto tras una edición.
    fn refresh(&mut self) {
        self.revision += 1;
        self.highlight();
        self.outline.take();
        if self.query.is_empty() {
            self.matches.clear();
        } else {
            let query = std::mem::take(&mut self.query);
            self.search(&query);
        }
        self.completions.clear();
    }
    pub fn search(&mut self, query: &str) {
        self.query = query.into();
        self.matches.clear();
        if query.is_empty() {
            return;
        }
        let mut pat = if self.search_regex {
            query.to_string()
        } else {
            regex::escape(query)
        };
        if self.search_word {
            pat = format!(r"\b(?:{pat})\b");
        }
        // Sin la opción, una mayúscula en la búsqueda la hace exacta.
        if !self.search_case && !query.chars().any(char::is_uppercase) {
            pat = format!("(?i){pat}");
        }
        // Una expresión a medio escribir no es un error: aún no encuentra nada.
        let Ok(re) = Regex::new(&pat) else {
            return;
        };
        for (row, line) in self.lines.iter().enumerate() {
            for m in re.find_iter(line).filter(|m| !m.is_empty()) {
                self.matches.push((
                    Pos::new(row, line[..m.start()].chars().count()),
                    Pos::new(row, line[..m.end()].chars().count()),
                ));
            }
        }
    }
    pub fn find_next(&mut self, backwards: bool) {
        let current = self.selection().0;
        let found = if backwards {
            self.matches
                .iter()
                .rev()
                .find(|(a, _)| *a < current)
                .or(self.matches.last())
        } else {
            self.matches
                .iter()
                .find(|(a, _)| *a > current)
                .or(self.matches.first())
        };
        if let Some(&(a, b)) = found {
            self.cursor = b;
            self.anchor = Some(a);
            self.selected = None;
        }
    }
    pub fn replace_all(&mut self, replacement: &str) {
        if self.matches.is_empty() {
            return;
        }
        let mut starts = Vec::with_capacity(self.lines.len());
        let mut start = 0;
        for line in &self.lines {
            starts.push(start);
            start += line.len() + 1;
        }
        let offset = |p: Pos| starts[p.row] + byte_col(&self.lines[p.row], p.col);
        let mut text = self.text.clone();
        for &(a, b) in self.matches.iter().rev() {
            text.replace_range(offset(a)..offset(b), replacement);
        }
        self.remember();
        self.load(&text);
        self.cursor = self.end();
        self.anchor = None;
    }
    pub fn goto(&mut self, row: usize, col: usize) {
        self.cursor = Pos::new(row.min(self.lines.len() - 1), col);
        self.cursor.col = self
            .cursor
            .col
            .min(self.lines[self.cursor.row].chars().count());
        self.anchor = None;
        self.selected = None;
        self.completions.clear();
    }
    fn previous(&self, p: Pos) -> Pos {
        if p.col > 0 {
            Pos::new(p.row, p.col - 1)
        } else if p.row > 0 {
            Pos::new(p.row - 1, self.lines[p.row - 1].chars().count())
        } else {
            p
        }
    }
    fn next(&self, p: Pos) -> Pos {
        if p.col < self.lines[p.row].chars().count() {
            Pos::new(p.row, p.col + 1)
        } else if p.row + 1 < self.lines.len() {
            Pos::new(p.row + 1, 0)
        } else {
            p
        }
    }
    fn char_at(&self, p: Pos) -> char {
        self.lines[p.row].chars().nth(p.col).unwrap_or('\n')
    }
    pub fn smart_char(&mut self, c: char) {
        if !self.format.editable() {
            return;
        }
        let (a, b) = self.selection();
        if c == '\'' && self.format.label() == "Rust" && a == b {
            self.insert("'");
            return;
        }
        if self.closes_block(c) {
            return;
        }
        let following = self.char_at(self.cursor);
        let previous = if self.cursor.col > 0 {
            self.char_at(self.previous(self.cursor))
        } else {
            '\n'
        };
        let closer = match c {
            '{' => Some('}'),
            '[' => Some(']'),
            '(' => Some(')'),
            '$' if self.format == Format::Latex => Some('$'),
            '"' | '\'' if matches!(self.format, Format::Code(_)) => Some(c),
            _ => None,
        };
        if c == '$' && self.format == Format::Latex && a == b {
            let line = &self.lines[self.cursor.row];
            let before = &line[..byte_col(line, self.cursor.col)];
            if matches!(
                highlight::tokenize_line(before, &highlight::State::default())
                    .1
                    .math,
                Some(highlight::Math::Dollar)
            ) {
                if following == '$' {
                    self.cursor = self.next(self.cursor);
                } else {
                    self.insert("$");
                }
                return;
            }
        }
        if a == b
            && ("}])".contains(c) || (closer == Some(c)))
            && following == c
            && previous != '\\'
        {
            self.cursor = self.next(self.cursor);
            return;
        }
        if let Some(close) = closer {
            if a != b {
                self.wrap(&c.to_string(), &close.to_string());
                return;
            }
            if previous != '\\' && !following.is_alphanumeric() && following != '\\' {
                self.insert(&format!("{c}{close}"));
                self.cursor = self.previous(self.cursor);
                return;
            }
        }
        self.insert(&c.to_string());
    }
    pub fn newline(&mut self) {
        if !self.format.editable() {
            return;
        }
        let (a, b) = self.selection();
        if a != b {
            let indent: String = self.lines[a.row]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            self.replace(a, b, &format!("\n{indent}"));
            return;
        }
        let line = &self.lines[self.cursor.row];
        let before = &line[..byte_col(line, self.cursor.col)];
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if self.format != Format::Latex {
            let at = self.offset(a);
            let inside_code = self.format == Format::Markdown
                && pulldown_cmark::Parser::new(&self.text)
                    .into_offset_iter()
                    .any(|(event, range)| {
                        matches!(
                            event,
                            pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(_))
                        ) && range.start <= at
                            && at <= range.end
                    });
            if self.format == Format::Markdown
                && !inside_code
                && let Some(list) =
                    regex(r"^(\s*)([-+*]|\d+[.)])\s+(?:\[([ xX])\]\s+)?(.*)$").captures(before)
            {
                if list[4].trim().is_empty() {
                    self.replace(Pos::new(a.row, 0), a, &indent);
                } else {
                    let marker = if let Some(number) = list[2].strip_suffix(['.', ')']) {
                        number
                            .parse::<usize>()
                            .ok()
                            .and_then(|n| n.checked_add(1))
                            .map_or(list[2].to_string(), |n| {
                                format!("{n}{}", list[2].chars().last().unwrap())
                            })
                    } else {
                        list[2].to_string()
                    };
                    let check = if list.get(3).is_some() { "[ ] " } else { "" };
                    self.insert(&format!("\n{indent}{marker} {check}"));
                }
            } else if matches!(self.format, Format::Code(_))
                && (before.trim_end().ends_with(['{', '[', '('])
                    || (self.format.label() == "Python" && before.trim_end().ends_with(':')))
            {
                let row = a.row;
                let after = &line[byte_col(line, a.col)..];
                let tail = if after.starts_with(['}', ']', ')']) {
                    format!("\n{indent}")
                } else {
                    String::new()
                };
                let unit = self.unit();
                self.insert(&format!("\n{indent}{unit}{tail}"));
                self.cursor = Pos::new(row + 1, (indent + &unit).chars().count());
            } else if self.format.label() == "Python"
                && regex(r"^\s*(return|pass|break|continue|raise)\b").is_match(before)
            {
                // Tras salir del bloque, la línea siguiente pierde un nivel.
                let unit = self.unit();
                let indent = indent.strip_suffix(unit.as_str()).unwrap_or(&indent);
                self.insert(&format!("\n{indent}"));
            } else {
                self.insert(&format!("\n{indent}"));
            }
            return;
        }
        let env = regex(r"\\begin\{([^{}]+)\}(?:\[[^\]]*\]|\{[^{}]*\})*\s*$")
            .captures(before)
            .map(|m| m[1].to_string());
        if let Some(env) = env {
            let body = if catalog().list_envs.contains(&env) {
                "\\item "
            } else {
                ""
            };
            let row = self.cursor.row;
            let text = self.text();
            let tail = if text.matches(&format!("\\begin{{{env}}}")).count()
                > text.matches(&format!("\\end{{{env}}}")).count()
            {
                format!("\n{indent}\\end{{{env}}}")
            } else {
                String::new()
            };
            let pad = " ".repeat(self.tab);
            self.insert(&format!("\n{indent}{pad}{body}{tail}"));
            self.cursor = Pos::new(
                row + 1,
                indent.chars().count() + self.tab + body.chars().count(),
            );
        } else if before.trim() == "\\item" {
            self.replace(
                Pos::new(self.cursor.row, 0),
                Pos::new(self.cursor.row, line.chars().count()),
                "",
            );
        } else {
            let body = if before.trim_start().starts_with("\\item ") {
                "\\item "
            } else {
                ""
            };
            self.insert(&format!("\n{indent}{body}"));
        }
    }
    pub fn rewrite_lines(&mut self, comment: bool, dedent: bool) {
        if !self.format.editable() {
            return;
        }
        let marker = self.format.comment();
        if comment && marker.is_none() {
            return;
        }
        let (prefix, suffix) = marker.unwrap_or(("", ""));
        let (a, b) = self.selection();
        let last = if b.col == 0 && b.row > a.row {
            b.row - 1
        } else {
            b.row
        };
        let uncomment = comment
            && self.lines[a.row..=last]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .all(|l| l.trim_start().starts_with(prefix) && l.trim_end().ends_with(suffix));
        let unit = self.unit();
        let tab = unit
            .chars()
            .count()
            .max(if unit == "\t" { self.tab } else { 0 });
        let lines: Vec<_> = self.lines[a.row..=last]
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    return line.clone();
                }
                if comment {
                    let indent = line.len() - line.trim_start().len();
                    let rest = &line[indent..];
                    format!(
                        "{}{}",
                        &line[..indent],
                        if uncomment {
                            let rest = if suffix.is_empty() {
                                rest
                            } else {
                                rest.trim_end()
                            };
                            let rest = rest.strip_suffix(suffix).unwrap();
                            let rest = rest.strip_prefix(prefix).unwrap();
                            let rest = rest.strip_prefix(' ').unwrap_or(rest);
                            if suffix.is_empty() {
                                rest.to_string()
                            } else {
                                rest.strip_suffix(' ').unwrap_or(rest).to_string()
                            }
                        } else {
                            format!(
                                "{prefix} {rest}{}",
                                if suffix.is_empty() {
                                    String::new()
                                } else {
                                    format!(" {suffix}")
                                }
                            )
                        }
                    )
                } else if dedent {
                    line.strip_prefix('\t')
                        .unwrap_or_else(|| {
                            &line[line.chars().take(tab).take_while(|c| *c == ' ').count()..]
                        })
                        .to_string()
                } else {
                    format!("{unit}{line}")
                }
            })
            .collect();
        self.replace(
            Pos::new(a.row, 0),
            Pos::new(last, self.lines[last].chars().count()),
            &lines.join("\n"),
        );
        if a != b {
            self.anchor = Some(Pos::new(a.row, 0));
        }
    }
    /// Si hay algo que completar ante el cursor: todas las sugerencias siguen
    /// a un comando, así que sin `\` no hace falta reunir las fuentes.
    pub fn wants_completion(&self) -> bool {
        let line = &self.lines[self.cursor.row];
        self.anchor.is_none_or(|anchor| anchor == self.cursor)
            && match self.format {
                Format::Latex => line[..byte_col(line, self.cursor.col)].contains('\\'),
                // En el código se completan las palabras del propio documento.
                Format::Code(_) => self.word_prefix().is_some(),
                _ => false,
            }
    }
    pub fn update_completion(&mut self) {
        if !self.wants_completion() {
            self.completions.clear();
            self.completion_index = 0;
            return;
        }
        if self.format != Format::Latex {
            self.update_completion_from(&[]);
            return;
        }
        let mut sources = Vec::new();
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent())
            && let Ok(files) = fs::read_dir(dir)
        {
            for file in files
                .flatten()
                .filter(|f| f.path().extension().is_some_and(|e| e == "bib"))
                .take(20)
            {
                if let Ok(text) = fs::read_to_string(file.path()) {
                    sources.push(Source {
                        path: file.path(),
                        text,
                    });
                }
            }
        }
        self.update_completion_from(&sources);
    }
    pub fn update_completion_from(&mut self, sources: &[Source]) {
        self.completions.clear();
        self.completion_index = 0;
        if !self.wants_completion() {
            return;
        }
        if self.format != Format::Latex {
            self.complete_words();
            return;
        }
        let line = &self.lines[self.cursor.row];
        let before = &line[..byte_col(line, self.cursor.col)];
        let state = self.syntax.latex_before(self.cursor.row);
        let (spans, _) = highlight::tokenize_line(before, &state);
        if spans.iter().any(|s| {
            matches!(s.tok, highlight::Tok::Comment | highlight::Tok::Verbatim)
                && s.end == self.cursor.col
        }) {
            return;
        }
        let current_path = self
            .path
            .clone()
            .unwrap_or_else(|| PathBuf::from(&self.suggested_name));
        let mut sources = sources.to_vec();
        if let Some(source) = sources.iter_mut().find(|s| s.path == current_path) {
            source.text = self.text();
        } else {
            sources.push(Source {
                path: current_path,
                text: self.text(),
            });
        }
        if let Some(m) = regex(r"\\(begin|end)\s*\{([a-zA-Z*]*)$").captures(before) {
            let prefix = &m[2];
            let start = self.cursor.col - prefix.chars().count();
            let mut names: Vec<_> = catalog().environments.keys().cloned().collect();
            for source in &sources {
                for entry in regex(r"\\(?:newenvironment|renewenvironment|newtheorem)\*?\s*\{([^}]+)\}").captures_iter(&latex::code(&source.text)) {
                    if !names.iter().any(|name| name == &entry[1]) { names.push(entry[1].into()); }
                }
            }
            if &m[1] == "end" {
                let text_before = self.text[..self.offset(self.cursor)].to_string();
                let mut stack = Vec::new();
                for entry in regex(r"\\(begin|end)\s*\{([^}]+)\}").captures_iter(&latex::code(&text_before)) {
                    if &entry[1] == "begin" { stack.push(entry[2].to_owned()); }
                    else if stack.last().is_some_and(|name| name == &entry[2]) { stack.pop(); }
                }
                if let Some(name) = stack.last() {
                    names.retain(|item| item != name);
                    names.insert(0, name.clone());
                }
            }
            for name in names.into_iter().filter(|name| name.starts_with(prefix)) {
                self.completions.push(Completion { label: name.clone(), insert: name, detail: "Entorno".into(), start, kind: format!("env.{}", &m[1]) });
            }
        } else if let Some(m) = regex(r"\\(?:ref|eqref|pageref|autoref|cref|Cref|nameref|vref)\*?(?:\[[^\]]*\])*\s*\{(?:[^{}]*,\s*)?([^{} ,]*)$").captures(before) {
            let prefix = m[1].to_lowercase();
            let start = self.cursor.col - m[1].chars().count();
            for target in latex::labels(&sources) {
                if target.label.to_lowercase().contains(&prefix) && !self.completions.iter().any(|c| c.insert == target.label) {
                    self.completions.push(Completion { label: target.label.clone(), insert: target.label, detail: format!("{}:{}", target.path.display(), target.row + 1), start, kind: "label".into() });
                }
            }
        } else if let Some(m) = regex(r"\\(?:[cC]ite[a-zA-Z]*|[pP]arencite|[tT]extcite|[aA]utocite|footcite|nocite|smartcite|supercite)\*?(?:\[[^\]]*\])*\s*\{(?:[^{}]*,\s*)?([^{} ,]*)$").captures(before) {
            let prefix = m[1].to_lowercase();
            let start = self.cursor.col - m[1].chars().count();
            for target in latex::citations(&sources) {
                if format!("{} {}", target.label, target.detail).to_lowercase().contains(&prefix)
                    && !self.completions.iter().any(|c| c.insert == target.label)
                {
                    self.completions.push(Completion { label: target.label.clone(), insert: target.label, detail: target.detail, start, kind: "cite".into() });
                }
            }
        } else if let Some(m) = regex(r"\\(input|include|subfile|addbibresource|bibliography|includegraphics)\*?(?:\[[^\]]*\])*\s*\{([^{}]*)$").captures(before) {
            let prefix = m[2].to_lowercase();
            let start = self.cursor.col - m[2].chars().count();
            for source in &sources {
                let valid = match &m[1] {
                    "bibliography" | "addbibresource" => source.path.extension().is_some_and(|e| e == "bib"),
                    "includegraphics" => matches!(crate::format::Format::detect(&source.path), Format::Pdf | Format::Image),
                    _ => source.path.extension().is_some_and(|e| e == "tex"),
                };
                if !valid { continue; }
                let base = sources.first().and_then(|s| s.path.parent()).or_else(|| self.path.as_ref().and_then(|p| p.parent()));
                let Some(base) = base else { continue };
                let mut name = source.path.strip_prefix(base).unwrap_or(&source.path).to_path_buf();
                if matches!(&m[1], "input" | "include" | "subfile" | "bibliography") { name.set_extension(""); }
                let name = name.to_string_lossy().replace('\\', "/");
                if name.to_lowercase().starts_with(&prefix) {
                    self.completions.push(Completion { label: name.clone(), insert: name, detail: "Archivo del proyecto".into(), start, kind: "file".into() });
                }
            }
        } else if let Some(m) = regex(r"\\([a-zA-Z]+)$").captures(before) {
            let prefix = &m[1];
            let start = self.cursor.col - prefix.chars().count() - 1;
            let mut found: Vec<_> = catalog().commands.iter().filter(|c| c.name.to_lowercase().starts_with(&prefix.to_lowercase())).collect();
            found.sort_by_key(|c| c.name != prefix);
            for c in found.into_iter().take(60) {
                self.completions.push(Completion { label: format!("\\{}", c.name), insert: c.snippet.clone(), detail: c.help.clone(), start, kind: "command".into() });
            }
            for source in &sources {
                for m in regex(r"\\(?:newcommand|renewcommand|providecommand|DeclareMathOperator)\*?\s*\{?\\([a-zA-Z]+)\}?\s*(?:\[(\d+)\])?|\\def\s*\\([a-zA-Z]+)").captures_iter(&latex::code(&source.text)) {
                    let name = m.get(1).or_else(|| m.get(3)).unwrap().as_str();
                    let label = format!("\\{name}");
                    if name.to_lowercase().starts_with(&prefix.to_lowercase()) && !self.completions.iter().any(|c| c.label == label) {
                        let args = m.get(2).and_then(|m| m.as_str().parse::<usize>().ok()).unwrap_or(0).min(9);
                        let insert = if args == 0 { label.clone() } else { format!("{label}{{$0}}{}", "{}".repeat(args - 1)) };
                        self.completions.push(Completion { label, insert, detail: "Comando del proyecto".into(), start, kind: "command".into() });
                    }
                }
            }
        }
        self.completions.truncate(60);
    }
    pub fn accept_completion(&mut self) {
        let Some(c) = self.completions.get(self.completion_index).cloned() else {
            return;
        };
        let a = Pos::new(self.cursor.row, c.start);
        let mut b = self.cursor;
        if c.kind == "word" {
            self.replace(a, b, &c.insert);
            self.quiet = Some((self.revision, self.cursor));
            return;
        }
        let closes = self.char_at(b) == '}';
        if c.kind == "env.begin" {
            if closes {
                b.col += 1;
            }
            let body = catalog()
                .environments
                .get(&c.insert)
                .map_or("$0", String::as_str);
            let args = catalog().env_args.get(&c.insert).map_or("", String::as_str);
            let text = self.text();
            let tail = if text.matches(&format!("\\begin{{{}}}", c.insert)).count()
                >= text.matches(&format!("\\end{{{}}}", c.insert)).count()
            {
                format!("\n\\end{{{}}}", c.insert)
            } else {
                String::new()
            };
            let pad = " ".repeat(self.tab);
            self.snippet(
                &format!(
                    "{}}}{args}\n{pad}{}{tail}",
                    c.insert,
                    body.replace('\n', &format!("\n{pad}"))
                ),
                a,
                b,
            );
        } else if c.kind == "command" {
            if closes && c.insert.ends_with("{$0}") {
                b.col += 1;
            }
            self.snippet(&c.insert, a, b);
        } else {
            while self.char_at(b).is_alphanumeric()
                || matches!(self.char_at(b), ':' | '_' | '-' | '.' | '/')
            {
                b = self.next(b);
            }
            self.replace(a, b, &c.insert);
            if self.char_at(self.cursor) == '}' && c.kind != "cite" {
                self.cursor.col += 1;
            } else if self.char_at(self.cursor) != '}' {
                self.insert("}");
            }
        }
        self.update_completion();
    }
}

/// El widget de texto de egui edita el documento directamente, sin copiarlo
/// en cada cuadro, y `layout` lo recupera desde el búfer para maquetarlo.
impl egui::TextBuffer for Editor {
    fn is_mutable(&self) -> bool {
        self.format.editable()
    }
    fn as_str(&self) -> &str {
        &self.text
    }
    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        let at = self.position(char_index.0);
        self.widget_edit(at, at, text);
        text.chars().count()
    }
    fn delete_char_range(&mut self, char_range: Range<CharIndex>) {
        let (a, b) = (
            self.position(char_range.start.0),
            self.position(char_range.end.0),
        );
        self.widget_edit(a, b, "");
    }
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_editing_and_safe_save() {
        let mut md = Editor::untitled(
            "# Título **uno**\n\n```md\n# no es un título\n```\n\nSegundo\n-------".into(),
            "nota.MD",
        );
        assert_eq!(md.format, Format::Markdown);
        assert_eq!(
            md.outline(),
            [(0, 2, "Título uno".into()), (6, 3, "Segundo".into())]
        );
        md.goto(0, 2);
        md.anchor = Some(Pos::new(0, 8));
        md.emphasize(true);
        assert!(md.lines[0].starts_with("# **Título**"));
        md.goto(0, 0);
        let original = md.text();
        md.rewrite_lines(true, false);
        assert!(md.lines[0].starts_with("<!-- #"));
        md.goto(0, 0);
        md.rewrite_lines(true, false);
        assert_eq!(md.text(), original);
        md = Editor::untitled("- [x] tarea ñ".into(), "nota.md");
        md.goto(0, md.end().col);
        md.newline();
        assert_eq!(md.text(), "- [x] tarea ñ\n- [ ] ");
        md.newline();
        assert_eq!(md.text(), "- [x] tarea ñ\n");
        md.smart_char('$');
        assert_eq!(md.lines[1], "$");
        md.set_text("```text\n- ejemplo");
        md.goto(1, 9);
        md.newline();
        assert_eq!(md.text(), "```text\n- ejemplo\n");
        md.set_text("\\sect");
        md.goto(0, 5);
        md.update_completion();
        assert!(md.completions.is_empty());
        for (filename, marker) in [
            ("main.rs", "//"),
            ("main.py", "#"),
            ("main.sql", "--"),
            ("main.css", "/*"),
        ] {
            let mut code = Editor::untitled("    ñ = 1;  ".into(), filename);
            assert!(matches!(code.format, Format::Code(_)));
            let original = code.text();
            code.rewrite_lines(true, false);
            assert!(code.text().trim_start().starts_with(marker));
            code.goto(0, 0);
            code.rewrite_lines(true, false);
            assert_eq!(code.text(), original);
        }
        let mut code = Editor::untitled("fn main() {}".into(), "main.rs");
        code.goto(0, 11);
        code.newline();
        assert_eq!(code.text(), "fn main() {\n    \n}");
        assert_eq!(code.cursor, Pos::new(1, 4));
        code.smart_char('"');
        code.backspace();
        assert_eq!(code.text(), "fn main() {\n    \n}");
        code.smart_char('\'');
        code.insert("static");
        assert_eq!(code.lines[1], "    'static");
        let mut json = Editor::untitled("{}".into(), "data.json");
        json.rewrite_lines(true, false);
        assert_eq!(json.text(), "{}");
        let mut pdf = Editor::new(String::new(), Some(PathBuf::from("file.PDF")));
        pdf.insert("no cambiar");
        pdf.set_text("no cambiar");
        assert!(!pdf.dirty());
        assert_eq!(pdf.text(), "");
        assert!(pdf.save(None).is_err());
        let folder = std::env::temp_dir().join(format!("miyu-formats-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("windows.py");
        fs::write(&path, "# ñ\r\nprint(1)\r\n").unwrap();
        let mut code = Editor::new(fs::read_to_string(&path).unwrap(), Some(path.clone()));
        code.insert("# comentario\n");
        code.save(None).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# comentario\r\n# ñ\r\nprint(1)\r\n"
        );
        let copy = folder.join("nota.md");
        code.save(Some(copy)).unwrap();
        assert_eq!(code.format, Format::Markdown);
        assert!(!code.dirty());
        assert!(code.save(Some(folder.join("bad.pdf"))).is_err());
        assert!(!folder.join("bad.pdf").exists());
        fs::remove_dir_all(folder).unwrap();
    }
    fn type_text(e: &mut Editor, text: &str) {
        for c in text.chars() {
            e.smart_char(c);
            e.update_completion();
        }
    }
    #[test]
    fn refuses_external_changes_and_existing_save_as() {
        let dir = std::env::temp_dir().join(format!("miyu-save-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.tex");
        fs::write(&path, "original").unwrap();
        let mut e = Editor::new("original".into(), Some(path.clone()));
        e.insert("nuevo ");
        fs::write(&path, "cambio externo").unwrap();
        assert!(e.save(None).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "cambio externo");
        let other = dir.join("otro.tex");
        fs::write(&other, "no borrar").unwrap();
        assert!(e.save(Some(other.clone())).is_err());
        assert_eq!(fs::read_to_string(other).unwrap(), "no borrar");
        let copy = dir.join("copia.tex");
        e.save(Some(copy.clone())).unwrap();
        assert_eq!(fs::read_to_string(copy).unwrap(), "nuevo original");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unicode_editing_environment_search_and_undo() {
        let mut e = Editor::new(String::new(), None);
        type_text(&mut e, "ñ \\beg");
        e.accept_completion();
        type_text(&mut e, "enum");
        e.accept_completion();
        type_text(&mut e, "uno");
        e.newline();
        type_text(&mut e, "dos");
        assert_eq!(
            e.text(),
            "ñ \\begin{enumerate}\n    \\item uno\n    \\item dos\n\\end{enumerate}"
        );
        e.search("ñ");
        assert_eq!(e.matches[0], (Pos::new(0, 0), Pos::new(0, 1)));
        e.replace_all("sí");
        assert!(e.text().starts_with("sí "));
        e.undo(false);
        assert!(e.text().starts_with("ñ "));
        e.undo(true);
        assert!(e.text().starts_with("sí "));
        let mut e = Editor::new("á(x)".into(), None);
        e.goto(0, 3);
        e.backspace();
        assert_eq!(e.text(), "á()");
        e.backspace();
        assert_eq!(e.text(), "á");
    }
}
