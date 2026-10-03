//! Corrector ortográfico de la prosa de LaTeX, Markdown y texto. Usa el
//! diccionario del sistema en macOS; en otras plataformas queda desactivado.

use crate::{
    config::Config,
    editor::{Editor, Pos},
    format::Format,
    highlight::{self, Tok},
};
use eframe::egui::{self, Color32, epaint::text::Glyph};
use std::{
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// Espera tras la última edición antes de consultar el diccionario.
const PAUSE: Duration = Duration::from_millis(350);
/// Tiempo de diccionario por cuadro; el resto de filas espera al siguiente.
const BUDGET: Duration = Duration::from_millis(3);

#[cfg(target_os = "macos")]
mod system {
    use objc2::rc::Retained;
    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSArray, NSRange, NSString};

    fn strings(array: Option<Retained<NSArray<NSString>>>) -> Vec<String> {
        array.map_or(Vec::new(), |array| {
            (0..array.count())
                .map(|i| array.objectAtIndex(i).to_string())
                .collect()
        })
    }
    /// Tramos, en unidades UTF-16, de las palabras que no están en el diccionario.
    pub fn check(text: &str, language: &str) -> Vec<(usize, usize)> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let string = NSString::from_str(text);
        let language = NSString::from_str(language);
        let length = string.length();
        let mut found = Vec::new();
        let mut offset = 0;
        while offset < length {
            // SAFETY: el contador de palabras es opcional y se pasa nulo.
            let range = unsafe {
                checker
                    .checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(
                        &string,
                        offset as isize,
                        Some(&language),
                        false,
                        0,
                        std::ptr::null_mut(),
                    )
            };
            if range.length == 0 || range.location >= length {
                break;
            }
            found.push((range.location, range.location + range.length));
            offset = range.location + range.length;
        }
        found
    }
    pub fn guesses(word: &str, language: &str) -> Vec<String> {
        let string = NSString::from_str(word);
        strings(
            NSSpellChecker::sharedSpellChecker()
                .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                    NSRange::new(0, string.length()),
                    &string,
                    Some(&NSString::from_str(language)),
                    0,
                ),
        )
    }
    pub fn learn(word: &str) {
        NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
    }
    pub fn languages() -> Vec<String> {
        strings(Some(
            NSSpellChecker::sharedSpellChecker().availableLanguages(),
        ))
    }
}

#[cfg(not(target_os = "macos"))]
mod system {
    pub fn check(_: &str, _: &str) -> Vec<(usize, usize)> {
        Vec::new()
    }
    pub fn guesses(_: &str, _: &str) -> Vec<String> {
        Vec::new()
    }
    pub fn learn(_: &str) {}
    pub fn languages() -> Vec<String> {
        Vec::new()
    }
}

/// Idiomas con diccionario en este equipo.
pub fn languages() -> &'static [String] {
    static LANGUAGES: OnceLock<Vec<String>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        let mut languages = system::languages();
        languages.sort();
        languages
    })
}

