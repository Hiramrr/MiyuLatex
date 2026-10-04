//! Diálogos que crean documentos o insertan texto.

use super::*;

/// Ajustes de la ventana Insertar tabla; se conservan entre aperturas.
pub(in crate::app) struct Table {
    pub(in crate::app) open: bool,
    rows: usize,
    columns: usize,
    alignment: char,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            open: false,
            rows: 3,
            columns: 3,
            alignment: 'l',
        }
    }
}

impl App {
    pub(super) fn table_dialog(&mut self, ctx: &egui::Context) {
        if self.table.open {
            let mut open = true;
            let mut insert = false;
            egui::Window::new("Insertar tabla")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Filas");
                        ui.add(egui::DragValue::new(&mut self.table.rows).range(1..=100));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Columnas");
                        ui.add(egui::DragValue::new(&mut self.table.columns).range(1..=20));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Alineación");
                        for (alignment, label) in
                            [('l', "Izquierda"), ('c', "Centro"), ('r', "Derecha")]
                        {
                            ui.selectable_value(&mut self.table.alignment, alignment, label);
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        insert = action(
                            ui,
                            "Insertar tabla",
                            self.editor().format == Format::Latex,
                            "Inserta la tabla en el documento LaTeX activo.",
                        )
                        .clicked();
                        if action(ui, "Cancelar", true, "Cierra sin insertar una tabla.").clicked()
                        {
                            ui.close_kind(egui::UiKind::Window);
                        }
                    });
                });
            self.table.open = open;
            if insert && self.editor().format == Format::Latex {
                self.insert_snippet(&latex::table(
                    self.table.rows,
                    self.table.columns,
                    self.table.alignment,
                ));
                self.table.open = false;
            }
        }
    }
    pub(super) fn templates_dialog(&mut self, ctx: &egui::Context) {
        if self.templates {
            let mut open = true;
            // Nombre por omisión y contenido del archivo elegido.
            let mut chosen: Option<(&str, &str)> = None;
            let mut name = std::mem::take(&mut self.workspace.name);
            // Con la extensión ya escrita solo se ofrecen los formatos que la usan.
            let extension = Path::new(name.trim())
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase());
            let typed = extension.is_some();
            let fits = |default: &str| {
                extension
                    .as_deref()
                    .is_none_or(|e| Path::new(default).extension().is_some_and(|d| d == e))
            };
            let blank: Vec<_> = [
                ("LaTeX", "sin-titulo.tex"),
                ("Bibliografía", "referencias.bib"),
                ("Markdown", "sin-titulo.md"),
                ("Texto", "sin-titulo.txt"),
            ]
            .iter()
            .chain(workspace::CODE_FILES)
            .filter(|(_, default)| fits(default))
            .collect();
            egui::Window::new("Nuevo documento")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .vscroll(true)
                .default_height(560.0)
                .default_width(420.0)
                .show(ctx, |ui| {
                    let label = ui.label("Nombre");
                    let response = ui
                        .add(
                            TextEdit::singleline(&mut name)
                                .hint_text("nombre o carpeta/nombre")
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    if std::mem::take(&mut self.workspace.focus_name) {
                        response.request_focus();
                    }
                    if response.changed() {
                        self.workspace.name_error.clear();
                    }
                    ui.label(
                        RichText::new(format!(
                            "Se guarda en {}. Elige un formato; sin nombre se usa el del formato.",
                            self.project.display()
                        ))
                        .size(12.0)
                        .color(col(self.theme.muted())),
                    );
                    if !self.workspace.name_error.is_empty() {
                        ui.colored_label(col(self.theme.error), &self.workspace.name_error);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if action(ui, "Crear archivo", typed, "Crea el archivo con la extensión que escribiste. Enter.").clicked()
                            || typed && response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                        {
                            chosen = Some(("", ""));
                        }
                        if action(ui, "Cancelar", true, "Cierra sin crear un documento.").clicked() { ui.close_kind(egui::UiKind::Window); }
                    });
                    if !blank.is_empty() {
                        ui.separator();
                        ui.label("Documento vacío");
                        ui.horizontal_wrapped(|ui| {
                            for (label, default) in &blank {
                                if action(ui, label, true, "Crea un archivo vacío de este formato en el proyecto.").clicked() { chosen = Some((default, "")); }
                            }
                        });
                    }
                    if fits("sin-titulo.tex") {
                        ui.separator();
                        ui.label("Plantillas LaTeX");
                        for template in &catalog().templates {
                            if ui
                                .button(format!("Crear {}", template.title))
                                .on_hover_text(&template.description)
                                .clicked()
                            {
                                chosen = Some((&template.filename, &template.text));
                            }
                            ui.label(
                                RichText::new(&template.description)
                                    .size(13.0)
                                    .color(col(self.theme.muted())),
                            );
                        }
                    }
                });
            self.templates = open;
            if let Some((default, text)) = chosen {
                match self.create_file(&name, default, text) {
                    Ok(()) => self.templates = false,
                    Err(e) => self.workspace.name_error = e,
                }
            }
            self.workspace.name = name;
        }
    }
    pub(super) fn symbols_dialog(&mut self, ctx: &egui::Context) {
        if self.symbols {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Símbolos LaTeX")
                .open(&mut open)
                .default_size([420.0, 440.0])
                .show(ctx, |ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.symbol_query)
                            .hint_text("Buscar símbolo o comando")
                            .desired_width(f32::INFINITY),
                    );
                    if action(
                        ui,
                        "Cerrar símbolos",
                        true,
                        "Cierra sin insertar un símbolo.",
                    )
                    .clicked()
                    {
                        ui.close_kind(egui::UiKind::Window);
                    }
                    let query = self.symbol_query.to_lowercase();
                    ScrollArea::vertical().show(ui, |ui| {
                        for symbol in &catalog().symbols {
                            if !format!("{} {} {}", symbol.name, symbol.latex, symbol.group)
                                .to_lowercase()
                                .contains(&query)
                            {
                                continue;
                            }
                            if ui
                                .add_enabled(
                                    self.editor().format == Format::Latex,
                                    egui::Button::new(format!(
                                        "Insertar {}  {}  {}",
                                        symbol.char, symbol.latex, symbol.name
                                    )),
                                )
                                .on_hover_text(
                                    "Inserta este comando en el cursor del documento LaTeX activo.",
                                )
                                .clicked()
                            {
                                chosen = Some(symbol.latex.clone());
                            }
                        }
                    });
                });
            self.symbols = open;
            if let Some(text) = chosen.filter(|_| self.editor().format == Format::Latex) {
                self.editor_mut().insert(&text);
                self.changed_editor();
                self.symbols = false;
            }
        }
    }
    pub(super) fn goto_dialog(&mut self, ctx: &egui::Context) {
        if self.goto {
            let mut open = true;
            let mut chosen = false;
            egui::Window::new("Ir a línea")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    let count = self.editor().lines.len().max(1);
                    ui.add(egui::DragValue::new(&mut self.line).range(1..=count));
                    ui.label("Número de línea");
                    ui.horizontal_wrapped(|ui| {
                        chosen = action(
                            ui,
                            "Ir a línea",
                            self.editor().format.editable(),
                            "Mueve el cursor a esta línea del documento activo.",
                        )
                        .clicked();
                        if action(ui, "Cancelar", true, "Cierra sin mover el cursor.").clicked() {
                            ui.close_kind(egui::UiKind::Window);
                        }
                    });
                });
            self.goto = open;
            if chosen {
                let line = self.line;
                self.editor_mut().goto(line.saturating_sub(1), 0);
                self.sync_cursor = true;
                self.focus_editor = true;
                self.goto = false;
            }
        }
    }
}
