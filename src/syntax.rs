//! Resaltado incremental. Cada línea guarda sus tramos de estilo y el estado
//! con que termina; tras una edición solo se rehacen las líneas afectadas,
//! hasta que el estado vuelve a coincidir con el que ya había.

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use eframe::egui;
use syntect::{
    highlighting::{FontStyle, HighlightIterator, HighlightState, Highlighter},
    parsing::{ParseState, ScopeStack, SyntaxReference},
};

use crate::{
    format::{self, Format},
    highlight::{self, Tok},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ink {
    /// Color de texto del tema.
    #[default]
    Plain,
    /// Color que el tema asigna a un token LaTeX.
    Tok(Tok),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub ink: Ink,
    pub italic: bool,
    pub underline: bool,
}

/// Tramo de estilo que termina en el byte `end` de su línea.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Run {
    pub end: u32,
    pub style: Style,
}

#[derive(Clone, PartialEq)]
enum State {
    Latex(highlight::State),
    Code(Box<(ParseState, HighlightState)>),
}

pub struct Line {
    /// Vacío si toda la línea va con el estilo normal.
    pub runs: Vec<Run>,
    /// Huella del texto y sus tramos: dos líneas con la misma se ven igual.
    pub key: u64,
    /// Estado al terminar la línea; `None` si aún no se ha resaltado.
    after: Option<State>,
}

impl Line {
    fn fresh(text: &str) -> Self {
        Self {
            runs: Vec::new(),
            key: key(text, &[]),
            after: None,
        }
    }
}

fn key(text: &str, runs: &[Run]) -> u64 {
    egui::util::hash((text, runs))
}

enum Kind {
    Plain,
    Latex,
    Code(&'static SyntaxReference),
}

pub struct Syntax {
    kind: Kind,
    dark: bool,
    pub lines: Vec<Line>,
    /// Filas por rehacer. Todo lo anterior a la primera es válido.
    stale: BTreeSet<usize>,
    /// Cambia cada vez que cambia algún tramo.
    pub version: u64,
}

impl Syntax {
    pub fn new(format: &Format, lines: &[String]) -> Self {
        let syntaxes = &format::syntax_settings().ps;
        let kind = match format {
            Format::Latex => Kind::Latex,
            Format::Markdown => syntaxes
                .find_syntax_by_name("Markdown")
                .map_or(Kind::Plain, Kind::Code),
            Format::Code(name) => syntaxes
                .find_syntax_by_name(name)
                .map_or(Kind::Plain, Kind::Code),
            _ => Kind::Plain,
        };
        let mut syntax = Self {
            kind,
            dark: true,
            lines: Vec::new(),
            stale: BTreeSet::new(),
            version: 0,
        };
        syntax.edit(0, 0, lines);
        syntax
    }

    /// Los colores de syntect dependen de si el tema es claro u oscuro.
    pub fn set_dark(&mut self, dark: bool) {
        if self.dark != dark {
            self.dark = dark;
            if matches!(self.kind, Kind::Code(_)) {
                for line in &mut self.lines {
                    line.after = None;
                }
                self.stale = BTreeSet::from([0]);
            }
        }
    }

    /// Sustituye `removed` filas a partir de `row` por las de `inserted`.
    pub fn edit(&mut self, row: usize, removed: usize, inserted: &[String]) {
        self.lines.splice(
            row..row + removed,
            inserted.iter().map(|line| Line::fresh(line)),
        );
        self.stale = self
            .stale
            .iter()
            .map(|&stale| {
                if stale < row {
                    stale
                } else if stale >= row + removed {
                    stale - removed + inserted.len()
                } else {
                    row
                }
            })
            .collect();
        // Si solo se borraron filas, la siguiente hereda otro estado.
        self.stale.insert(row);
        self.version += 1;
    }

