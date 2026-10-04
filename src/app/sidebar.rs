//! Panel lateral: archivos, esquema y referencias.

use super::*;

impl App {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        side_panel("files", self.config.sidebar_right)
            .frame(egui::Frame::side_top_panel(ui.style()).fill(col(self.theme.surface)))
            .default_size(240.0)
            .min_size(160.0)
            .show(ui, |ui| self.sidebar_content(ui));
    }
    /// El estado del botón indica si el panel lateral está visible.
    pub(super) fn sidebar_toggle(&mut self, ui: &mut egui::Ui) {
        let shown = self.config.show_sidebar && !self.zen;
        let (label, help) = if shown {
            (
                "Ocultar panel lateral",
                "Oculta el panel de archivos, esquema y referencias. F2.",
            )
        } else {
            (
                "Mostrar panel lateral",
                "Muestra el panel de archivos, esquema y referencias. F2.",
            )
        };
        let response = ui
            .add(
                egui::Button::new("Panel")
                    .selected(shown)
                    .frame_when_inactive(shown),
            )
            .on_hover_text(help);
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Button, true, shown, label)
        });
        if response.clicked() && !self.leave_zen() {
            self.config.show_sidebar = !shown;
            self.preferences_changed(ui.ctx());
        }
    }
    pub(super) fn sidebar_content(&mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(5.0, 4.0);
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(
                        !self.outline && !self.references && !self.git_active(),
                        "Archivos",
                    )
                    .on_hover_text("Explora y gestiona los archivos de la carpeta del proyecto.")
                    .clicked()
                {
                    self.outline = false;
                    self.references = false;
                    self.git.tab = false;
                }
                if ui
                    .selectable_label(self.outline, "Esquema")
                    .on_hover_text("Ve a una sección o definición del documento.")
                    .clicked()
                {
                    self.outline = true;
                    self.references = false;
                }
                if ui
                    .selectable_label(self.references, "Referencias")
                    .on_hover_text("Inserta citas y referencias LaTeX o abre sus definiciones.")
                    .clicked()
                {
                    self.outline = false;
                    self.references = true;
                }
                if ui
                    .selectable_label(self.git_active(), "Git")
                    .on_hover_text("Cambios, commits, ramas, historial y stash del repositorio.")
                    .clicked()
                {
                    self.show_git(ui.ctx());
                }
            });
        });
        ui.separator();
        if self.git_active() {
            self.git_sidebar(ui);
        } else if self.references {
            ui.add(
                TextEdit::singleline(&mut self.reference_query)
                    .hint_text("Clave, autor o título")
                    .desired_width(f32::INFINITY),
            );
            let sources = self.completion_sources();
            let query = self.reference_query.to_lowercase();
            ScrollArea::vertical().id_salt("references").show(ui, |ui| {
                        for (heading, command, targets) in [
                            ("Etiquetas", "ref", latex::labels(&sources)),
                            ("Bibliografía", "cite", latex::citations(&sources)),
                        ] {
                            ui.label(heading);
                            let mut found = false;
                            for target in targets {
                                if !format!("{} {}", target.label, target.detail)
                                    .to_lowercase()
                                    .contains(&query)
                                {
                                    continue;
                                }
                                found = true;
                                ui.horizontal_wrapped(|ui| {
                                    if ui
                                        .add_enabled(
                                            self.editor().format == Format::Latex,
                                            egui::Button::new(format!("Insertar {}", target.label)),
                                        )
                                        .on_disabled_hover_text("Abre un documento LaTeX para insertar una cita o referencia.")
                                        .on_hover_text(format!(
                                            "Insertar \\{command}{{{}}}\n{}\n{}:{}",
                                            target.label, target.detail,
                                            target.path.display(),
                                            target.row + 1
                                        ))
                                        .clicked()
                                    {
                                        self.insert_snippet(&format!(
                                            "\\{command}{{{}}}",
                                            target.label
                                        ));
                                    }
                                    if ui
                                        .button("Ver origen")
                                        .on_hover_text("Abre el archivo y la línea donde se define esta etiqueta o cita.")
                                        .clicked()
                                    {
                                        self.jump(&target);
                                    }
                                });
                            }
                            if !found {
                                ui.label("Sin coincidencias.");
                            }
                            ui.separator();
                        }
                    });
        } else if self.outline {
            let mut outline = self.source_outline.clone();
            if self.editor().format != Format::Latex {
                outline.clear();
                for (row, level, title) in self.editor().outline() {
                    outline.push((
                        Target {
                            path: self.editor().path.clone().unwrap_or_default(),
                            row: *row,
                            col: 0,
                            label: title.clone(),
                            detail: String::new(),
                        },
                        *level,
                    ));
                }
            } else {
                for doc in &self.documents {
                    if doc.editor.format != Format::Latex {
                        continue;
                    }
                    let path = doc.editor.path.clone().unwrap_or_default();
                    if !self.source_cache.iter().any(|s| s.path == path)
                        && doc.id != self.documents[self.active].id
                    {
                        continue;
                    }
                    outline.retain(|(target, _)| target.path != path);
                    for (row, level, title) in doc.editor.outline() {
                        outline.push((
                            Target {
                                path: path.clone(),
                                row: *row,
                                col: 0,
                                label: title.clone(),
                                detail: String::new(),
                            },
                            *level,
                        ));
                    }
                }
            }
            if outline.is_empty() {
                ui.label("No hay secciones en este documento.");
            }
            let mut jump = None;
            // Solo se dibujan las filas visibles.
            ScrollArea::vertical().id_salt("outline").show_rows(
                ui,
                list_row_height(ui),
                outline.len(),
                |ui, rows| {
                    // Filas de una línea: todas miden lo mismo.
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    for (target, level) in &outline[rows] {
                        if ui
                            .selectable_label(
                                self.editor().cursor.row == target.row
                                    && self.editor().path.as_ref() == Some(&target.path),
                                format!("{}{}", "  ".repeat(level.saturating_sub(2)), target.label),
                            )
                            .on_hover_text(format!("{}:{}", target.path.display(), target.row + 1))
                            .clicked()
                        {
                            jump = Some(target.clone());
                        }
                    }
                },
            );
            if let Some(target) = jump {
                if target.path.as_os_str().is_empty() {
                    self.editor_mut().goto(target.row, 0);
                    self.sync_cursor = true;
                    self.focus_editor = true;
                } else {
                    self.jump(&target);
                }
            }
        } else {
            // Una sola fila: el nombre a la izquierda y las acciones a la derecha,
            // para que la barra estrecha no las apile.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("…", |ui| {
                        if action(
                            ui,
                            "Abrir proyecto…",
                            true,
                            "Elige otra carpeta de proyecto.",
                        )
                        .clicked()
                        {
                            self.folder_dialog();
                            ui.close();
                        }
                        if action(
                            ui,
                            "Añadir archivos…",
                            true,
                            "Copia archivos existentes al proyecto sin sobrescribirlos.",
                        )
                        .clicked()
                        {
                            self.add_files();
                            ui.close();
                        }
                        if action(
                            ui,
                            "Actualizar lista",
                            true,
                            "Vuelve a leer los nombres de archivos y las referencias del proyecto.",
                        )
                        .clicked()
                        {
                            self.files = project_files(&self.project);
                            self.refresh_sources();
                            ui.close();
                        }
                    })
                    .response
                    .on_hover_text("Más acciones del proyecto.");
                    if action(
                        ui,
                        "+",
                        true,
                        "Crea un archivo nuevo dentro de este proyecto.",
                    )
                    .clicked()
                    {
                        self.new_file("");
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        ui.label(
                            RichText::new(
                                self.project
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                            .color(col(self.theme.muted())),
                        )
                        .on_hover_text(self.project.display().to_string());
                    });
                });
            });
            ui.add(
                TextEdit::singleline(&mut self.file_query)
                    .hint_text("Filtrar archivos")
                    .desired_width(f32::INFINITY),
            );
            self.file_tree(ui);
        }
    }
}