/// Comandos cuyos primeros argumentos entre llaves no son prosa.
fn code_arguments(name: &str) -> usize {
    match name {
        "textcolor" | "colorbox" | "href" | "parbox" | "raisebox" | "scalebox" | "newtheorem"
        | "fcolorbox" | "multirow" | "rule" | "foreignlanguage" => 1,
        "multicolumn" | "resizebox" => 2,
        "vspace"
        | "hspace"
        | "setlength"
        | "addtolength"
        | "newcommand"
        | "renewcommand"
        | "providecommand"
        | "newenvironment"
        | "renewenvironment"
        | "DeclareMathOperator"
        | "color"
        | "definecolor"
        | "pagestyle"
        | "thispagestyle"
        | "setcounter"
        | "addtocounter"
        | "geometry"
        | "hypersetup"
        | "graphicspath"
        | "fontsize"
        | "selectlanguage"
        | "bibitem"
        | "lstset"
        | "usetikzlibrary"
        | "pgfplotsset"
        | "captionsetup"
        | "titleformat"
        | "titlespacing"
        | "setmainfont"
        | "setsansfont"
        | "setmonofont"
        | "newcolumntype"
        | "includepdf"
        | "lstinputlisting"
        | "newlength"
        | "numberwithin"
        | "linespread"
        | "PassOptionsToPackage"
        | "pagenumbering"
        | "cline"
        | "newcounter"
        | "stepcounter"
        | "refstepcounter"
        | "DeclareUnicodeCharacter"
        | "hyphenation"
        | "setstretch"
        | "columnbreak"
        | "tikzset"
        | "draw"
        | "node"
        | "path"
        | "fill"
        | "addplot"
        | "lstinline"
        | "verb"
        | "mintinline"
        | "setlist"
        | "fancyhf"
        | "fancyhead"
        | "fancyfoot"
        | "theoremstyle"
        | "newacronym"
        | "gls"
        | "glspl"
        | "Gls"
        | "acrshort"
        | "acrlong"
        | "acrfull"
        | "si"
        | "SI"
        | "num"
        | "unit"
        | "qty"
        | "cellcolor"
        | "rowcolor"
        | "columncolor"
        | "arraystretch"
        | "setcitestyle"
        | "addcontentsline"
        | "phantomsection"
        | "subfile"
        | "subimport"
        | "import"
        | "inputminted"
        | "newglossaryentry"
        | "printbibliography"
        | "ExecuteBibliographyOptions"
        | "DeclareFieldFormat"
        | "AtBeginDocument"
        | "makeatletter"
        | "def"
        | "let" => usize::MAX,
        _ => 0,
    }
}
/// Entornos cuyos argumentos son medidas, columnas u opciones.
const CODE_ENVIRONMENTS: &[&str] = &[
    "tabular",
    "tabular*",
    "tabularx",
    "longtable",
    "array",
    "minipage",
    "figure",
    "figure*",
    "table",
    "table*",
    "wrapfigure",
    "subfigure",
    "subtable",
    "thebibliography",
    "multicols",
    "enumerate",
    "itemize",
    "description",
    "tikzpicture",
    "axis",
    "spacing",
    "adjustwidth",
    "column",
    "columns",
    "adjustbox",
    "tcolorbox",
    "algorithm",
    "algorithmic",
    "listing",
];

/// Final (exclusivo) del grupo que abre en `start`; hasta el fin de línea si no cierra.
fn group_end(chars: &[char], start: usize) -> usize {
    let (open, close) = (chars[start], if chars[start] == '{' { '}' } else { ']' });
    let mut depth = 0;
    for (i, c) in chars.iter().enumerate().skip(start) {
        if *c == open && (i == 0 || chars[i - 1] != '\\') {
            depth += 1;
        } else if *c == close && (i == 0 || chars[i - 1] != '\\') {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
    }
    chars.len()
}

/// Columnas de una línea LaTeX que no son prosa: comandos, matemáticas,
/// comentarios, claves de referencias y argumentos técnicos.
pub fn latex_mask(line: &str, state: &highlight::State) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    let mut mask = vec![false; chars.len()];
    let (spans, _) = highlight::tokenize_line(line, state);
    for span in &spans {
        let prose = matches!(
            span.tok,
            Tok::Title | Tok::Str | Tok::Bold | Tok::Italic | Tok::Underline
        );
        if !prose {
            mask[span.start..span.end.min(chars.len())].fill(true);
        }
    }
    for span in &spans {
        if !matches!(span.tok, Tok::Command | Tok::Item | Tok::Section | Tok::Env) {
            continue;
        }
        let name: String = chars[span.start + 1..span.end.min(chars.len())]
            .iter()
            .filter(|c| **c != '*')
            .collect();
        let mut pos = span.end;
        while chars.get(pos) == Some(&'*') {
            pos += 1;
        }
        let mut braces = code_arguments(&name);
        let mut optional = !matches!(
            name.as_str(),
            "item"
                | "caption"
                | "footnote"
                | "part"
                | "chapter"
                | "section"
                | "subsection"
                | "subsubsection"
                | "paragraph"
                | "subparagraph"
        );
        if name == "begin" || name == "end" {
            if chars.get(pos) != Some(&'{') {
                continue;
            }
            let end = group_end(&chars, pos);
            let environment: String = chars[pos + 1..end.saturating_sub(1).max(pos + 1)]
                .iter()
                .collect();
            mask[pos..end].fill(true);
            pos = end;
            let code = CODE_ENVIRONMENTS.contains(&environment.as_str());
            braces = if code { usize::MAX } else { 0 };
            optional = code;
        }
        while let Some(open) = chars.get(pos).filter(|c| matches!(c, '{' | '[')) {
            let end = group_end(&chars, pos);
            if *open == '[' {
                if !optional {
                    break;
                }
            } else if braces == 0 {
                break;
            } else {
                braces -= 1;
            }
            mask[pos..end].fill(true);
            pos = end;
        }
    }
    // Acentos escritos como órdenes: la palabra entera queda fuera.
    for i in 0..chars.len().saturating_sub(1) {
        if chars[i] == '\\' && matches!(chars[i + 1], '\'' | '"' | '~' | '^' | '`' | 'c' | 'v') {
            let space = |c: &char| c.is_whitespace();
            let start = chars[..i].iter().rposition(space).map_or(0, |p| p + 1);
            let end = chars[i..]
                .iter()
                .position(space)
                .map_or(chars.len(), |p| i + p);
            if chars[i + 1].is_alphabetic() && chars.get(i + 2).is_some_and(|c| c.is_alphabetic()) {
                continue;
            }
            mask[start..end].fill(true);
        }
    }
    mask
}

