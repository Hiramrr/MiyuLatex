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
            let mut chosen = None;
            let mut blank = None;
            egui::Window::new("Nuevo documento")
                .open(&mut open)
                .resizable(false)
                .vscroll(true)
                .default_height(560.0)
                .default_width(420.0)
                .show(ctx, |ui| {
                    if action(ui, "Cancelar", true, "Cierra sin crear un documento.").clicked() { ui.close_kind(egui::UiKind::Window); }
                    ui.label("Documento vacío");
                    ui.horizontal_wrapped(|ui| {
                        for (label, name) in [
                            ("LaTeX", "sin-titulo.tex"),
                            ("Bibliografía", "referencias.bib"),
                            ("Markdown", "sin-titulo.md"),
                            ("Texto", "sin-titulo.txt"),
                        ] {
                            if action(ui, label, true, "Abre un documento vacío de este formato. Elige su ubicación al guardar.").clicked() { blank = Some(name); }
                        }
                        for (language, name) in workspace::CODE_FILES {
                            if action(ui, language, true, "Abre un archivo de código vacío de este lenguaje.").clicked() { blank = Some(*name); }
                        }
                    });
                    ui.separator();
                    ui.label("Plantillas LaTeX");
                    for (i, template) in catalog().templates.iter().enumerate() {
                        if ui
                            .button(format!("Crear {}", template.title))
                            .on_hover_text(&template.description)
                            .clicked()
                        {
                            chosen = Some(i);
                        }
                        ui.label(
                            RichText::new(&template.description)
                                .size(13.0)
                                .color(col(self.theme.muted())),
                        );
                    }
                    ui.separator();

                });
            self.templates = open;
            if let Some(name) = blank {
                self.add_document(Editor::untitled(String::new(), name));
                self.templates = false;
            }
            if let Some(i) = chosen {
                self.new_document(i);
                self.templates = false;
            }
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
