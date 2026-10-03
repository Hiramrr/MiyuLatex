//! Vista previa de PDF y Markdown, y lista de problemas.

use super::*;

impl App {
    pub(super) fn pdf(&self) -> &Preview {
        self.documents[self.active]
            .pdf
            .as_ref()
            .unwrap_or(&self.preview)
    }
    pub(super) fn pdf_path(&self) -> Option<&Path> {
        if self.editor().format == Format::Pdf || self.root().is_some() {
            self.pdf().path.as_deref()
        } else {
            None
        }
    }
    pub(super) fn pdf_mut(&mut self) -> &mut Preview {
        self.documents[self.active]
            .pdf
            .as_mut()
            .unwrap_or(&mut self.preview)
    }
    pub(super) fn pdf_panel(&mut self, ui: &mut egui::Ui) {
        side_panel("preview", !self.config.preview_left)
            .default_size(430.0)
            .min_size(230.0)
            .show(ui, |ui| self.pdf_view(ui));
    }
    pub(super) fn pdf_view(&mut self, ui: &mut egui::Ui) {
        let count = self.pdf().count;
        let has_pdf = self.pdf_path().is_some();
        if self.focus_pdf_find {
            self.pdf_mut().search_open = true;
        }
        egui::Frame::new()
            .fill(col(self.theme.surface))
            .inner_margin(egui::Margin::symmetric(6, 3))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
                ui.spacing_mut().interact_size = egui::vec2(24.0, 24.0);
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 4.0);
                let wide = ui.available_width() >= 360.0;
                ui.horizontal(|ui| {
                    if wide {
                        ui.label("PDF");
                        if toolbar::toolbar_action(
                            ui,
                            "‹",
                            "Página anterior",
                            count > 0 && self.pdf().page > 0,
                            "Página anterior",
                        )
                        .clicked()
                        {
                            self.pdf_mut().change_page(-1);
                        }
                    }
                    let mut page = if count == 0 { 0 } else { self.pdf().page + 1 };
                    if ui
                        .add_enabled(
                            count > 0,
                            egui::DragValue::new(&mut page).range(1..=count.max(1)),
                        )
                        .on_hover_text("Número de página. Escribe un número para ir a esa página.")
                        .changed()
                    {
                        self.pdf_mut().go_to(page.saturating_sub(1));
                    }
                    ui.label(format!("/ {count}"));
                    if wide
                        && toolbar::toolbar_action(
                            ui,
                            "›",
                            "Página siguiente",
                            self.pdf().page + 1 < count,
                            "Página siguiente",
                        )
                        .clicked()
                    {
                        self.pdf_mut().change_page(1);
                    }
                    ui.menu_button(format!("{:.0} %", self.pdf().zoom), |ui| {
                        let zoom = self.pdf().zoom;
                        if action(
                            ui,
                            "Reducir zoom",
                            count > 0 && zoom > 50.5,
                            "Reduce el tamaño de las páginas.",
                        )
                        .clicked()
                        {
                            self.pdf_mut().change_zoom(-1);
                        }
                        if action(
                            ui,
                            "Aumentar zoom",
                            count > 0 && zoom < 299.5,
                            "Aumenta el tamaño de las páginas.",
                        )
                        .clicked()
                        {
                            self.pdf_mut().change_zoom(1);
                        }
                        ui.separator();
                        if action(
                            ui,
                            "Ajustar al ancho",
                            count > 0,
                            "Ajusta la página al ancho del panel.",
                        )
                        .clicked()
                        {
                            self.pdf_mut().zoom = 100.0;
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Zoom del PDF");
                    if pdf_search_button(ui, count > 0).clicked() {
                        self.pdf_mut().search_open = !self.pdf().search_open;
                        self.focus_pdf_find = self.pdf().search_open;
                    }
                    ui.menu_button("…", |ui| {
                        if !wide {
                            if action(
                                ui,
                                "Página anterior",
                                count > 0 && self.pdf().page > 0,
                                "Muestra la página anterior.",
                            )
                            .clicked()
                            {
                                self.pdf_mut().change_page(-1);
                            }
                            if action(
                                ui,
                                "Página siguiente",
                                self.pdf().page + 1 < count,
                                "Muestra la página siguiente.",
                            )
                            .clicked()
                            {
                                self.pdf_mut().change_page(1);
                            }
                            ui.separator();
                        }
                        if action(
                            ui,
                            "Abrir en visor externo",
                            has_pdf,
                            "Abre este PDF en el visor del sistema. F6.",
                        )
                        .clicked()
                        {
                            self.open_pdf();
                            ui.close();
                        }
                        if action(
                            ui,
                            "Exportar PDF…",
                            has_pdf,
                            "Guarda una copia de este PDF.",
                        )
                        .clicked()
                        {
                            self.export_pdf();
                            ui.close();
                        }
                        if action(
                            ui,
                            "Mostrar línea en PDF",
                            self.tool_rx.is_none() && self.root().is_some() && has_pdf,
                            "Lleva el PDF a la línea del cursor. Cmd/Ctrl+Mayús+J.",
                        )
                        .clicked()
                        {
                            self.sync_to_pdf(ui.ctx());
                            ui.close();
                        }
                        if action(
                            ui,
                            "Recargar PDF",
                            has_pdf,
                            "Vuelve a leer este PDF del disco.",
                        )
                        .clicked()
                        {
                            if let Some(path) = self.pdf_path().map(Path::to_path_buf)
                                && let Err(e) = self.pdf_mut().load(&path)
                            {
                                self.message = e;
                            }
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Acciones del PDF")
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            "Acciones del PDF",
                        )
                    });
                    if self.pdf().loading {
                        ui.spinner();
                    }
                });
                if self.pdf().search_open {
                    ui.horizontal(|ui| {
                        let mut query = self.pdf().query().to_owned();
                        let response = ui.add_enabled(
                            count > 0,
                            TextEdit::singleline(&mut query)
                                .id_salt(self.documents[self.active].id.with("pdf_find"))
                                .hint_text("Buscar en PDF")
                                .return_key(None)
                                .desired_width((ui.available_width() - 84.0).max(40.0)),
                        );
                        if std::mem::take(&mut self.focus_pdf_find) {
                            response.request_focus();
                        }
                        if response.changed() {
                            self.pdf_mut().search(&query);
                        }
                        let mut step = None;
                        if response.has_focus()
                            && Self::shortcut(ui.ctx(), Modifiers::NONE, Key::Enter)
                        {
                            step = Some(ui.input(|i| i.modifiers.shift));
                        }
                        let (_, total) = self.pdf().found();
                        if toolbar::toolbar_action(
                            ui,
                            "‹",
                            "Coincidencia anterior",
                            total > 0,
                            "Coincidencia anterior. Mayús+Enter.",
                        )
                        .clicked()
                        {
                            step = Some(true);
                        }
                        if toolbar::toolbar_action(
                            ui,
                            "›",
                            "Coincidencia siguiente",
                            total > 0,
                            "Coincidencia siguiente. Enter.",
                        )
                        .clicked()
                        {
                            step = Some(false);
                        }
                        if let Some(backwards) = step {
                            self.pdf_mut().find_next(backwards);
                        }
                        if toolbar::toolbar_action(
                            ui,
                            "×",
                            "Cerrar búsqueda del PDF",
                            true,
                            "Cerrar búsqueda. Esc.",
                        )
                        .clicked()
                            || ((response.has_focus() || response.lost_focus())
                                && Self::shortcut(ui.ctx(), Modifiers::NONE, Key::Escape))
                        {
                            self.pdf_mut().search_open = false;
                            self.pdf_mut().search("");
                            response.surrender_focus();
                        }
                    });
                    if !self.pdf().query().is_empty() {
                        let (current, total) = self.pdf().found();
                        match current {
                            _ if total == 0 && self.pdf().reading() => {
                                ui.label("Leyendo el texto…")
                            }
                            _ if total == 0 => {
                                ui.colored_label(col(self.theme.error), "Sin coincidencias")
                            }
                            Some(current) => ui.label(format!("{current} de {total}")),
                            None => ui.label(format!("{total} coincidencias")),
                        };
                    }
                }
            });
        if !self.pdf().error.is_empty() {
            ui.colored_label(col(self.theme.error), &self.pdf().error);
        }
        let id = self.documents[self.active].id.with("pdf_scroll");
        let accent = col(self.theme.primary);
        let marker = self.pdf_marker;
        let mut reveal = self.scroll_pdf_marker;
        let clicked = self.pdf_mut().show(ui, id, marker, &mut reveal, accent);
        self.scroll_pdf_marker = reveal;
        if let Some((page, x, y)) = clicked
            && self.tool_rx.is_none()
            && let Some(pdf) = self.pdf().path.clone()
        {
            self.start_tool(ui.ctx(), move || {
                compiler::sync_back(&pdf, page, x, y)
                    .map(|(path, line)| ToolResult::Back(path, line))
            });
        }
    }
    pub(super) fn markdown_panel(&mut self, ui: &mut egui::Ui) {
        let id = self.documents[self.active].id;
        let base = self
            .editor()
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .unwrap_or(&self.project)
            .to_path_buf();
        let scheme = format!("file://{}/", base.display());
        let editor = &self.documents[self.active].editor;
        let text = editor.source();
        // Un solo análisis por revisión da los trozos de la vista y los enlaces.
        let scroll_id = id.with("markdown_scroll");
        self.markdown_view.update(scroll_id, text, editor.revision);
        let links = std::mem::take(&mut self.markdown_view.links);
        self.markdown_cache.link_hooks_clear();
        for link in links.iter() {
            self.markdown_cache.add_link_hook(link);
        }
        side_panel("markdown_preview", !self.config.preview_left)
            .default_size(430.0)
            .min_size(230.0)
            .frame(egui::Frame::side_top_panel(ui.style()).fill(col(self.theme.bg)))
            .show(ui, |ui| {
                ui.label("Vista previa de Markdown");
                ui.separator();
                let width = (ui.available_width() - 32.0).clamp(1.0, 720.0);
                ui.style_mut()
                    .text_styles
                    .insert(egui::TextStyle::Body, FontId::proportional(18.0));
                ui.spacing_mut().item_spacing.y = 10.0;
                self.markdown_view.show(
                    ui,
                    scroll_id,
                    &mut self.markdown_cache,
                    text,
                    editor.revision,
                    width,
                    |ui| {
                        CommonMarkViewer::new()
                            .enable_scroll_to_heading(true)
                            .default_implicit_uri_scheme(scheme.clone())
                            .max_image_width(Some(ui.available_width().max(1.0) as usize))
                    },
                );
            });
        let clicked = links
            .iter()
            .find(|link| self.markdown_cache.get_link_hook(link) == Some(true))
            .map(|link| base.join(link.split('#').next().unwrap_or(link)));
        self.markdown_view.links = links;
        if let Some(path) = clicked
            && let Err(e) = self.open(&path)
        {
            self.message = e;
        }
    }
    pub(super) fn problems(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("problems")
            .resizable(true)
            .default_size(260.0)
            .min_size(150.0)
            .max_size((ui.available_height() - 100.0).max(150.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(!self.developer.selected && !self.log, "Problemas")
                        .clicked()
                    {
                        self.developer.selected = false;
                        self.log = false;
                    }
                    if ui
                        .selectable_label(!self.developer.selected && self.log, "Registro")
                        .clicked()
                    {
                        self.developer.selected = false;
                        self.log = true;
                    }
                    if ui
                        .selectable_label(self.developer.selected, "Terminal")
                        .clicked()
                    {
                        self.show_terminal(ui.ctx());
                    }
                    if action(
                        ui,
                        "Ocultar panel",
                        true,
                        "Oculta el panel inferior. Las terminales siguen abiertas.",
                    )
                    .clicked()
                    {
                        self.panel = false;
                        self.focus_editor = true;
                    }
                    if !self.developer.selected
                        && action(
                            ui,
                            "Copiar",
                            true,
                            "Copia al portapapeles los problemas o el registro que se ven en el panel.",
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(self.panel_text());
                        self.message = if self.log {
                            "Registro copiado".into()
                        } else {
                            "Problemas copiados".into()
                        };
                    }
                });
                ui.separator();
                if self.developer.selected {
                    self.terminal_panel(ui);
                    return;
                }
                let mut jump = None;
                ScrollArea::both().id_salt("diagnostics").show(ui, |ui| {
                    if let Some(result) = &self.result {
                        if self.log {
                            ui.label(RichText::new(&result.output).monospace().size(13.0));
                        } else {
                            if result.problems.is_empty() {
                                ui.label("Sin problemas en la última compilación.");
                            }
                            for problem in &result.problems {
                                let color = match problem.severity.as_str() {
                                    "error" => self.theme.error,
                                    "warning" => self.theme.warning,
                                    _ => self.theme.muted(),
                                };
                                let location = format!(
                                    "{}{}",
                                    problem.file.display(),
                                    problem.line.map_or(String::new(), |n| format!(":{n}"))
                                );
                                let text =
                                    format!("{} · {location} · {}", problem.severity, problem.message);
                                let button = ui.button(RichText::new(&text).color(col(color)));
                                copy_menu(&button, text);
                                if button.clicked() {
                                    jump =
                                        Some((Self::problem_path(result, problem), problem.line));
                                }
                            }
                        }
                    } else {
                        ui.label("Todavía no hay una compilación.");
                    }
                    if !self.log {
                        self.language_server_problems(ui, &mut jump);
                    }
                });
                if let Some((path, line)) = jump {
                    match self.open(&path) {
                        Ok(()) => {
                            self.editor_mut()
                                .goto(line.unwrap_or(1).saturating_sub(1), 0);
                            self.sync_cursor = true;
                            self.focus_editor = true;
                        }
                        Err(e) => self.message = e,
                    }
                }
            });
    }
}