/// Columnas de una línea Markdown o de texto que no son prosa: código y enlaces.
pub fn plain_mask(line: &str, markdown: bool) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    let mut mask = vec![false; chars.len()];
    let mut i = 0;
    while i < chars.len() {
        let rest = &chars[i..];
        let starts = |text: &str| rest.iter().take(text.len()).copied().eq(text.chars());
        let end = if starts("http://") || starts("https://") || starts("www.") {
            rest.iter()
                .position(|c| c.is_whitespace() || matches!(c, ')' | '>' | ']'))
        } else if markdown && chars[i] == '`' {
            rest[1..].iter().position(|c| *c == '`').map(|p| p + 2)
        } else if markdown && starts("](") {
            rest.iter().position(|c| *c == ')').map(|p| p + 1)
        } else if markdown && chars[i] == '<' {
            rest.iter().position(|c| *c == '>').map(|p| p + 1)
        } else {
            i += 1;
            continue;
        };
        let end = end.map_or(chars.len(), |p| i + p);
        mask[i..end].fill(true);
        i = end.max(i + 1);
    }
    mask
}

/// Palabras que el diccionario no decide bien: siglas, medidas, claves.
fn skipped(word: &[char]) -> bool {
    word.len() < 3
        || word
            .iter()
            .any(|c| c.is_numeric() || *c == '_' || *c == '@')
        || word.iter().all(|c| !c.is_lowercase())
}

#[derive(Clone, Copy, PartialEq, Hash)]
enum Kind {
    Latex,
    Markdown,
    Text,
}

/// Filas que no se revisan en el documento que se está dibujando.
enum Rows {
    /// Preámbulo LaTeX: todo hasta `\begin{document}`.
    Before(usize),
    /// Bloques de código de Markdown.
    Fenced(Vec<bool>),
}

struct Menu {
    row: usize,
    start: usize,
    end: usize,
    word: String,
    guesses: Vec<String>,
}

pub struct Speller {
    language: String,
    /// Columnas de carácter mal escritas, por huella de la línea.
    cache: HashMap<u64, Rc<[(usize, usize)]>>,
    ignored: HashSet<String>,
    /// Cambia al aprender o ignorar una palabra.
    epoch: u64,
    document: Option<(egui::Id, u64, Kind, Rows)>,
    menu: Option<Menu>,
    /// Última edición del documento que se dibuja.
    edited: Instant,
    /// Tiempo de diccionario gastado en este cuadro.
    spent: Duration,
    /// Para pedir otro cuadro cuando quedan filas sin revisar.
    ctx: Option<egui::Context>,
}

