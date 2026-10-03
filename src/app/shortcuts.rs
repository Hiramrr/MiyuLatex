//! Atajos de teclado de la ventana y del editor.

use super::*;

impl App {
    pub(super) fn shortcut(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
        ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
    }
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        // Los atajos con Mayús van antes: sin ella coinciden también los simples.
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::F) {
            self.search.open = true;
            self.search_project();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::O) {
            self.open_quick();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::J) {
            self.sync_to_pdf(ctx);
        }
        if Self::shortcut(ctx, cmd | Modifiers::SHIFT, Key::N) {
            self.new_project();
        } else if Self::shortcut(ctx, cmd, Key::N) {
            self.templates = true;
        }
        if Self::shortcut(ctx, cmd, Key::O) {
            self.open_dialog();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            self.save_document(self.active, true);
        } else if Self::shortcut(ctx, cmd, Key::S) {
            self.save_document(self.active, false);
        }
        if Self::shortcut(ctx, cmd, Key::W) {
            self.request_close(Pending::Close(self.active), ctx);
        }
        if Self::shortcut(ctx, cmd, Key::Q) {
            self.request_close(Pending::Quit, ctx);
        }
        if Self::shortcut(ctx, cmd, Key::R) || Self::shortcut(ctx, Modifiers::NONE, Key::F5) {
            self.compile(false, ctx);
        }
        if Self::shortcut(ctx, cmd, Key::F) {
            self.start_find();
        }
        if Self::shortcut(ctx, Modifiers::CTRL, Key::Space) && self.config.completions {
            self.editor_mut().request_completion();
            self.update_completion();
        }
        if Self::shortcut(ctx, cmd, Key::G) {
            self.start_goto();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::I) {
            self.format_document(ctx);
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::M) {
            self.toggle_equation();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::P) {
            self.open_palette();
        } else if Self::shortcut(ctx, cmd, Key::P) || Self::shortcut(ctx, cmd, Key::Comma) {
            self.settings = true;
        }
        if Self::shortcut(ctx, cmd, Key::T) && self.editor().format == Format::Latex {
            self.symbols = true;
        }
        if Self::shortcut(ctx, Modifiers::CTRL, Key::Tab) {
            self.activate((self.active + 1) % self.documents.len());
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F1) {
            self.help = !self.help;
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F2) {
            self.config.show_sidebar = !self.config.show_sidebar;
            self.preferences_changed(ctx);
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F3) {
            self.config.show_preview = !self.config.show_preview;
            self.preferences_changed(ctx);
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F4) {
            self.panel = !self.panel;
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F6) {
            self.open_pdf();
        }
        if Self::shortcut(ctx, Modifiers::SHIFT, Key::F8) {
            self.next_problem(true);
        } else if Self::shortcut(ctx, Modifiers::NONE, Key::F8) {
            self.next_problem(false);
        }
        if Self::shortcut(ctx, Modifiers::NONE, Key::F12) {
            self.goto_definition(self.editor().cursor);
        }
    }
    pub(super) fn editor_keys(&mut self, ctx: &egui::Context) {
        let id = self.documents[self.active].id;
        if !self.editor().format.editable()
            || !ctx.memory(|m| m.has_focus(id))
            || self.pending.is_some()
        {
            return;
        }
        let mut changed = false;
        // egui solo pega texto: con una imagen en el portapapeles no avisa de
        // nada al pulsar, y al soltar la V se sabe que no llegó ningún texto.
        let (pasted, released) = ctx.input(|input| {
            let pasted = input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::Paste(_)));
            let released = input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key { key: Key::V, pressed: false, modifiers, .. }
                        if modifiers.command && !modifiers.shift
                )
            });
            (pasted, released)
        });
        self.pasted_text |= pasted;
        if released
            && !std::mem::take(&mut self.pasted_text)
            && self.accepts_image(Path::new("x.png"))
        {
            self.paste_image();
        }
        if Self::shortcut(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || Self::shortcut(ctx, Modifiers::COMMAND, Key::Y)
        {
            self.editor_mut().undo(true);
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::Z) {
            self.editor_mut().undo(false);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::B) {
            self.editor_mut().emphasize(true);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::I) {
            self.editor_mut().emphasize(false);
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::Slash) {
            self.editor_mut().rewrite_lines(true, false);
            changed = true;
        }
        // Mueven el cursor o la selección sin editar el texto.
        let mut moved = false;
        let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
        if Self::shortcut(ctx, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowDown)
            || Self::shortcut(ctx, shifted, Key::D)
        {
            self.editor_mut().duplicate_lines();
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::D) {
            moved |= self.editor_mut().select_next_also();
        }
        // Con Cmd y Alt se añade un cursor; solo con Alt se mueven las líneas.
        let both = Modifiers::COMMAND | Modifiers::ALT;
        if Self::shortcut(ctx, both, Key::ArrowUp) {
            moved |= self.editor_mut().add_cursor_vertical(true);
        }
        if Self::shortcut(ctx, both, Key::ArrowDown) {
            moved |= self.editor_mut().add_cursor_vertical(false);
        }
        if Self::shortcut(ctx, Modifiers::ALT, Key::ArrowUp) {
            changed |= self.editor_mut().move_lines(true);
        }
        if Self::shortcut(ctx, Modifiers::ALT, Key::ArrowDown) {
            changed |= self.editor_mut().move_lines(false);
        }
        if Self::shortcut(ctx, shifted, Key::K) {
            self.editor_mut().delete_lines();
            changed = true;
        }
        if Self::shortcut(ctx, Modifiers::COMMAND, Key::L) {
            self.editor_mut().select_line();
            moved = true;
        }
        if Self::shortcut(ctx, shifted, Key::Enter) {
            self.editor_mut().open_line(true);
            changed = true;
        } else if Self::shortcut(ctx, Modifiers::COMMAND, Key::Enter) {
            self.editor_mut().open_line(false);
            changed = true;
        }
        if Self::shortcut(ctx, shifted, Key::Backslash)
            || Self::shortcut(ctx, Modifiers::CTRL, Key::M)
        {
            moved |= self.editor_mut().jump_bracket();
        }
        if matches!(self.editor().format, Format::Code(_)) {
            let select = ctx.input(|i| i.modifiers.shift);
            if Self::shortcut(ctx, Modifiers::NONE, Key::Home)
                || (cfg!(target_os = "macos")
                    && Self::shortcut(ctx, Modifiers::MAC_CMD, Key::ArrowLeft))
            {
                self.editor_mut().smart_home(select);
                moved = true;
            }
        }
        if !self.editor().extras().is_empty() && self.cursor_keys(ctx) {
            changed = true;
        }
        if moved {
            self.sync_cursor = true;
        }
        // ponytail: egui procesa los lotes en orden; el emparejado usa eventos individuales.
        let edits_in_frame = ctx.input(|input| {
            input
                .events
                .iter()
                .filter(|event| match event {
                    egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut => true,
                    egui::Event::Key {
                        key, pressed: true, ..
                    } => matches!(
                        key,
                        Key::Enter
                            | Key::Backspace
                            | Key::Delete
                            | Key::Tab
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::Home
                            | Key::End
                    ),
                    _ => false,
                })
                .count()
        });
        if edits_in_frame > 1 {
            if changed {
                self.changed_editor();
            }
            return;
        }
        let mut edits = Vec::new();
        let completion = !self.editor().completions.is_empty();
        let pairs = self.config.auto_pairs;
        let latex = self.editor().format == Format::Latex;
        let code = matches!(self.editor().format, Format::Code(_));
        // Sin selección, copiar y cortar actúan sobre la línea entera.
        let whole_line = self
            .editor()
            .anchor
            .is_none_or(|a| a == self.editor().cursor);
        self.editor_mut().tab = self.config.tab_width;
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let special = match event {
                    egui::Event::Copy | egui::Event::Cut => whole_line,
                    egui::Event::Key {
                        key: Key::Tab,
                        pressed: true,
                        modifiers,
                        ..
                    } if *modifiers == Modifiers::SHIFT => true,
                    egui::Event::Text(text) => {
                        pairs
                            && text.chars().count() == 1
                            && text.chars().next().is_some_and(|c| {
                                "{}[]()".contains(c)
                                    || (latex && c == '$')
                                    || (code && "\"'".contains(c))
                            })
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if *modifiers == Modifiers::NONE => {
                        matches!(key, Key::Enter | Key::Tab)
                            || (pairs && *key == Key::Backspace)
                            || (completion
                                && matches!(key, Key::ArrowUp | Key::ArrowDown | Key::Escape))
                    }
                    _ => false,
                };
                if special {
                    edits.push(event.clone());
                }
                !special
            })
        });
        for event in edits {
            match event {
                egui::Event::Text(s) => {
                    self.editor_mut().smart_char(s.chars().next().unwrap());
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Enter, ..
                } => {
                    self.editor_mut().newline();
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Backspace,
                    ..
                } => {
                    self.editor_mut().backspace();
                    changed = true;
                }
                egui::Event::Copy => ctx.copy_text(self.editor().line_text()),
                egui::Event::Cut => {
                    ctx.copy_text(self.editor().line_text());
                    self.editor_mut().delete_lines();
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::Tab,
                    modifiers,
                    ..
                } => {
                    if modifiers.shift {
                        self.editor_mut().rewrite_lines(false, true);
                    } else if !self.editor().completions.is_empty() {
                        self.editor_mut().accept_completion();
                    } else if self.editor().selection().0 != self.editor().selection().1 {
                        self.editor_mut().rewrite_lines(false, false);
                    } else {
                        self.editor_mut().indent_cursor();
                    }
                    changed = true;
                }
                egui::Event::Key {
                    key: Key::ArrowUp, ..
                } => {
                    self.editor_mut().completion_index =
                        self.editor().completion_index.saturating_sub(1);
                }
                egui::Event::Key {
                    key: Key::ArrowDown,
                    ..
                } => {
                    self.editor_mut().completion_index = (self.editor().completion_index + 1)
                        .min(self.editor().completions.len().saturating_sub(1));
                }
                egui::Event::Key {
                    key: Key::Escape, ..
                } => self.editor_mut().completions.clear(),
                _ => {}
            }
        }
        if changed {
            if self.config.completions {
                self.update_completion();
            }
            self.changed_editor();
        }
    }
    /// Con varios cursores, las teclas de edición y de movimiento no llegan
    /// al widget de texto: se aplican aquí en todos a la vez. Devuelve si
    /// cambió el texto.
    fn cursor_keys(&mut self, ctx: &egui::Context) -> bool {
        use crate::editor::cursors::{Edit, Move};
        let mut taken = Vec::new();
        ctx.input_mut(|input| {
            input.events.retain(|event| {
                let ours = match event {
                    egui::Event::Text(_) | egui::Event::Paste(_) => true,
                    egui::Event::Copy | egui::Event::Cut => true,
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        let plain = modifiers.is_none() || *modifiers == Modifiers::SHIFT;
                        match key {
                            Key::Backspace | Key::Delete | Key::Enter | Key::Tab | Key::Escape => {
                                modifiers.is_none()
                            }
                            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                                plain
                            }
                            Key::Home | Key::End => plain,
                            _ => false,
                        }
                    }
                    _ => false,
                };
                if ours {
                    taken.push(event.clone());
                }
                !ours
            })
        });
        let mut changed = false;
        for event in taken {
            let editor = self.editor_mut();
            let mut edit = |editor: &mut Editor, edit: Edit| {
                editor.edit_cursors(edit);
                changed = true;
            };
            match event {
                egui::Event::Text(text) | egui::Event::Paste(text) => {
                    edit(editor, Edit::Insert(&text));
                }
                egui::Event::Copy => ctx.copy_text(editor.selected_all()),
                egui::Event::Cut => {
                    ctx.copy_text(editor.selected_all());
                    edit(editor, Edit::Insert(""));
                }
                egui::Event::Key { key, modifiers, .. } => match key {
                    Key::Backspace => edit(editor, Edit::Backspace),
                    Key::Delete => edit(editor, Edit::Delete),
                    Key::Enter => edit(editor, Edit::Newline),
                    Key::Tab => edit(editor, Edit::Indent),
                    Key::Escape => editor.clear_extras(),
                    Key::ArrowLeft => editor.move_cursors(Move::Left, modifiers.shift),
                    Key::ArrowRight => editor.move_cursors(Move::Right, modifiers.shift),
                    Key::ArrowUp => editor.move_cursors(Move::Up, modifiers.shift),
                    Key::ArrowDown => editor.move_cursors(Move::Down, modifiers.shift),
                    Key::Home => editor.move_cursors(Move::Home, modifiers.shift),
                    Key::End => editor.move_cursors(Move::End, modifiers.shift),
                    _ => {}
                },
                _ => {}
            }
        }
        // El widget conserva su cursor: se le lleva el del editor.
        self.sync_cursor = true;
        changed
    }
    pub(super) fn start_find(&mut self) {
        if self.editor().format == Format::Pdf {
            self.focus_pdf_find = true;
        }
        if !self.editor().format.editable() {
            return;
        }
        let selected = self.editor().selected();
        if !selected.is_empty() && !selected.contains('\n') {
            self.query = selected;
        }
        self.find = true;
        self.focus_find = true;
        self.editor_mut().completions.clear();
    }
    pub(super) fn start_goto(&mut self) {
        if !self.editor().format.editable() {
            return;
        }
        self.goto = true;
        self.line = self.editor().cursor.row + 1;
    }
}