    /// Rehace las filas pendientes. Con `budget` se detiene al agotarlo y
    /// devuelve `true` si quedó trabajo para la próxima llamada.
    pub fn advance(&mut self, lines: &[String], budget: Option<Duration>) -> bool {
        if matches!(self.kind, Kind::Plain) {
            self.stale.clear();
        }
        if self.stale.is_empty() {
            return false;
        }
        let started = Instant::now();
        let theme = &format::syntax_settings().ts.themes[if self.dark {
            "base16-mocha.dark"
        } else {
            "Solarized (light)"
        }];
        let highlighter = Highlighter::new(theme);
        let mut buffer = String::new();
        let mut changed = false;
        'pending: while let Some(mut row) = self.stale.pop_first() {
            if row >= lines.len() {
                continue;
            }
            let mut state = row
                .checked_sub(1)
                .and_then(|previous| self.lines[previous].after.clone())
                .unwrap_or_else(|| match self.kind {
                    Kind::Code(syntax) => State::Code(Box::new((
                        ParseState::new(syntax),
                        HighlightState::new(&highlighter, ScopeStack::new()),
                    ))),
                    _ => State::Latex(Default::default()),
                });
            loop {
                let runs = runs(&lines[row], &mut state, &highlighter, &mut buffer);
                let line = &mut self.lines[row];
                if line.runs != runs {
                    line.key = key(&lines[row], &runs);
                    line.runs = runs;
                    changed = true;
                }
                // Mismo estado que antes: lo que sigue ya era correcto.
                if line.after.as_ref() == Some(&state) {
                    break;
                }
                line.after = Some(state.clone());
                row += 1;
                if row == lines.len() {
                    break;
                }
                self.stale.remove(&row);
                if budget.is_some_and(|budget| started.elapsed() > budget) {
                    self.stale.insert(row);
                    break 'pending;
                }
            }
        }
        if changed {
            self.version += 1;
        }
        !self.stale.is_empty()
    }

    /// Estado LaTeX con que empieza una fila.
    pub fn latex_before(&self, row: usize) -> highlight::State {
        match row
            .checked_sub(1)
            .map(|previous| &self.lines[previous].after)
        {
            Some(Some(State::Latex(state))) => state.clone(),
            _ => Default::default(),
        }
    }

    /// Tokens LaTeX de las filas que contienen un comando de sección.
    pub fn sections<'a>(
        &'a self,
        lines: &'a [String],
    ) -> impl Iterator<Item = (usize, Vec<highlight::Span>)> + 'a {
        let section = Ink::Tok(Tok::Section);
        self.lines
            .iter()
            .enumerate()
            .filter(move |(_, line)| line.runs.iter().any(|run| run.style.ink == section))
            .map(|(row, _)| {
                let before = self.latex_before(row);
                (row, highlight::tokenize_line(&lines[row], &before).0)
            })
    }
}

