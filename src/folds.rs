//! Plegado: secciones, entornos y bloques con sangría que se ocultan bajo
//! su primera línea.

use super::{Editor, Pos, code};
use crate::format::Format;

impl Editor {
    /// Sangría de una fila, o `None` si está en blanco.
    fn depth(&self, row: usize) -> Option<usize> {
        let line = &self.lines[row];
        (!line.trim().is_empty()).then(|| code::indent_columns(line))
    }
    /// Entorno que abre una fila de LaTeX, si no lo cierra ella misma.
    fn opened_environment(&self, row: usize) -> Option<&str> {
        let line = &self.lines[row];
        let name = line.split("\\begin{").nth(1)?.split('}').next()?;
        (name != "document" && !line.contains(&format!("\\end{{{name}}}"))).then_some(name)
    }
    /// Última fila que se oculta al plegar `row`, si hay algo que plegar.
    pub fn fold_end(&self, row: usize) -> Option<usize> {
        if row + 1 >= self.lines.len() {
            return None;
        }
        let last = self.lines.len() - 1;
        let prose = matches!(self.format, Format::Latex | Format::Markdown);
        let outline = self.outline();
        let mut end = None;
        if prose && let Ok(at) = outline.binary_search_by_key(&row, |entry| entry.0) {
            // Una sección llega hasta la siguiente de su nivel o de uno superior.
            let level = outline[at].1;
            let next = outline[at + 1..].iter().find(|entry| entry.1 <= level);
            let mut stop = next.map_or(last, |entry| entry.0 - 1);
            // El cierre del documento queda a la vista.
            while self.format == Format::Latex
                && stop > row
                && next.is_none()
                && (self.lines[stop].trim().is_empty()
                    || self.lines[stop].contains("\\end{document}"))
            {
                stop -= 1;
            }
            end = Some(stop);
        } else if self.format == Format::Latex
            && let Some(name) = self.opened_environment(row)
        {
            // Un entorno llega hasta la línea anterior a su cierre.
            let (open, close) = (format!("\\begin{{{name}}}"), format!("\\end{{{name}}}"));
            let mut nested = 0usize;
            for (at, line) in self.lines.iter().enumerate().skip(row + 1) {
                if line.contains(&close) {
                    if nested == 0 {
                        end = Some(at - 1);
                        break;
                    }
                    nested -= 1;
                } else if line.contains(&open) {
                    nested += 1;
                }
            }
        } else if !prose && let Some(depth) = self.depth(row) {
            // Un bloque de código llega hasta que la sangría vuelve a su nivel.
            let stop = (row + 1..=last)
                .find(|at| self.depth(*at).is_some_and(|d| d <= depth))
                .map_or(last, |at| at - 1);
            end = Some(stop);
        }
        // Las líneas en blanco del final no se pliegan.
        let mut end = end?;
        while end > row && self.lines[end].trim().is_empty() {
            end -= 1;
        }
        (end > row).then_some(end)
    }
    /// Barato de comprobar en cada fila visible; `fold_end` dice hasta dónde.
    pub fn foldable(&self, row: usize) -> bool {
        match self.format {
            Format::Latex | Format::Markdown => {
                self.outline().binary_search_by_key(&row, |e| e.0).is_ok()
                    || (self.format == Format::Latex && self.opened_environment(row).is_some())
            }
            _ => self.depth(row).is_some_and(|depth| {
                (row + 1..self.lines.len())
                    .find_map(|at| self.depth(at))
                    .is_some_and(|next| next > depth)
            }),
        }
    }
    pub fn folded(&self, row: usize) -> bool {
        self.folds.binary_search(&row).is_ok()
    }
    pub fn has_folds(&self) -> bool {
        !self.folds.is_empty()
    }
    /// Si una fila está oculta dentro de un bloque plegado.
    pub fn is_hidden(&self, row: usize) -> bool {
        let at = self.hidden.partition_point(|(_, end)| *end < row);
        self.hidden.get(at).is_some_and(|(start, _)| *start <= row)
    }
    /// Recalcula las filas ocultas; los pliegues que ya no tienen qué ocultar se quitan.
    pub(super) fn update_folds(&mut self) {
        let mut hidden: Vec<(usize, usize)> = Vec::new();
        let folds = std::mem::take(&mut self.folds);
        self.folds = folds
            .into_iter()
            .filter(|row| {
                let Some(end) = (*row < self.lines.len())
                    .then(|| self.fold_end(*row))
                    .flatten()
                else {
                    return false;
                };
                // Un pliegue dentro de otro ya está oculto por el de fuera.
                if hidden.last().is_none_or(|(_, last)| *last < row + 1) {
                    hidden.push((row + 1, end));
                }
                true
            })
            .collect();
        self.hidden = hidden;
        self.fold_version += 1;
    }
    /// Pliega o despliega el bloque que empieza en `row`.
    pub fn toggle_fold(&mut self, row: usize) -> bool {
        match self.folds.binary_search(&row) {
            Ok(at) => {
                self.folds.remove(at);
            }
            Err(at) if self.fold_end(row).is_some() => self.folds.insert(at, row),
            Err(_) => return false,
        }
        self.update_folds();
        true
    }
    /// Bloque plegable más interior que contiene `row`, para plegar desde el cursor.
    pub fn enclosing_fold(&self, row: usize) -> Option<usize> {
        (0..=row)
            .rev()
            .take(2000)
            .find(|start| self.fold_end(*start).is_some_and(|end| end >= row))
    }
    /// Despliega lo que oculte `row`.
    pub fn unfold_at(&mut self, row: usize) {
        let before = self.folds.len();
        let ends: Vec<Option<usize>> = self.folds.iter().map(|r| self.fold_end(*r)).collect();
        let mut ends = ends.into_iter();
        self.folds.retain(|start| {
            !ends
                .next()
                .flatten()
                .is_some_and(|end| *start < row && row <= end)
        });
        if self.folds.len() != before {
            self.update_folds();
        }
    }
    pub fn unfold_all(&mut self) {
        self.folds.clear();
        self.update_folds();
    }
    /// Pliega todas las secciones del nivel más alto, o los bloques de primer nivel.
    pub fn fold_all(&mut self) {
        let top = self.outline().iter().map(|e| e.1).min();
        self.folds = (0..self.lines.len())
            .filter(|row| match self.format {
                Format::Latex | Format::Markdown => self
                    .outline()
                    .binary_search_by_key(row, |e| e.0)
                    .is_ok_and(|at| Some(self.outline()[at].1) == top),
                _ => self.depth(*row) == Some(0),
            })
            .filter(|row| self.fold_end(*row).is_some())
            .collect();
        self.update_folds();
    }
    /// Saca el cursor de una fila oculta: sigue en el sentido en que venía
    /// desde `from`. Devuelve si lo movió.
    pub fn skip_hidden(&mut self, from: Pos) -> bool {
        let row = self.cursor.row;
        let at = self.hidden.partition_point(|(_, end)| *end < row);
        let Some(&(start, end)) = self.hidden.get(at).filter(|(start, _)| *start <= row) else {
            return false;
        };
        let target = if from.row > end || (from.row >= start && end + 1 >= self.lines.len()) {
            // Hacia arriba se queda al final de la línea que encabeza el pliegue.
            Pos::new(start - 1, self.lines[start - 1].chars().count())
        } else if end + 1 < self.lines.len() {
            Pos::new(
                end + 1,
                self.cursor.col.min(self.lines[end + 1].chars().count()),
            )
        } else {
            Pos::new(start - 1, self.lines[start - 1].chars().count())
        };
        // Al alargar una selección el ancla se conserva.
        let anchor = self.anchor;
        self.place(target, anchor);
        true
    }
    /// Ajusta los pliegues cuando, desde la fila `first`, se quitan `removed`
    /// filas y se ponen `added`. Con `pushed`, el texto de la primera fila
    /// queda en la última de las nuevas: se insertó delante de él.
    pub(super) fn shift_folds(&mut self, first: usize, removed: usize, added: usize, pushed: bool) {
        if self.folds.is_empty() || removed == added {
            return;
        }
        self.folds = std::mem::take(&mut self.folds)
            .into_iter()
            .filter_map(|row| {
                if row < first {
                    Some(row)
                } else if row >= first + removed {
                    Some(row + added - removed)
                } else if row == first && pushed && removed == 1 {
                    Some(first + added - 1)
                } else {
                    // La línea que lo encabezaba se reescribió.
                    (row == first).then_some(row)
                }
            })
            .collect();
        self.folds.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_sections_environments_and_code_blocks() {
        let tex = "\\section{Uno}\nTexto\n\\begin{itemize}\n  \\item a\n  \\begin{itemize}\n    \\item b\n  \\end{itemize}\n\\end{itemize}\n\n\\subsection{Sub}\nMás\n\n\\section{Dos}\nFin\n\n\\end{document}\n";
        let mut e = Editor::new(tex.into(), Some("a.tex".into()));
        assert_eq!(e.fold_end(0), Some(10));
        assert_eq!(e.fold_end(2), Some(6));
        assert_eq!(e.fold_end(4), Some(5));
        assert_eq!(e.fold_end(9), Some(10));
        // La última sección no se lleva el cierre del documento.
        assert_eq!(e.fold_end(12), Some(13));
        assert_eq!(e.fold_end(1), None);
        assert!(e.foldable(0) && e.foldable(2) && !e.foldable(1) && !e.foldable(3));
        assert_eq!(e.enclosing_fold(5), Some(4));
        assert!(e.toggle_fold(2));
        assert!(e.toggle_fold(0));
        assert!(!e.toggle_fold(1));
        assert!(e.folded(0) && !e.is_hidden(0) && e.is_hidden(1) && e.is_hidden(10));
        assert!(!e.is_hidden(11) && !e.is_hidden(12));
        // El cursor salta el pliegue en el sentido en que venía.
        e.goto(1, 0);
        assert!(e.skip_hidden(Pos::new(0, 3)));
        assert_eq!(e.cursor, Pos::new(11, 0));
        e.goto(10, 2);
        assert!(e.skip_hidden(Pos::new(11, 0)));
        assert_eq!(e.cursor, Pos::new(0, 13));
        assert!(!e.skip_hidden(Pos::new(0, 0)));
        // Al escribir antes, los pliegues siguen a sus líneas.
        e.goto(0, 0);
        e.insert("% nota\n\n");
        assert!(e.folded(2) && e.folded(4) && !e.folded(0));
        assert!(e.is_hidden(3) && !e.is_hidden(13));
        e.undo(false);
        assert!(e.folded(0) && e.folded(2));
        // Desplegar lo que oculta una fila abre todos los pliegues que la contienen.
        e.unfold_at(3);
        assert!(!e.has_folds());
        e.fold_all();
        assert!(e.folded(0) && e.folded(12) && !e.folded(9));
        e.unfold_all();
        assert!(!e.is_hidden(1));

        let code = "fn main() {\n    if x {\n        y();\n    }\n\n}\n\nfn otro() {}\n";
        let mut e = Editor::new(code.into(), Some("a.rs".into()));
        assert_eq!(e.fold_end(0), Some(3));
        assert_eq!(e.fold_end(1), Some(2));
        assert_eq!(e.fold_end(7), None);
        assert!(e.foldable(0) && e.foldable(1) && !e.foldable(2) && !e.foldable(7));
        e.fold_all();
        assert!(e.folded(0) && e.is_hidden(3) && !e.is_hidden(4) && !e.is_hidden(5));
    }
}
