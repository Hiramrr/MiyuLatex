//! Marcas que se pintan bajo el texto del editor: coincidencias de la
//! búsqueda, apariciones de lo seleccionado, corchetes emparejados y guías
//! de sangría.

use eframe::egui::{self, Color32, Rect, Shape, Stroke, epaint::text::Glyph};

use crate::{
    editor::{Editor, Pos, byte_col, code},
    format::Format,
    theme::{Theme, col},
};

pub struct Marks {
    /// Pintar las coincidencias de la búsqueda.
    search: bool,
    /// El widget solo pinta la selección con el foco: sin él se pinta aquí.
    selection: Option<(Pos, Pos)>,
    /// Texto seleccionado cuyas otras apariciones se señalan.
    word: Option<String>,
    pair: Option<(Pos, Pos)>,
    /// Columnas por nivel de sangría y ancho de una columna.
    guides: Option<(usize, f32)>,
    found: Color32,
    current: Color32,
    occurrence: Color32,
    bracket: Color32,
    guide: Color32,
}

impl Marks {
    /// `column_width` es el ancho de un carácter; `guides` y `search` dicen qué pintar.
    pub fn new(
        editor: &Editor,
        theme: &Theme,
        focused: bool,
        search: bool,
        guides: bool,
        column_width: f32,
    ) -> Self {
        let (a, b) = editor.selection();
        let word = (focused && a != b && a.row == b.row && b.col - a.col <= 100)
            .then(|| editor.selected())
            .filter(|text| text.trim().chars().count() >= 2);
        Self {
            search: search && !editor.matches.is_empty(),
            selection: (!focused && a != b).then_some((a, b)),
            word,
            pair: (focused && a == b)
                .then(|| editor.matching_bracket())
                .flatten(),
            guides: (guides && matches!(editor.format, Format::Code(_)))
                .then(|| (editor.unit_columns(), column_width)),
            found: col(theme.warning).gamma_multiply(0.28),
            current: col(theme.selection()),
            occurrence: col(theme.fg).gamma_multiply(0.13),
            bracket: col(theme.accent),
            guide: col(theme.border).gamma_multiply(0.7),
        }
    }

    /// Marcas de una fila visual: la de la línea `line` que empieza en su
    /// columna `column` y ocupa `rect`.
    #[allow(clippy::too_many_arguments)]
    pub fn row(
        &self,
        editor: &Editor,
        line: usize,
        column: usize,
        glyphs: &[Glyph],
        rect: Rect,
        starts_line: bool,
        shapes: &mut Vec<Shape>,
    ) {
        let Some(text) = editor.lines.get(line) else {
            return;
        };
        if starts_line && let Some((unit, width)) = self.guides {
            let depth = if text.trim().is_empty() {
                // Una línea en blanco sigue las guías del código que viene después.
                editor.lines[line..]
                    .iter()
                    .take(100)
                    .find(|next| !next.trim().is_empty())
                    .map_or(0, |next| code::indent_columns(next))
            } else {
                code::indent_columns(text)
            };
            for level in (0..depth).step_by(unit.max(1)) {
                let x = (rect.left() + level as f32 * width).round() + 0.5;
                shapes.push(Shape::line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    Stroke::new(1.0, self.guide),
                ));
            }
        }
        if glyphs.is_empty() {
            return;
        }
        // Tramo de la fila entre dos columnas de la línea, si cae en ella.
        let span = |start: usize, end: usize| {
            let (from, to) = (start.max(column), end.min(column + glyphs.len()));
            (from < to).then(|| {
                let (first, last) = (&glyphs[from - column], &glyphs[to - 1 - column]);
                Rect::from_x_y_ranges(
                    rect.left() + first.pos.x..=rect.left() + last.pos.x + last.advance_width,
                    rect.y_range(),
                )
            })
        };
        if let Some((a, b)) = self.selection
            && (a.row..=b.row).contains(&line)
        {
            let start = if line == a.row { a.col } else { 0 };
            let end = if line == b.row { b.col } else { usize::MAX };
            if let Some(area) = span(start, end) {
                shapes.push(Shape::rect_filled(area, 2.0, self.current));
            }
        }
        if self.search {
            let first = editor.matches.partition_point(|(a, _)| a.row < line);
            for (a, b) in editor.matches[first..]
                .iter()
                .take_while(|(a, _)| a.row == line)
            {
                if let Some(area) = span(a.col, b.col) {
                    shapes.push(Shape::rect_filled(area, 2.0, self.found));
                }
            }
        }
        if let Some(word) = &self.word {
            let (selected, _) = editor.selection();
            for (at, _) in text.match_indices(word.as_str()) {
                let start = text[..at].chars().count();
                if (line, start) == (selected.row, selected.col) {
                    continue;
                }
                if let Some(area) = span(start, start + word.chars().count()) {
                    shapes.push(Shape::rect_filled(area, 2.0, self.occurrence));
                }
            }
        }
        if let Some((a, b)) = self.pair {
            for bracket in [a, b] {
                if bracket.row == line
                    && let Some(area) = span(bracket.col, bracket.col + 1)
                {
                    shapes.push(Shape::rect_filled(
                        area,
                        2.0,
                        self.bracket.gamma_multiply(0.22),
                    ));
                    shapes.push(Shape::line_segment(
                        [area.left_bottom(), area.right_bottom()],
                        Stroke::new(1.5, self.bracket),
                    ));
                }
            }
        }
    }
}

/// Subrayado ondulado entre `x0` y `x1`, sobre el borde inferior de una fila.
pub fn squiggle(x0: f32, x1: f32, bottom: f32, color: Color32) -> Shape {
    let y = bottom - 1.5;
    let steps = ((x1 - x0) / 2.5).ceil().max(1.0) as usize;
    let points = (0..=steps)
        .map(|i| {
            egui::pos2(
                (x0 + i as f32 * 2.5).min(x1),
                y + if i % 2 == 0 { 1.0 } else { -1.0 },
            )
        })
        .collect();
    Shape::line(points, Stroke::new(1.0, color))
}

/// Posición entre las coincidencias de la que está seleccionada, desde 1.
pub fn current_match(editor: &Editor) -> Option<usize> {
    let selection = editor.selection();
    editor
        .matches
        .iter()
        .position(|found| *found == selection)
        .map(|index| index + 1)
}

/// Resumen de la selección para la barra de estado.
pub fn selection_summary(editor: &Editor) -> Option<String> {
    let (a, b) = editor.selection();
    if a == b {
        return None;
    }
    let chars = if a.row == b.row {
        b.col - a.col
    } else {
        let line = &editor.lines[a.row];
        let first = line[byte_col(line, a.col)..].chars().count() + 1;
        let middle: usize = editor.lines[a.row + 1..b.row]
            .iter()
            .map(|line| line.chars().count() + 1)
            .sum();
        first + middle + b.col
    };
    Some(if a.row == b.row {
        format!("{chars} seleccionados")
    } else {
        format!("{} líneas, {chars} seleccionados", b.row - a.row + 1)
    })
}