fn pdf_search_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .min_size(egui::vec2(24.0, 24.0))
            .frame_when_inactive(false),
    );
    let center = response.rect.center() - egui::vec2(1.5, 1.5);
    let stroke = Stroke::new(1.5, ui.style().interact(&response).fg_stroke.color);
    ui.painter().circle_stroke(center, 4.5, stroke);
    ui.painter().line_segment(
        [center + egui::vec2(3.0, 3.0), center + egui::vec2(7.0, 7.0)],
        stroke,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Buscar en PDF")
    });
    response.on_hover_text("Buscar en el PDF. Cmd/Ctrl+F en una pestaña PDF.")
}

/// Clic derecho sobre un problema: copiar su texto.
pub(super) fn copy_menu(response: &egui::Response, text: String) {
    response.context_menu(|ui| {
        if ui.button("Copiar").clicked() {
            ui.ctx().copy_text(text);
            ui.close();
        }
    });
}

impl App {
    /// Lo que muestra el panel de problemas, en texto plano y sin el tope de filas.
    pub(super) fn panel_text(&self) -> String {
        let Some(result) = &self.result else {
            return self.language_server_text();
        };
        if self.log {
            return result.output.clone();
        }
        let mut lines: Vec<String> = result
            .problems
            .iter()
            .map(|problem| {
                format!(
                    "{} · {}{} · {}",
                    problem.severity,
                    problem.file.display(),
                    problem.line.map_or(String::new(), |n| format!(":{n}")),
                    problem.message
                )
            })
            .collect();
        lines.push(self.language_server_text());
        lines.retain(|line| !line.is_empty());
        lines.join("\n")
    }
}
