//! Panel del editor, pestañas e inserciones.

use super::*;

impl App {
    pub(super) fn editor_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let mut close = None;
                egui::Frame::new()
                    .fill(col(self.theme.surface))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 4.0);
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut activate = None;
                                for (i, doc) in self.documents.iter().enumerate() {
                                    let label = format!(
                                        "{}{}",
                                        doc.editor.title(),
                                        if doc.editor.dirty() { " *" } else { "" }
                                    );
                                    ui.push_id(doc.id, |ui| {
                                        let tab = ui.selectable_label(i == self.active, label)
                                            .on_hover_text(doc.editor.path.as_ref().map_or_else(|| "Documento sin guardar".into(), |p| p.display().to_string()));
                                        if i == self.active {
                                            ui.painter().line_segment([tab.rect.left_bottom(), tab.rect.right_bottom()], Stroke::new(2.0, col(self.theme.primary)));
                                        } else if self.split.is_some_and(|panes| panes.contains(&doc.id)) {
                                            // El documento del otro panel de la vista dividida.
                                            ui.painter().line_segment([tab.rect.left_bottom(), tab.rect.right_bottom()], Stroke::new(2.0, col(self.theme.muted())));
                                        }
                                        if tab.clicked() { activate = Some(i); }
                                        let name = format!("Cerrar {}", doc.editor.title());
                                        let response = ui.add(egui::Button::new("×").frame_when_inactive(false)).on_hover_text(&name);
                                        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
                                        if response.clicked() { close = Some(i); }
                                    });
                                }
                                if let Some(index) = activate {
                                    self.activate(index);
                                }
                            });
                        });
                    });
                if let Some(i) = close {
                    self.request_close(Pending::Close(i), &ctx);
                }
                if self.editor().format == Format::Pdf {
                    egui::Frame::new()
                        .inner_margin(16.0)
                        .show(ui, |ui| self.pdf_view(ui));
                    return;
                }
                if self.editor().format == Format::Image {
                    let uri = format!("file://{}", self.editor().path.as_ref().unwrap().display());
                    egui::Frame::new().inner_margin(16.0).show(ui, |ui| {
                        ui.label("Imagen · solo lectura");
                        if action(ui, "Recargar imagen", true, "Vuelve a leer esta imagen del disco.").clicked() {
                            ui.ctx().forget_image(&uri);
                        }
                        ui.separator();
                        ScrollArea::both()
                            .id_salt(self.documents[self.active].id.with("image_scroll"))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Image::new(&uri)
                                        .max_size(ui.available_size())
                                        .show_loading_spinner(true),
                                );
                            });
                    });
                    return;
                }
                if self.find {
                    if self.editor().query != self.query {
                        let query = self.query.clone();
                        self.editor_mut().search(&query);
                    }
                    // Mayús indica hacia atrás.
                    let mut step = None;
                    let mut close = ui.input(|i| i.key_pressed(Key::Escape))
                        && self.editor().completions.is_empty();
                    egui::Frame::new()
                        .fill(self.panel_fill())
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Buscar");
                                let response = ui.add(
                                    TextEdit::singleline(&mut self.query)
                                        .id_salt("find_query")
                                        .return_key(None)
                                        .desired_width(180.0),
                                );
                                if self.focus_find {
                                    response.request_focus();
                                    self.focus_find = false;
                                    self.focus_editor = false;
                                }
                                if response.changed() {
                                    let query = self.query.clone();
                                    self.editor_mut().search(&query);
                                }
                                // Enter sigue buscando sin salir del campo.
                                if response.has_focus()
                                    && Self::shortcut(&ctx, Modifiers::NONE, Key::Enter)
                                {
                                    step = Some(ui.input(|i| i.modifiers.shift));
                                }
                                let editor = &mut self.documents[self.active].editor;
                                let mut options = false;
                                for (flag, label, help) in [
                                    (&mut editor.search_case, "Aa", "Distinguir mayúsculas"),
                                    (&mut editor.search_word, "ab", "Solo palabras completas"),
                                    (&mut editor.search_regex, ".*", "Expresión regular"),
                                ] {
                                    options |=
                                        ui.toggle_value(flag, label).on_hover_text(help).changed();
                                }
                                if options {
                                    let query = self.query.clone();
                                    self.editor_mut().search(&query);
                                }
                                let found = !self.editor().matches.is_empty();
                                if action(ui, "Anterior", found, "Selecciona la coincidencia anterior. Mayús+Enter. Requiere coincidencias.").clicked() {
                                    step = Some(true);
                                    self.focus_editor = true;
                                }
                                if action(ui, "Siguiente", found, "Selecciona la coincidencia siguiente. Enter. Requiere coincidencias.").clicked() {
                                    step = Some(false);
                                    self.focus_editor = true;
                                }
                                let count = self.editor().matches.len();
                                if count == 0 && !self.query.is_empty() {
                                    ui.label(
                                        RichText::new("Sin coincidencias")
                                            .color(col(self.theme.error)),
                                    );
                                } else if let Some(current) = marks::current_match(self.editor()) {
                                    ui.label(format!("{current} de {count}"));
                                } else {
                                    ui.label(format!("{count} coincidencias"));
                                }
                                close |= action(ui, "Cerrar búsqueda", true, "Cierra la búsqueda y vuelve al editor. Esc.").clicked();
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Reemplazar por");
                                ui.add(
                                    TextEdit::singleline(&mut self.replacement)
                                        .desired_width(180.0),
                                );
                                let selected = self.editor().matches.contains(&self.editor().selection());
                                if action(ui, "Reemplazar coincidencia", selected, "Cambia solo la coincidencia seleccionada y pasa a la siguiente. Selecciona una coincidencia con Anterior o Siguiente.").clicked() {
                                    let replacement = self.replacement.clone();
                                    self.editor_mut().insert(&replacement);
                                    self.changed_editor();
                                    self.message = "Coincidencia reemplazada. Puedes deshacer el cambio.".into();
                                    step = Some(false);
                                    self.focus_editor = true;
                                }
                                if action(ui, "Reemplazar todas", !self.editor().matches.is_empty(), "Reemplaza todas las coincidencias del documento activo. Puedes deshacerlo en un paso. Requiere coincidencias.").clicked() {
                                    let count = self.editor().matches.len();
                                    let replacement = self.replacement.clone();
                                    self.editor_mut().replace_all(&replacement);
                                    self.changed_editor();
                                    self.message = format!("{count} coincidencias reemplazadas. Puedes deshacer el cambio.");
                                    self.focus_editor = true;
                                }
                            });
                        });
                    if let Some(backwards) = step {
                        self.editor_mut().find_next(backwards);
                        self.sync_cursor = true;
                    }
                    if close {
                        self.find = false;
                        self.focus_editor = true;
                    }
                }
                self.editor_panes(ui);
            });
    }
    /// Texto de un documento. `primary` es el que recibe el teclado; del
    /// otro panel de la vista dividida se devuelve si pide pasar a serlo.
    fn editor_body(&mut self, ui: &mut egui::Ui, index: usize, primary: bool) -> bool {
        let ctx = ui.ctx().clone();
        let rect = ui.available_rect_before_wrap();
        if primary {
            self.editor_keys(&ctx);
        }
        let follow_cursor = primary && self.sync_cursor;
        let doc = &mut self.documents[index];
        // Un salto a una línea plegada (buscar, ir a línea, un problema) la despliega.
        if follow_cursor && doc.editor.is_hidden(doc.editor.cursor.row) {
            doc.editor.unfold_at(doc.editor.cursor.row);
        }
        let mut toggle_fold = None;
        if follow_cursor {
            let mut state = egui::text_edit::TextEditState::load(&ctx, doc.id).unwrap_or_default();
            let range = CCursorRange::two(
                CCursor::new(
                    doc.editor
                        .index(doc.editor.anchor.unwrap_or(doc.editor.cursor)),
                ),
                CCursor::new(doc.editor.index(doc.editor.cursor)),
            );
            state.cursor.set_char_range(Some(range));
            state.store(&ctx, doc.id);
            self.sync_cursor = false;
        }
        let format = doc.editor.format.clone();
        let theme = self.theme.clone();
        let size = self.config.font_size as f32;
        let wrap = self.config.soft_wrap;
        let spacing = self.config.line_height as f32;
        let numbers = self.config.line_numbers;
        let highlight_line = self.config.highlight_line;
        let guides = self.config.indent_guides;
        let find = self.find;
        let completions = self.config.completions;
        let shadow = if self.backdrop.image.is_some() {
            (self.config.text_shadow * 255.0).round() as u8
        } else {
            0
        };
        let line_height = (spacing > 1.0)
            .then(|| ui.fonts_mut(|f| f.row_height(&FontId::monospace(size))) * spacing);
        doc.editor.syntax.set_dark(theme.dark);
        if doc.editor.highlight() {
            // Queda resaltado pendiente para el próximo cuadro.
            ctx.request_repaint();
        }
        let look = Look {
            theme: &theme,
            size,
            line_height,
            fonts: layout::fonts_epoch(ui),
        };
        // El editor es el búfer del widget, así que se saca su maquetado.
        let mut text_layout = std::mem::take(&mut doc.layout);
        let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
            let width = if wrap { width } else { f32::INFINITY };
            match layout::editor(buffer) {
                Some(editor) => text_layout.galley(ui, editor, &look, width),
                None => ui.fonts_mut(|f| {
                    f.layout_job(LayoutJob::simple(
                        buffer.as_str().into(),
                        FontId::monospace(size),
                        col(theme.fg),
                        width,
                    ))
                }),
            }
        };
        let mut cursor_position = None;
        let mut changed_document = false;
        // La ortografía solo se revisa en el panel que tiene el foco.
        let spelling = primary && self.spell.begin(&ctx, &doc.editor, doc.id, &self.config);
        let mut wants = false;
        let mut corrected = false;
        // Problemas de la última compilación en este archivo.
        let problems: Vec<&Diagnostic> = self
            .diagnostics
            .iter()
            .filter(|d| Some(&d.path) == doc.editor.path.as_ref())
            .collect();
        let mut definition = None;
        if doc.changes.0 != doc.editor.revision {
            let marks = doc
                .git
                .as_ref()
                .and_then(|git| git.base.as_deref())
                .map(|base| crate::git::marks(base, doc.editor.source()))
                .unwrap_or_default();
            doc.changes = (doc.editor.revision, marks);
        }
        let changes = std::mem::take(&mut doc.changes.1);
        ScrollArea::both()
            .id_salt(doc.id.with("scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_size(rect.size());
                ui.horizontal_top(|ui| {
                    let gutter = if numbers {
                        let digits = doc.editor.lines.len().to_string().len().max(3);
                        digits as f32 * size * 0.62 + 16.0
                    } else {
                        8.0
                    };
                    let text_width = (rect.width() - gutter - 16.0).max(100.0);
                    let origin = ui.cursor().min;
                    // Reservado para pintar bajo el texto la sombra y la línea actual.
                    let under_text = ui.painter().add(egui::Shape::Noop);
                    let mut under = Vec::new();
                    ui.add_space(gutter);
                    let before = (
                        doc.editor.anchor.unwrap_or(doc.editor.cursor),
                        doc.editor.cursor,
                    );
                    let output = TextEdit::multiline(&mut doc.editor)
                        .id(doc.id)
                        .font(FontId::monospace(size))
                        .frame(egui::Frame::NONE)
                        .margin(egui::vec2(8.0, 10.0))
                        .desired_width(text_width)
                        .desired_rows(1)
                        // Ocupa todo el alto: un clic bajo el texto lleva el cursor al final.
                        .min_size(egui::vec2(0.0, rect.height()))
                        .code_editor()
                        .event_filter(egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        })
                        .layouter(&mut layouter)
                        .show(ui);
                    if primary && self.focus_editor {
                        // Pedir el foco otra vez borra el filtro de Tab y flechas.
                        if !output.response.has_focus() {
                            output.response.request_focus();
                            // Sin el foco egui reduce la selección al cursor:
                            // se vuelve a llevar al widget cuando ya lo tiene.
                            self.sync_cursor = true;
                        }
                        self.focus_editor = false;
                    }
                    // Un clic en el otro panel lo convierte en el activo.
                    wants = !primary
                        && (output.response.has_focus()
                            || output.response.is_pointer_button_down_on());
                    // Alt y clic añade un cursor; cualquier otro clic deja uno solo.
                    let alt = ui.input(|i| i.modifiers.alt);
                    let add_cursor = alt && output.response.clicked();
                    if output.response.is_pointer_button_down_on() && !alt {
                        doc.editor.clear_extras();
                    }
                    let changed = doc.editor.take_touched();
                    if changed {
                        self.edited_at = (format == Format::Latex
                            || doc
                                .editor
                                .path
                                .as_ref()
                                .is_some_and(|p| latex::is_source(p)))
                        .then(Instant::now);
                        self.save_at = self.edited_at;
                        changed_document = true;
                    }
                    if let Some(range) = output.cursor_range {
                        doc.editor
                            .select(range.primary.index.0, range.secondary.index.0);
                        if add_cursor {
                            doc.editor.add_cursor(before.0, before.1);
                        }
                        // Las flechas no entran en lo plegado: lo saltan.
                        if doc.editor.skip_hidden(before.1) {
                            self.sync_cursor = true;
                            ctx.request_repaint();
                        }
                        let cursor_rect = output.galley.pos_from_cursor(range.primary);
                        let cursor_rect = cursor_rect.translate(output.galley_pos.to_vec2());
                        cursor_position = Some(cursor_rect.left_bottom());
                        if highlight_line && range.is_empty() {
                            let color = col(theme.highlight()).gamma_multiply(0.6);
                            under.push(egui::Shape::rect_filled(
                                egui::Rect::from_x_y_ranges(
                                    ui.clip_rect().x_range(),
                                    cursor_rect.y_range(),
                                ),
                                0.0,
                                color,
                            ));
                        }
                        if output.response.has_focus() && (changed || follow_cursor) {
                            ui.scroll_to_rect(cursor_rect, None);
                        }
                    } else if follow_cursor {
                        // Sin el foco (se busca desde la barra) el widget no
                        // informa del cursor: se lleva la vista hasta él.
                        let cursor = CCursor::new(doc.editor.index(doc.editor.cursor));
                        let cursor_rect = output
                            .galley
                            .pos_from_cursor(cursor)
                            .translate(output.galley_pos.to_vec2());
                        ui.scroll_to_rect(cursor_rect, Some(egui::Align::Center));
                    }
                    let column_width =
                        ui.fonts_mut(|f| f.glyph_width(&FontId::monospace(size), ' '));
                    let marks = Marks::new(
                        &doc.editor,
                        &theme,
                        output.response.has_focus(),
                        find,
                        guides,
                        column_width,
                    );
                    let visible = ui.clip_rect().y_range();
                    let scrim = col(theme.bg).gamma_multiply_u8(shadow);
                    let mut shadows = Vec::new();
                    let mut line = 1;
                    let mut starts_line = true;
                    // Columna de la línea con que empieza cada fila visual.
                    let mut column = 0;
                    let mut misspelled = Vec::new();
                    // Los triángulos de plegado solo se ven con el puntero en el margen.
                    let over_margin = ui.rect_contains_pointer(egui::Rect::from_x_y_ranges(
                        origin.x..=origin.x + gutter,
                        ui.clip_rect().y_range(),
                    ));
                    for row in &output.galley.rows {
                        let row_rect = row.rect().translate(output.galley_pos.to_vec2());
                        let shown = !doc.editor.is_hidden(line - 1);
                        if shown && starts_line && visible.intersects(row_rect.y_range()) {
                            let folded = doc.editor.folded(line - 1);
                            if folded || (over_margin && doc.editor.foldable(line - 1)) {
                                let area = egui::Rect::from_x_y_ranges(
                                    origin.x..=origin.x + 12.0,
                                    row_rect.y_range(),
                                );
                                let (x, y) = (area.left() + 2.0, area.center().y);
                                let points = if folded {
                                    vec![
                                        egui::pos2(x + 2.0, y - 4.0),
                                        egui::pos2(x + 7.0, y),
                                        egui::pos2(x + 2.0, y + 4.0),
                                    ]
                                } else {
                                    vec![
                                        egui::pos2(x, y - 2.5),
                                        egui::pos2(x + 8.0, y - 2.5),
                                        egui::pos2(x + 4.0, y + 2.5),
                                    ]
                                };
                                misspelled.push(egui::Shape::convex_polygon(
                                    points,
                                    col(theme.muted()),
                                    Stroke::NONE,
                                ));
                                let help = if folded {
                                    "Desplegar este bloque. F9."
                                } else {
                                    "Plegar este bloque. F9."
                                };
                                let id = doc.id.with(("fold", line));
                                if ui
                                    .interact(area, id, egui::Sense::click())
                                    .on_hover_text(help)
                                    .clicked()
                                {
                                    toggle_fold = Some(line - 1);
                                }
                            }
                            if folded {
                                // Señal de que tras esta línea hay texto oculto.
                                let mark = egui::Rect::from_min_size(
                                    egui::pos2(row_rect.right() + 8.0, row_rect.center().y - 6.0),
                                    egui::vec2(22.0, 12.0),
                                );
                                misspelled.push(egui::Shape::rect_filled(
                                    mark,
                                    3.0,
                                    col(theme.muted()).gamma_multiply(0.35),
                                ));
                                for dot in 0..3 {
                                    misspelled.push(egui::Shape::circle_filled(
                                        egui::pos2(
                                            mark.left() + 5.0 + 6.0 * dot as f32,
                                            mark.center().y,
                                        ),
                                        1.3,
                                        col(theme.fg),
                                    ));
                                }
                            }
                        }
                        if shown && visible.intersects(row_rect.y_range()) {
                            marks.row(
                                &doc.editor,
                                line - 1,
                                column,
                                &row.glyphs,
                                row_rect,
                                starts_line,
                                &mut under,
                            );
                            if spelling && !row.glyphs.is_empty() {
                                self.spell.underline(
                                    &doc.editor,
                                    line - 1,
                                    column,
                                    &row.glyphs,
                                    row_rect.left(),
                                    row_rect.bottom(),
                                    col(theme.error),
                                    &mut misspelled,
                                );
                            }
                            // Cambios desde el último commit, junto al texto.
                            let first = changes.partition_point(|(row, _)| *row < line - 1);
                            if let Some((_, mark)) =
                                changes.get(first).filter(|(row, _)| *row == line - 1)
                            {
                                let x = origin.x + gutter - 2.5;
                                let (color, y) = match mark {
                                    crate::git::Mark::Added => (theme.success, row_rect.y_range()),
                                    crate::git::Mark::Modified => {
                                        (theme.warning, row_rect.y_range())
                                    }
                                    crate::git::Mark::Removed => (
                                        theme.error,
                                        (row_rect.top() - 2.0..=row_rect.top() + 2.0).into(),
                                    ),
                                };
                                if !matches!(mark, crate::git::Mark::Removed) || starts_line {
                                    misspelled.push(egui::Shape::rect_filled(
                                        egui::Rect::from_x_y_ranges(x - 1.0..=x + 1.0, y),
                                        0.0,
                                        col(color),
                                    ));
                                }
                            }
                            let here = || problems.iter().filter(|d| d.row == line - 1);
                            if here().next().is_some() {
                                let color = col(if here().any(|d| d.error) {
                                    theme.error
                                } else {
                                    theme.warning
                                });
                                // Se subraya el texto, no su sangría.
                                if let Some(glyph) =
                                    row.glyphs.iter().find(|g| !g.chr.is_whitespace())
                                {
                                    misspelled.push(marks::squiggle(
                                        row_rect.left() + glyph.pos.x,
                                        row_rect.right(),
                                        row_rect.bottom(),
                                        color,
                                    ));
                                }
                                if starts_line {
                                    let margin = egui::Rect::from_x_y_ranges(
                                        origin.x..=origin.x + gutter,
                                        row_rect.y_range(),
                                    );
                                    misspelled.push(egui::Shape::circle_filled(
                                        egui::pos2(origin.x + 5.0, margin.center().y),
                                        3.0,
                                        color,
                                    ));
                                    let id = doc.id.with(("problem", line));
                                    ui.interact(margin, id, egui::Sense::hover()).on_hover_text(
                                        here()
                                            .map(|d| d.message.as_str())
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    );
                                }
                            }
                            // La foto queda detrás de una sombra del color del fondo.
                            let left = if starts_line && numbers {
                                origin.x
                            } else {
                                row_rect.left() - 4.0
                            };
                            if shadow > 0 && (!row.glyphs.is_empty() || left == origin.x) {
                                let right = if row.glyphs.is_empty() {
                                    origin.x + gutter - 2.0
                                } else {
                                    row_rect.right() + 6.0
                                };
                                shadows.push(egui::Shape::rect_filled(
                                    egui::Rect::from_x_y_ranges(left..=right, row_rect.y_range()),
                                    0.0,
                                    scrim,
                                ));
                            }
                            if starts_line && numbers {
                                let mut format = egui::TextFormat::simple(
                                    FontId::monospace(size),
                                    col(theme.muted()),
                                );
                                format.line_height = line_height;
                                let number = ui.painter().layout_job(LayoutJob::single_section(
                                    line.to_string(),
                                    format,
                                ));
                                ui.painter().galley(
                                    egui::pos2(
                                        origin.x + gutter - 6.0 - number.size().x,
                                        row_rect.top(),
                                    ),
                                    number,
                                    col(theme.muted()),
                                );
                            }
                        }
                        starts_line = row.ends_with_newline;
                        if starts_line {
                            line += 1;
                            column = 0;
                        } else {
                            column += row.glyphs.len();
                        }
                    }
                    shadows.append(&mut under);
                    ui.painter().set(under_text, egui::Shape::Vec(shadows));
                    ui.painter().extend(misspelled);
                    // Cmd o Ctrl y clic llevan a la definición de lo señalado.
                    if output.response.clicked()
                        && ui.input(|i| i.modifiers.command)
                        && let Some(pointer) = output.response.interact_pointer_pos()
                    {
                        let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
                        definition = Some(doc.editor.position(cursor.index.0));
                    }
                    if spelling
                        && output.response.secondary_clicked()
                        && let Some(pointer) = output.response.interact_pointer_pos()
                    {
                        let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
                        self.spell.target(&doc.editor, cursor.index.0);
                    }
                    if spelling && self.spell.has_menu() {
                        output.response.context_menu(|ui| {
                            corrected |= self.spell.menu_ui(ui, &mut doc.editor);
                        });
                    }
                });
            });
        self.documents[index].layout = text_layout;
        self.documents[index].changes.1 = changes;
        if let Some(row) = toggle_fold {
            self.toggle_fold(index, row);
        }
        if !primary {
            return wants;
        }
        if corrected {
            self.changed_editor();
        }
        if let Some(pos) = definition {
            self.goto_definition(pos);
        }
        if changed_document && completions {
            self.update_completion();
        }
        if let Some(pos) = cursor_position
            && !self.editor().completions.is_empty()
            && ctx.memory(|m| m.has_focus(self.documents[self.active].id))
        {
            let completions = self.editor().completions.clone();
            let selected = self.editor().completion_index;
            let mut chosen = None;
            egui::Window::new("Sugerencias")
                .id(Id::new("completions"))
                .title_bar(false)
                .resizable(false)
                .fixed_pos(pos)
                .default_width(340.0)
                .show(&ctx, |ui| {
                    ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                        for (i, completion) in completions.iter().enumerate() {
                            // En el código, al lado va qué es cada sugerencia.
                            let text: egui::WidgetText =
                                if matches!(completion.kind.as_str(), "word" | "snippet") {
                                    let mut job = LayoutJob::default();
                                    job.append(
                                        &completion.label,
                                        0.0,
                                        egui::TextFormat::simple(
                                            egui::TextStyle::Button.resolve(ui.style()),
                                            ui.visuals().text_color(),
                                        ),
                                    );
                                    job.append(
                                        &completion.detail,
                                        12.0,
                                        egui::TextFormat::simple(
                                            FontId::proportional(12.0),
                                            col(self.theme.muted()),
                                        ),
                                    );
                                    job.into()
                                } else {
                                    (&completion.label).into()
                                };
                            let response = ui
                                .selectable_label(i == selected, text)
                                .on_hover_text(&completion.detail);
                            if i == selected {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
                    ui.label(
                        RichText::new("Tab para insertar")
                            .size(12.0)
                            .color(col(self.theme.muted())),
                    );
                });
            if let Some(i) = chosen {
                self.editor_mut().completion_index = i;
                self.editor_mut().accept_completion();
                self.update_completion();
                self.changed_editor();
            }
        }
        wants
    }
    /// Pliega o despliega un bloque; el cursor que quede dentro sube a su primera línea.
    fn toggle_fold(&mut self, index: usize, row: usize) {
        let editor = &mut self.documents[index].editor;
        if editor.toggle_fold(row) && editor.skip_hidden(crate::editor::Pos::new(usize::MAX, 0)) {
            self.sync_cursor = true;
        }
    }
    /// Pliega el bloque del cursor o, si no encabeza ninguno, el que lo contiene.
    pub(super) fn toggle_fold_at_cursor(&mut self) {
        let row = self.editor().cursor.row;
        let start = if self.editor().fold_end(row).is_some() {
            Some(row)
        } else {
            self.editor().enclosing_fold(row)
        };
        match start {
            Some(start) => self.toggle_fold(self.active, start),
            None => self.message = "Aquí no hay ningún bloque que plegar".into(),
        }
    }
    pub(super) fn fold_everything(&mut self, fold: bool) {
        let editor = self.editor_mut();
        if fold {
            editor.fold_all();
            editor.skip_hidden(crate::editor::Pos::new(usize::MAX, 0));
        } else {
            editor.unfold_all();
        }
        self.sync_cursor = true;
    }
    /// Los dos documentos de la vista dividida, si los dos siguen abiertos.
    fn split_panes(&mut self) -> Option<[usize; 2]> {
        let mut panes = self.split?;
        let active = self.documents[self.active].id;
        match panes.iter().position(|id| *id == active) {
            Some(slot) => self.split_focus = slot,
            // Al elegir otra pestaña, ocupa el panel que tenía el foco.
            None => panes[self.split_focus] = active,
        }
        let find = |id: Id| {
            self.documents
                .iter()
                .position(|d| d.id == id && d.editor.format.editable())
        };
        let indices = find(panes[0]).zip(find(panes[1])).map(|(a, b)| [a, b]);
        self.split = indices.map(|_| panes);
        indices
    }
    pub(super) fn toggle_split(&mut self) {
        if self.split.take().is_some() {
            return;
        }
        let count = self.documents.len();
        let other = (1..count)
            .map(|step| (self.active + step) % count)
            .find(|i| self.documents[*i].editor.format.editable());
        match other {
            Some(other) if self.editor().format.editable() => {
                self.split = Some([self.documents[self.active].id, self.documents[other].id]);
                self.split_focus = 0;
            }
            _ => self.message = "Abre otro documento de texto para dividir el editor".into(),
        }
    }
    fn editor_panes(&mut self, ui: &mut egui::Ui) {
        let Some(panes) = self.split_panes() else {
            self.editor_body(ui, self.active, true);
            return;
        };
        let mut activate = None;
        let border = Stroke::new(1.0, col(self.theme.border));
        ui.columns(2, |columns| {
            for (slot, index) in panes.into_iter().enumerate() {
                let ui = &mut columns[slot];
                if slot == 1 {
                    let edge = ui.max_rect();
                    ui.painter()
                        .vline(edge.left() - 4.0, edge.y_range(), border);
                }
                if self.editor_body(ui, index, index == self.active) {
                    activate = Some(index);
                }
            }
        });
        if let Some(index) = activate {
            self.activate(index);
        }
    }
    pub(super) fn insert_snippet(&mut self, text: &str) {
        let (a, b) = self.editor().selection();
        let selected = self.editor().selected();
        let text = if selected.is_empty() {
            text.to_owned()
        } else {
            text.replacen("$0", &selected, 1)
        };
        self.editor_mut().snippet(&text, a, b);
        self.changed_editor();
    }
    pub(super) fn insert_figure(&mut self) {
        let Some(root) = self.root() else {
            self.message = "Guarda el documento antes de insertar una figura".into();
            return;
        };
        if let Some(file) = rfd::FileDialog::new()
            .set_title("Insertar figura")
            .set_directory(root.parent().unwrap())
            .add_filter("Figuras", &["png", "jpg", "jpeg", "pdf"])
            .pick_file()
        {
            self.insert_image(&file);
        }
    }
    /// Una imagen soltada sobre la ventana se inserta en el documento activo
    /// si es LaTeX o Markdown y ya está guardado; si no, se abre en una pestaña.
    pub(super) fn accepts_image(&self, file: &Path) -> bool {
        let extension = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match self.editor().format {
            Format::Latex => {
                self.root().is_some() && ["png", "jpg", "jpeg"].contains(&extension.as_str())
            }
            Format::Markdown => {
                self.editor().path.is_some()
                    && ["png", "jpg", "jpeg", "gif", "webp"].contains(&extension.as_str())
            }
            _ => false,
        }
    }
    /// Inserta la imagen del portapapeles, por ejemplo una captura de pantalla.
    pub(super) fn paste_image(&mut self) {
        let image = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get_image())
            .ok()
            .and_then(|data| {
                image::RgbaImage::from_raw(
                    data.width as u32,
                    data.height as u32,
                    data.bytes.into_owned(),
                )
            });
        match image {
            Some(image) => self.insert_pasted(&image),
            None => self.message = "El portapapeles no tiene una imagen".into(),
        }
    }
    /// Guarda `image` como PNG en `images/` y la inserta en el cursor.
    pub(super) fn insert_pasted(&mut self, image: &image::RgbaImage) {
        let file = std::env::temp_dir().join(format!("miyu-pegada-{}.png", std::process::id()));
        if !self.accepts_image(&file) {
            self.message =
                "Para pegar una imagen, guarda antes el documento LaTeX o Markdown".into();
            return;
        }
        match image.save(&file) {
            Ok(()) => self.insert_image(&file),
            Err(e) => self.message = format!("No pude guardar la imagen: {e}"),
        }
        let _ = fs::remove_file(&file);
    }
    /// Inserta `file` como figura de LaTeX o imagen de Markdown. Si está fuera
    /// de la carpeta del documento, o su ruta no sirve en LaTeX, se copia a
    /// `images/`.
    pub(super) fn insert_image(&mut self, file: &Path) {
        let markdown = self.editor().format == Format::Markdown;
        let anchor = if markdown {
            self.editor().path.clone()
        } else {
            self.root()
        };
        let Some(base) = anchor.as_deref().and_then(Path::parent) else {
            self.message = "Guarda el documento antes de insertar una imagen".into();
            return;
        };
        let file = file.canonicalize().unwrap_or_else(|_| file.into());
        let base = base.canonicalize().unwrap_or_else(|_| base.into());
        let mut relative = file.strip_prefix(&base).ok().map(PathBuf::from);
        if relative.as_ref().is_none_or(|p| {
            p.to_string_lossy()
                .contains(['{', '}', '%', '#', '$', '\\'])
        }) {
            let images = base.join("images");
            let result = (|| {
                fs::create_dir_all(&images)?;
                let extension = file.extension().unwrap_or_default().to_string_lossy();
                let mut index = 1;
                loop {
                    let destination = images.join(format!("figura-{index}.{extension}"));
                    match fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&destination)
                    {
                        Ok(mut output) => {
                            if let Err(e) = fs::File::open(&file)
                                .and_then(|mut input| std::io::copy(&mut input, &mut output))
                            {
                                let _ = fs::remove_file(&destination);
                                return Err(e);
                            }
                            return Ok(destination);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => index += 1,
                        Err(e) => return Err(e),
                    }
                }
            })();
            match result {
                Ok(path) => relative = path.strip_prefix(&base).ok().map(PathBuf::from),
                Err(e) => {
                    self.message = e.to_string();
                    return;
                }
            }
        }
        let path = relative.unwrap().to_string_lossy().replace('\\', "/");
        self.files = project_files(&self.project);
        if markdown {
            // Markdown exige los ángulos cuando la ruta lleva espacios.
            let target = if path.contains(' ') {
                format!("<{path}>")
            } else {
                path
            };
            self.insert_snippet(&format!("![$0]({target})"));
            return;
        }
        self.insert_snippet(&format!("\\begin{{figure}}[htbp]\n    \\centering\n    \\includegraphics[width=0.8\\linewidth]{{{path}}}\n    \\caption{{$0}}\n    \\label{{fig:}}\n\\end{{figure}}"));
        let text = self.editor().text();
        if !crate::editor::regex(r"\\usepackage(?:\[[^\]]*\])?\s*\{[^}]*\bgraphicx\b[^}]*\}")
            .is_match(&latex::code(&text))
        {
            self.message =
                "Figura insertada. El documento principal necesita \\usepackage{graphicx}.".into();
        }
        self.refresh_sources();
    }
    pub(super) fn jump(&mut self, target: &Target) {
        match self.open(&target.path) {
            Ok(()) => {
                self.editor_mut().goto(target.row, target.col);
                self.sync_cursor = true;
                self.focus_editor = true;
            }
            Err(e) => self.message = e,
        }
    }
}
