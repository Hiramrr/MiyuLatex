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
        ui.horizontal_wrapped(|ui| {
            ui.label("PDF");
            if action(
                ui,
                "Anterior",
                count > 0 && self.pdf().page > 0,
                "Muestra la página anterior del PDF.",
            )
            .clicked()
            {
                self.pdf_mut().change_page(-1);
            }
            let mut page = if count == 0 { 0 } else { self.pdf().page + 1 };
            if count == 0 {
                ui.label("0");
            } else if ui
                .add(egui::DragValue::new(&mut page).range(1..=count))
                .on_hover_text("Número de página. Escribe un número para ir a esa página.")
                .changed()
            {
                self.pdf_mut().go_to(page.saturating_sub(1));
            }
            ui.label(format!("/ {count}"));
            if action(
                ui,
                "Siguiente",
                self.pdf().page + 1 < count,
                "Muestra la página siguiente del PDF.",
            )
            .clicked()
            {
                self.pdf_mut().change_page(1);
            }
            if self.pdf().loading {
                ui.spinner();
            }
        });
        ui.horizontal_wrapped(|ui| {
            let zoom = self.pdf().zoom;
            let reduce = action(
                ui,
                "−",
                count > 0 && zoom > 50.5,
                "Reducir zoom. Requiere un PDF cargado y un zoom mayor que 50 %.",
            );
            reduce.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    reduce.enabled(),
                    "Reducir zoom",
                )
            });
            if reduce.clicked() {
                self.pdf_mut().change_zoom(-1);
            }
            ui.label(format!("{zoom:.0} %"));
            let increase = action(
                ui,
                "+",
                count > 0 && zoom < 299.5,
                "Aumentar zoom. Requiere un PDF cargado y un zoom menor que 300 %.",
            );
            increase.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    increase.enabled(),
                    "Aumentar zoom",
                )
            });
            if increase.clicked() {
                self.pdf_mut().change_zoom(1);
            }
            if action(
                ui,
                "Ajustar al ancho",
                count > 0,
                "Ajusta la página al ancho disponible. Requiere un PDF cargado.",
            )
            .clicked()
            {
                self.pdf_mut().zoom = 100.0;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if action(ui, "Abrir en visor externo", has_pdf, "Abre este PDF en el visor del sistema. F6. Abre o compila un PDF primero.").clicked() { self.open_pdf(); }
            if action(ui, "Exportar PDF…", has_pdf, "Guarda una copia de este PDF en otra ubicación. Requiere un PDF abierto o compilado.").clicked() { self.export_pdf(); }
            if action(ui, "Mostrar línea en PDF", self.tool_rx.is_none() && self.root().is_some() && has_pdf, "Lleva el PDF a la línea del cursor. Cmd/Ctrl+Mayús+J. Requiere un archivo LaTeX guardado y su PDF.").clicked() { self.sync_to_pdf(ui.ctx()); }
            if action(ui, "Recargar PDF", has_pdf, "Vuelve a leer este PDF del disco, sin compilar el documento.").clicked()
                && let Some(path) = self.pdf_path().map(Path::to_path_buf)
                && let Err(e) = self.pdf_mut().load(&path) { self.message = e; }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Buscar");
            let mut query = self.pdf().query().to_owned();
            let response = ui.add_enabled(
                count > 0,
                TextEdit::singleline(&mut query)
                    .id_salt("pdf_find")
                    .hint_text("Texto del PDF")
                    .return_key(None)
                    .desired_width(150.0),
            );
            if std::mem::take(&mut self.focus_pdf_find) {
                response.request_focus();
            }
            if response.changed() {
                self.pdf_mut().search(&query);
            }
            // Enter sigue buscando sin salir del campo; Mayús indica hacia atrás.
            let mut step = None;
            if response.has_focus() && Self::shortcut(ui.ctx(), Modifiers::NONE, Key::Enter) {
                step = Some(ui.input(|i| i.modifiers.shift));
            }
            let (current, total) = self.pdf().found();
            if action(
                ui,
                "Anterior",
                total > 0,
                "Va a la coincidencia anterior del PDF. Mayús+Enter. Requiere coincidencias.",
            )
            .clicked()
            {
                step = Some(true);
            }
            if action(
                ui,
                "Siguiente",
                total > 0,
                "Va a la coincidencia siguiente del PDF. Enter. Requiere coincidencias.",
            )
            .clicked()
            {
                step = Some(false);
            }
            if let Some(backwards) = step {
                self.pdf_mut().find_next(backwards);
            }
            if !query.is_empty() {
                match current {
                    _ if total == 0 && self.pdf().reading() => ui.label("Leyendo el texto…"),
                    _ if total == 0 => {
                        ui.label(RichText::new("Sin coincidencias").color(col(self.theme.error)))
                    }
                    Some(current) => ui.label(format!("{current} de {total}")),
                    None => ui.label(format!("{total} coincidencias")),
                };
            }
        });
        ui.separator();
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
            .default_size(155.0)
            .min_size(85.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.log, false, "Problemas");
                    ui.selectable_value(&mut self.log, true, "Registro");
                    if action(
                        ui,
                        "Ocultar panel",
                        true,
                        "Oculta problemas y registro. F4 vuelve a mostrarlos.",
                    )
                    .clicked()
                    {
                        self.panel = false;
                    }
                });
                ui.separator();
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
                                if ui
                                    .button(
                                        RichText::new(format!(
                                            "{} · {location} · {}",
                                            problem.severity, problem.message
                                        ))
                                        .color(col(color)),
                                    )
                                    .clicked()
                                {
                                    jump =
                                        Some((Self::problem_path(result, problem), problem.line));
                                }
                            }
                        }
                    } else {
                        ui.label("Todavía no hay una compilación.");
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