impl Default for Speller {
    fn default() -> Self {
        Self {
            language: String::new(),
            cache: HashMap::new(),
            ignored: HashSet::new(),
            epoch: 0,
            document: None,
            menu: None,
            edited: Instant::now() - PAUSE,
            spent: Duration::ZERO,
            ctx: None,
        }
    }
}

impl Speller {
    /// Prepara la revisión del documento que se va a dibujar. Devuelve si se revisa.
    pub fn begin(
        &mut self,
        ctx: &egui::Context,
        editor: &Editor,
        id: egui::Id,
        config: &Config,
    ) -> bool {
        if !config.spellcheck || languages().is_empty() {
            return false;
        }
        self.spent = Duration::ZERO;
        if self.ctx.is_none() {
            self.ctx = Some(ctx.clone());
        }
        let extension = editor
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().to_lowercase());
        let kind = match editor.format {
            Format::Latex if matches!(extension.as_deref(), None | Some("tex" | "ltx")) => {
                Kind::Latex
            }
            Format::Markdown => Kind::Markdown,
            Format::Text => Kind::Text,
            _ => return false,
        };
        if self.language != config.spell_language {
            self.language = config.spell_language.clone();
            self.cache.clear();
        }
        if self
            .document
            .as_ref()
            .is_none_or(|(document, revision, ..)| *document != id || *revision != editor.revision)
        {
            let rows = match kind {
                Kind::Latex => Rows::Before(
                    editor
                        .lines
                        .iter()
                        .position(|line| line.trim_start().starts_with("\\begin{document}"))
                        .map_or(0, |row| row + 1),
                ),
                Kind::Markdown => {
                    let mut fenced = false;
                    Rows::Fenced(
                        editor
                            .lines
                            .iter()
                            .map(|line| {
                                let fence = line.trim_start().starts_with("```");
                                let skip = fenced || fence;
                                fenced ^= fence;
                                skip
                            })
                            .collect(),
                    )
                }
                Kind::Text => Rows::Before(0),
            };
            if self
                .document
                .as_ref()
                .is_some_and(|(document, ..)| *document == id)
            {
                self.edited = Instant::now();
            }
            self.document = Some((id, editor.revision, kind, rows));
        }
        true
    }
    /// Palabras mal escritas de una fila, en columnas de carácter.
    fn line(&mut self, editor: &Editor, row: usize) -> Rc<[(usize, usize)]> {
        let none = || Rc::from(Vec::new());
        let Some((_, _, kind, rows)) = &self.document else {
            return none();
        };
        let Some(text) = editor.lines.get(row) else {
            return none();
        };
        let outside = match rows {
            Rows::Before(start) => row < *start,
            Rows::Fenced(fenced) => fenced.get(row).copied().unwrap_or(false),
        };
        if outside || text.len() > 4000 || text.trim().is_empty() {
            return none();
        }
        let kind = *kind;
        let state = if kind == Kind::Latex {
            editor.syntax.latex_before(row)
        } else {
            highlight::State::default()
        };
        let mut hasher = DefaultHasher::new();
        (text, kind, self.epoch).hash(&mut hasher);
        if state != highlight::State::default() {
            format!("{state:?}").hash(&mut hasher);
        }
        let key = hasher.finish();
        if let Some(found) = self.cache.get(&key) {
            return found.clone();
        }
        // Consultar el diccionario cuesta en torno a un milisegundo por línea:
        // no se hace mientras se escribe ni más de unas pocas filas por cuadro.
        let wait = PAUSE.saturating_sub(self.edited.elapsed());
        if !wait.is_zero() || self.spent >= BUDGET {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint_after(wait);
            }
            return none();
        }
        let started = Instant::now();
        if self.cache.len() > 30_000 {
            self.cache.clear();
        }
        let mask = match kind {
            Kind::Latex => latex_mask(text, &state),
            Kind::Markdown => plain_mask(text, true),
            Kind::Text => plain_mask(text, false),
        };
        let found: Rc<[(usize, usize)]> = self.check(text, &mask).into();
        self.cache.insert(key, found.clone());
        self.spent += started.elapsed();
        found
    }
    fn check(&self, text: &str, mask: &[bool]) -> Vec<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        // Lo que no es prosa se sustituye por espacios del mismo ancho UTF-16.
        let mut masked = String::with_capacity(text.len());
        let mut units = Vec::with_capacity(chars.len() + 1);
        let mut unit = 0;
        for (c, hidden) in chars.iter().zip(mask) {
            units.push(unit);
            unit += c.len_utf16();
            if *hidden {
                masked.extend(std::iter::repeat_n(' ', c.len_utf16()));
            } else {
                masked.push(*c);
            }
        }
        units.push(unit);
        if masked.trim().is_empty() {
            return Vec::new();
        }
        system::check(&masked, &self.language)
            .into_iter()
            .filter_map(|(start, end)| {
                let start = units.binary_search(&start).ok()?;
                let end = units.binary_search(&end).ok()?;
                let word = &chars[start..end];
                let lower: String = word.iter().flat_map(|c| c.to_lowercase()).collect();
                (!skipped(word) && !self.ignored.contains(&lower)).then_some((start, end))
            })
            .collect()
    }
    /// Subraya las palabras mal escritas de una fila visual. `column` es la
    /// columna de la línea con que empieza la fila y `left`, `bottom` su posición.
    #[allow(clippy::too_many_arguments)]
    pub fn underline(
        &mut self,
        editor: &Editor,
        line: usize,
        column: usize,
        glyphs: &[Glyph],
        left: f32,
        bottom: f32,
        color: Color32,
        shapes: &mut Vec<egui::Shape>,
    ) {
        let found = self.line(editor, line);
        for &(start, end) in found.iter() {
            // La palabra que se está escribiendo aún no está terminada.
            let typing = editor.cursor.row == line && (start..=end).contains(&editor.cursor.col);
            let (from, to) = (start.max(column), end.min(column + glyphs.len()));
            if typing || from >= to {
                continue;
            }
            let (first, last) = (&glyphs[from - column], &glyphs[to - 1 - column]);
            let (x0, x1) = (left + first.pos.x, left + last.pos.x + last.advance_width);
            shapes.push(crate::marks::squiggle(x0, x1, bottom, color));
        }
    }
    /// Prepara el menú de la palabra mal escrita bajo el carácter `index`, si la hay.
    pub fn target(&mut self, editor: &Editor, index: usize) -> bool {
        let pos = editor.position(index);
        let found = self.line(editor, pos.row);
        self.menu = found
            .iter()
            .find(|(start, end)| (*start..=*end).contains(&pos.col))
            .map(|&(start, end)| {
                let word: String = editor.lines[pos.row]
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect();
                let mut guesses = system::guesses(&word, &self.language);
                guesses.truncate(8);
                Menu {
                    row: pos.row,
                    start,
                    end,
                    word,
                    guesses,
                }
            });
        self.menu.is_some()
    }
    pub fn has_menu(&self) -> bool {
        self.menu.is_some()
    }
    /// Menú de sugerencias. Devuelve si cambió el texto.
    pub fn menu_ui(&mut self, ui: &mut egui::Ui, editor: &mut Editor) -> bool {
        let Some(menu) = &self.menu else {
            return false;
        };
        let current: String = editor
            .lines
            .get(menu.row)
            .map(|line| {
                line.chars()
                    .skip(menu.start)
                    .take(menu.end - menu.start)
                    .collect()
            })
            .unwrap_or_default();
        if current != menu.word {
            self.menu = None;
            ui.close();
            return false;
        }
        let mut replacement = None;
        if menu.guesses.is_empty() {
            ui.label("Sin sugerencias");
        }
        for guess in &menu.guesses {
            if ui.button(guess).clicked() {
                replacement = Some(guess.clone());
            }
        }
        ui.separator();
        let learn = ui
            .button("Añadir al diccionario")
            .on_hover_text("La palabra se guarda en el diccionario del sistema")
            .clicked();
        let ignore = ui.button("Ignorar en esta sesión").clicked();
        if learn {
            system::learn(&menu.word);
        }
        if ignore {
            self.ignored.insert(menu.word.to_lowercase());
        }
        let mut changed = false;
        if let Some(text) = replacement {
            editor.replace(
                Pos::new(menu.row, menu.start),
                Pos::new(menu.row, menu.end),
                &text,
            );
            changed = true;
        }
        if learn || ignore || changed {
            self.epoch += 1;
            self.cache.clear();
            self.menu = None;
            ui.close();
        }
        changed
    }
}