/// Tramos de una línea; deja en `state` el estado con que termina.
fn runs(text: &str, state: &mut State, highlighter: &Highlighter, buffer: &mut String) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut push = |end: usize, style: Style| match runs.last_mut() {
        Some(last) if last.style == style => last.end = end as u32,
        _ => runs.push(Run {
            end: end as u32,
            style,
        }),
    };
    match state {
        State::Latex(latex) => {
            let (spans, next) = highlight::tokenize_line(text, latex);
            *latex = next;
            if spans.is_empty() {
                return runs;
            }
            // Los tramos de fondo llegan primero y los demás se pintan encima.
            let mut styles = vec![Style::default(); text.chars().count()];
            for span in spans {
                let end = span.end.min(styles.len());
                for style in &mut styles[span.start.min(end)..end] {
                    match span.tok {
                        Tok::Bold => {}
                        Tok::Italic => style.italic = true,
                        Tok::Underline => style.underline = true,
                        tok => {
                            *style = Style {
                                ink: Ink::Tok(tok),
                                ..Default::default()
                            }
                        }
                    }
                }
            }
            for ((start, c), style) in text.char_indices().zip(styles) {
                push(start + c.len_utf8(), style);
            }
        }
        State::Code(code) => {
            let (parse, highlight) = &mut **code;
            // Las gramáticas de syntect esperan el salto de línea.
            buffer.clear();
            buffer.push_str(text);
            buffer.push('\n');
            let ops = parse
                .parse_line(buffer, &format::syntax_settings().ps)
                .unwrap_or_default();
            let mut end = 0;
            for (style, piece) in HighlightIterator::new(highlight, &ops, buffer, highlighter) {
                let start = end.min(text.len());
                end += piece.len();
                if end.min(text.len()) > start {
                    let color = style.foreground;
                    push(
                        end.min(text.len()),
                        Style {
                            ink: Ink::Rgb(color.r, color.g, color.b),
                            italic: style.font_style.contains(FontStyle::ITALIC),
                            underline: style.font_style.contains(FontStyle::UNDERLINE),
                        },
                    );
                }
            }
        }
    }
    if let [only] = runs[..]
        && only.style == Style::default()
    {
        runs.clear();
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.split('\n').map(str::to_string).collect()
    }

    fn snapshot(syntax: &Syntax) -> Vec<(Vec<Run>, u64)> {
        syntax
            .lines
            .iter()
            .map(|line| (line.runs.clone(), line.key))
            .collect()
    }

    /// Editar por partes da lo mismo que resaltar el resultado desde cero.
    #[test]
    fn incremental_matches_full_highlight() {
        for (format, before, row, removed, inserted) in [
            (
                Format::Latex,
                "\\section{Uno}\ntexto $x$\n\\begin{verbatim}\n\\section{no}\n\\end{verbatim}\n\\textbf{fin}",
                2,
                1,
                "texto normal\notra línea",
            ),
            (
                Format::Latex,
                "a\n\\begin{equation}\nx^2\n\\end{equation}\n\\section{Dos}",
                1,
                1,
                "",
            ),
            (
                Format::Code("Rust".into()),
                "fn main() {\n    let s = \"hola\";\n}\n// fin",
                1,
                1,
                "    /* abre\n    un comentario",
            ),
            (
                Format::Markdown,
                "# Título\n\n```rust\nfn f() {}\n```\n\n**fin**",
                2,
                1,
                "texto",
            ),
        ] {
            let mut text = lines(before);
            let mut syntax = Syntax::new(&format, &text);
            assert!(!syntax.advance(&text, None));
            let inserted = if inserted.is_empty() {
                Vec::new()
            } else {
                lines(inserted)
            };
            text.splice(row..row + removed, inserted.clone());
            syntax.edit(row, removed, &inserted);
            assert!(!syntax.advance(&text, None));
            let mut full = Syntax::new(&format, &text);
            full.advance(&text, None);
            assert_eq!(snapshot(&syntax), snapshot(&full), "{format:?}");
            assert!(syntax.lines.iter().any(|line| !line.runs.is_empty()));
        }
    }

    #[test]
    fn budget_resumes_where_it_stopped() {
        let text: Vec<String> = (0..400).map(|i| format!("let x{i} = \"{i}\";")).collect();
        let mut syntax = Syntax::new(&Format::Code("Rust".into()), &text);
        let mut calls = 0;
        while syntax.advance(&text, Some(Duration::ZERO)) {
            calls += 1;
        }
        assert!(calls > 1);
        let mut full = Syntax::new(&Format::Code("Rust".into()), &text);
        full.advance(&text, None);
        assert_eq!(snapshot(&syntax), snapshot(&full));
    }

    #[test]
    fn sections_skip_verbatim() {
        let text = lines("\\section{Uno}\n\\begin{verbatim}\n\\section{no}\n\\end{verbatim}");
        let mut syntax = Syntax::new(&Format::Latex, &text);
        syntax.advance(&text, None);
        let rows: Vec<_> = syntax.sections(&text).map(|(row, _)| row).collect();
        assert_eq!(rows, [0]);
    }
}
