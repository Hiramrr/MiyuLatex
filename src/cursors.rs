//! Cursores múltiples: además del cursor del widget de texto, el editor
//! guarda otros que reciben las mismas ediciones y movimientos.

use super::{Editor, Pos};

/// Edición que se aplica a la vez en todos los cursores.
pub enum Edit<'a> {
    Insert(&'a str),
    Backspace,
    Delete,
    Newline,
    Indent,
}

/// Movimiento que hacen a la vez todos los cursores.
#[derive(Clone, Copy)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl Editor {
    /// Cursores adicionales, cada uno con su ancla y su cursor. Dejan de
    /// valer cuando el texto cambia por otra vía.
    pub fn extras(&self) -> &[(Pos, Pos)] {
        if self.extras.0 == self.revision {
            &self.extras.1
        } else {
            &[]
        }
    }
    pub fn clear_extras(&mut self) {
        self.extras.1.clear();
    }
    /// Todos los cursores, el principal primero.
    fn cursors(&self) -> Vec<(Pos, Pos)> {
        let mut all = vec![(self.anchor.unwrap_or(self.cursor), self.cursor)];
        all.extend_from_slice(self.extras());
        all
    }
    /// Reparte `all`: el primero es el principal y los demás, sin repetir, adicionales.
    fn set_cursors(&mut self, all: Vec<(Pos, Pos)>) {
        let span = |c: &(Pos, Pos)| (c.0.min(c.1), c.0.max(c.1));
        let mut extras: Vec<(Pos, Pos)> = Vec::new();
        for cursor in &all[1..] {
            if span(cursor) != span(&all[0]) && !extras.iter().any(|e| span(e) == span(cursor)) {
                extras.push(*cursor);
            }
        }
        let (anchor, cursor) = all[0];
        self.place(cursor, Some(anchor));
        self.extras = (self.revision, extras);
    }
    /// Añade como adicional una selección, por ejemplo la que había antes de un clic.
    pub fn add_cursor(&mut self, anchor: Pos, cursor: Pos) {
        let mut all = self.cursors();
        all.push((anchor, cursor));
        self.set_cursors(all);
    }
    /// Añade un cursor en la línea de arriba o de abajo y lo deja como principal.
    pub fn add_cursor_vertical(&mut self, up: bool) -> bool {
        let row = if up {
            self.cursor.row.checked_sub(1)
        } else {
            Some(self.cursor.row + 1).filter(|row| *row < self.lines.len())
        };
        let Some(row) = row else {
            return false;
        };
        let col = self.cursor.col.min(self.lines[row].chars().count());
        let mut all = self.cursors();
        all.insert(0, (Pos::new(row, col), Pos::new(row, col)));
        self.set_cursors(all);
        true
    }
    /// Selecciona la palabra del cursor o, si ya hay algo seleccionado, añade
    /// su siguiente aparición y conserva las anteriores.
    pub fn select_next_also(&mut self) -> bool {
        let mut all = self.cursors();
        let selecting = all[0].0 != all[0].1;
        if !self.select_next() {
            return false;
        }
        if selecting {
            all.insert(0, (self.anchor.unwrap_or(self.cursor), self.cursor));
            self.set_cursors(all);
        }
        true
    }
    /// Texto de todas las selecciones, una por línea y en el orden del documento.
    pub fn selected_all(&self) -> String {
        let mut all = self.cursors();
        all.sort_by_key(|(a, b)| *a.min(b));
        all.iter()
            .filter(|(a, b)| a != b)
            .map(|(a, b)| {
                let (from, to) = (self.offset(*a.min(b)), self.offset(*a.max(b)));
                &self.text[from..to]
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Aplica `edit` en todos los cursores como un solo paso de deshacer.
    pub fn edit_cursors(&mut self, edit: Edit) {
        if !self.format.editable() {
            return;
        }
        let total = self.index(self.end());
        // Tramo que se sustituye, texto nuevo y a qué cursor pertenece.
        let mut changes: Vec<(usize, usize, String, usize)> = self
            .cursors()
            .iter()
            .enumerate()
            .map(|(i, (anchor, cursor))| {
                let (a, b) = (self.index(*anchor), self.index(*cursor));
                let (from, to) = (a.min(b), a.max(b));
                let empty = from == to;
                match &edit {
                    Edit::Insert(text) => (from, to, text.to_string(), i),
                    Edit::Backspace if empty => (from.saturating_sub(1), to, String::new(), i),
                    Edit::Delete if empty => (from, (to + 1).min(total), String::new(), i),
                    Edit::Backspace | Edit::Delete => (from, to, String::new(), i),
                    Edit::Newline => {
                        let line = &self.lines[self.position(from).row];
                        let indent: String =
                            line.chars().take_while(|c| c.is_whitespace()).collect();
                        (from, to, format!("\n{indent}"), i)
                    }
                    Edit::Indent => (from, to, self.unit(), i),
                }
            })
            .collect();
        changes.sort_by_key(|change| (change.0, change.1));
        // Dos cursores que tocan el mismo tramo cuentan como uno.
        let mut last = None;
        changes.retain(|change| {
            let keep = last.is_none_or(|end| change.0 >= end && (change.0, change.1) != (end, end));
            if keep {
                last = Some(change.1);
            }
            keep
        });
        self.remember();
        let spans: Vec<(Pos, Pos)> = changes
            .iter()
            .map(|change| (self.position(change.0), self.position(change.1)))
            .collect();
        // De atrás hacia delante, las posiciones anteriores siguen valiendo.
        for (change, (a, b)) in changes.iter().zip(&spans).rev() {
            self.splice(*a, *b, &change.2);
        }
        let mut shift = 0isize;
        let mut carets: Vec<(usize, usize)> = changes
            .iter()
            .map(|(from, to, text, owner)| {
                let added = text.chars().count();
                let caret = (*from as isize + shift) as usize + added;
                shift += added as isize - (*to - *from) as isize;
                (*owner, caret)
            })
            .collect();
        // El principal sigue siendo el primero; si se fundió con otro, lo es el más cercano.
        carets.sort_by_key(|(owner, _)| *owner);
        let all = carets
            .iter()
            .map(|(_, caret)| {
                let pos = self.position(*caret);
                (pos, pos)
            })
            .collect();
        self.set_cursors(all);
    }
    /// Mueve todos los cursores; con `extend`, cada uno alarga su selección.
    pub fn move_cursors(&mut self, movement: Move, extend: bool) {
        let all = self
            .cursors()
            .into_iter()
            .map(|(anchor, cursor)| {
                let (low, high) = (anchor.min(cursor), anchor.max(cursor));
                let length = |row: usize| self.lines[row].chars().count();
                let moved = match movement {
                    // Sin alargar, una selección se reduce a su extremo.
                    Move::Left if !extend && low != high => low,
                    Move::Right if !extend && low != high => high,
                    Move::Left => self.previous(cursor),
                    Move::Right => self.next(cursor),
                    Move::Up if cursor.row == 0 => Pos::new(0, 0),
                    Move::Up => Pos::new(cursor.row - 1, cursor.col.min(length(cursor.row - 1))),
                    Move::Down if cursor.row + 1 >= self.lines.len() => self.end(),
                    Move::Down => Pos::new(cursor.row + 1, cursor.col.min(length(cursor.row + 1))),
                    Move::Home => Pos::new(cursor.row, 0),
                    Move::End => Pos::new(cursor.row, length(cursor.row)),
                };
                (if extend { anchor } else { moved }, moved)
            })
            .collect();
        self.set_cursors(all);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        Editor::new(text.into(), Some("notas.txt".into()))
    }

    #[test]
    fn edits_and_moves_every_cursor() {
        let mut e = editor("uno\ndos\ntres");
        e.goto(0, 1);
        assert!(e.add_cursor_vertical(false));
        assert!(e.add_cursor_vertical(false));
        assert!(!e.add_cursor_vertical(false));
        assert_eq!(e.extras().len(), 2);
        assert_eq!(e.cursor, Pos::new(2, 1));
        e.edit_cursors(Edit::Insert("ñ-"));
        assert_eq!(e.text(), "uñ-no\ndñ-os\ntñ-res");
        assert_eq!(e.cursor, Pos::new(2, 3));
        assert_eq!(
            e.extras(),
            [
                (Pos::new(1, 3), Pos::new(1, 3)),
                (Pos::new(0, 3), Pos::new(0, 3))
            ]
        );
        e.edit_cursors(Edit::Backspace);
        assert_eq!(e.text(), "uñno\ndños\ntñres");
        e.move_cursors(Move::Home, false);
        e.edit_cursors(Edit::Delete);
        assert_eq!(e.text(), "ñno\nños\nñres");
        e.move_cursors(Move::End, true);
        assert_eq!(e.selected_all(), "ñno\nños\nñres");
        e.edit_cursors(Edit::Insert("x"));
        assert_eq!(e.text(), "x\nx\nx");
        // Todo ello se deshace paso a paso.
        e.undo(false);
        assert_eq!(e.text(), "ñno\nños\nñres");
        assert!(e.extras().is_empty());
        e.undo(false);
        e.undo(false);
        e.undo(false);
        assert_eq!(e.text(), "uno\ndos\ntres");
    }

    #[test]
    fn adds_occurrences_and_merges_cursors_that_meet() {
        let mut e = editor("ab ab\nab");
        e.goto(0, 0);
        assert!(e.select_next_also());
        assert!(e.extras().is_empty());
        assert!(e.select_next_also());
        assert!(e.select_next_also());
        assert_eq!(e.extras().len(), 2);
        assert_eq!(e.selection(), (Pos::new(1, 0), Pos::new(1, 2)));
        // Al dar la vuelta no se añade dos veces la misma aparición.
        e.select_next_also();
        assert_eq!(e.extras().len(), 2);
        e.edit_cursors(Edit::Insert("xyz"));
        assert_eq!(e.text(), "xyz xyz\nxyz");
        // Una edición por otra vía deja un solo cursor.
        e.insert("!");
        assert!(e.extras().is_empty());
        // Los cursores que coinciden tras moverse se funden.
        let mut e = editor("a\nb");
        e.goto(0, 1);
        e.add_cursor_vertical(false);
        e.move_cursors(Move::Up, false);
        e.move_cursors(Move::Up, false);
        assert!(e.extras().is_empty());
        e.add_cursor(Pos::new(1, 0), Pos::new(1, 1));
        e.edit_cursors(Edit::Newline);
        assert_eq!(e.text(), "\na\n\n");
    }
}