/// Opciones del corrector en Preferencias. Devuelve si hay que guardar.
pub fn preferences(ui: &mut egui::Ui, config: &mut Config) -> bool {
    if languages().is_empty() {
        ui.add_enabled(
            false,
            egui::Checkbox::new(&mut false, "Corrector ortográfico (solo en macOS)"),
        );
        return false;
    }
    let mut changed = ui
        .checkbox(&mut config.spellcheck, "Corrector ortográfico")
        .on_hover_text("Subraya la prosa de LaTeX, Markdown y texto. Clic derecho para corregir.")
        .changed();
    ui.add_enabled_ui(config.spellcheck, |ui| {
        ui.horizontal(|ui| {
            ui.label("Idioma");
            egui::ComboBox::from_id_salt("spell_language")
                .selected_text(&config.spell_language)
                .show_ui(ui, |ui| {
                    for language in languages() {
                        changed |= ui
                            .selectable_value(
                                &mut config.spell_language,
                                language.clone(),
                                language,
                            )
                            .changed();
                    }
                });
        });
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Texto que llega al diccionario: lo demás va como espacios.
    fn prose(line: &str, mask: &[bool]) -> String {
        let text: String = line
            .chars()
            .zip(mask)
            .map(|(c, hidden)| if *hidden { ' ' } else { c })
            .collect();
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }
    fn latex(line: &str) -> String {
        prose(line, &latex_mask(line, &highlight::State::default()))
    }

    #[test]
    fn only_prose_reaches_the_dictionary() {
        assert_eq!(
            latex("La \\textbf{medición} de \\cite{perez2020} en \\ref{fig:uno} % nota"),
            "La medición de en"
        );
        assert_eq!(
            latex("\\section{Introducción general}"),
            "Introducción general"
        );
        assert_eq!(
            latex("Sea $x_i^2 + \\alpha$ el valor y \\(y\\) otro."),
            "Sea el valor y otro."
        );
        assert_eq!(
            latex("\\begin{tabular}{lcc} Nombre & Valor \\\\"),
            "Nombre Valor"
        );
        assert_eq!(latex("\\begin{figure}[htbp]"), "");
        assert_eq!(
            latex("\\begin{theorem}[Teorema de Pitágoras]"),
            "Teorema de Pitágoras"
        );
        assert_eq!(
            latex("\\includegraphics[width=0.8\\linewidth]{imagenes/logo}"),
            ""
        );
        assert_eq!(
            latex("\\vspace{2cm} texto \\textcolor{red}{rojo}"),
            "texto rojo"
        );
        assert_eq!(latex("\\item[Primero] elemento"), "Primero elemento");
        assert_eq!(
            latex("\\caption{Gráfica de resultados}\\label{fig:res}"),
            "Gráfica de resultados"
        );
        assert_eq!(latex("canci\\'on bonita"), "bonita");
        assert_eq!(latex("\\usepackage[spanish]{babel}"), "");
        let math = highlight::State {
            math: Some(highlight::Math::Env("equation".into())),
            verbatim: None,
        };
        assert_eq!(prose("a = b + c", &latex_mask("a = b + c", &math)), "");

        let markdown = |line: &str| prose(line, &plain_mask(line, true));
        assert_eq!(
            markdown("Usa `cargo run` y lee [la guía](docs/guia.md) en https://ejemplo.org/x ya."),
            "Usa y lee [la guía en ya."
        );
        assert_eq!(markdown("Etiqueta <br> final"), "Etiqueta final");
        assert_eq!(
            prose("un `código`", &plain_mask("un `código`", false)),
            "un `código`"
        );

        let chars = |word: &str| word.chars().collect::<Vec<_>>();
        assert!(skipped(&chars("cm")));
        assert!(skipped(&chars("UNAM")));
        assert!(skipped(&chars("x2y")));
        assert!(!skipped(&chars("México")));
    }
}
