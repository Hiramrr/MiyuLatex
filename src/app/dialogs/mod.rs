//! Ventanas de diálogo de la aplicación.

use super::*;

mod bib;
mod equation;
mod insert;
mod palette;
mod project;
mod settings;

pub(super) use bib::Citation;
pub(super) use equation::Equation;
pub(super) use insert::Table;
pub(super) use palette::Palette;
pub(super) use project::{History, ProjectSearch, RenameLabel};

impl App {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.project_search_dialog(ctx);
        self.history_dialog(ctx);
        self.bib_report_dialog(ctx);
        self.citation_dialog(ctx);
        self.equation_dialog(ctx);
        self.rename_label_dialog(ctx);
        self.table_dialog(ctx);
        self.word_count_dialog(ctx);
        self.project_options_dialog(ctx);
        self.settings_dialog(ctx);
        self.templates_dialog(ctx);
        self.symbols_dialog(ctx);
        self.goto_dialog(ctx);
        self.help_dialog(ctx);
        self.palette_dialog(ctx);
        self.close_dialog(ctx);
    }
    pub(super) fn word_count_dialog(&mut self, ctx: &egui::Context) {
        if self.word_count.is_some() {
            let mut open = true;
            egui::Window::new("Conteo de palabras")
                .open(&mut open)
                .default_width(500.0)
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.label(self.word_count.as_deref().unwrap_or_default());
                });
            if !open {
                self.word_count = None;
            }
        }
    }
    pub(super) fn help_dialog(&mut self, ctx: &egui::Context) {
        if self.help {
            let mut open = true;
            egui::Window::new("Ayuda")
                .open(&mut open)
                .resizable(false)
                .vscroll(true)
                .default_height(560.0)
                .show(ctx, |ui| {
                    ui.label("MiyuLaTeX · LaTeX, Markdown, código, PDF e imágenes");
                    ui.separator();
                    let modifier = if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "Ctrl"
                    };
                    for (key, action) in [
                        ("N", "Nuevo"),
                        ("Shift+N", "Nuevo proyecto"),
                        ("O", "Abrir"),
                        ("S", "Guardar"),
                        ("Shift+S", "Guardar como"),
                        ("R", "Compilar"),
                        ("F", "Buscar y reemplazar"),
                        ("G", "Ir a línea"),
                        ("Shift+P", "Paleta de comandos"),
                        ("Shift+I", "Formatear el documento"),
                        ("Shift+M", "Vista previa de la ecuación del cursor"),
                        ("Shift+O", "Abrir rápido un archivo del proyecto"),
                        ("Shift+F", "Buscar en el proyecto"),
                        ("Shift+J", "Mostrar la línea en el PDF"),
                        ("T", "Insertar símbolo"),
                        ("B", "Negrita"),
                        ("I", "Cursiva"),
                        ("/", "Comentar"),
                        ("D", "Seleccionar la palabra y añadir su siguiente aparición"),
                        ("Alt+↑ / ↓", "Añadir un cursor arriba o abajo (o Alt+clic)"),
                        ("L", "Seleccionar la línea"),
                        ("Shift+D", "Duplicar la línea"),
                        ("Shift+K", "Borrar la línea"),
                        ("Enter", "Línea nueva debajo (con Shift, encima)"),
                        ("Shift+\\", "Ir al corchete emparejado (o Ctrl+M)"),
                        ("\\", "Dividir el editor en dos paneles"),
                        ("Z", "Deshacer"),
                        ("Shift+Z", "Rehacer"),
                        (",", "Preferencias"),
                        ("W", "Cerrar documento"),
                        ("Q", "Salir"),
                    ] {
                        ui.label(format!("{modifier}+{key}   {action}"));
                    }
                    ui.separator();
                    ui.label("Alt+↑ y Alt+↓ mueven la línea. Tab y Mayús+Tab cambian la sangría.");
                    ui.label("Copiar o cortar sin selección toman la línea entera.");
                    ui.label("F5 compila. Tab acepta una sugerencia.");
                    ui.label("F8 y Mayús+F8 recorren los problemas de la compilación.");
                    ui.label(format!("F12 o {modifier}+clic llevan a la etiqueta, la cita o el archivo señalado."));
                    ui.label("F2, F3 y F4 muestran u ocultan paneles.");
                    ui.label("F9 pliega o despliega el bloque del cursor; Mayús+F9 despliega todo.");
                    ui.label("Markdown tiene vista previa y esquema de títulos.");
                    ui.label("PDF e imágenes se abren en pestañas de solo lectura.");
                    ui.label("En el PDF, Buscar resalta el texto y arrastrar lo selecciona para copiarlo.");
                    if action(
                        ui,
                        "Cerrar ayuda",
                        true,
                        "Cierra la ayuda y vuelve al documento.",
                    )
                    .clicked()
                    {
                        ui.close_kind(egui::UiKind::Window);
                    }
                });
            self.help = open;
        }
    }
    pub(super) fn close_dialog(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending {
            let mut choice = 0;
            let (save_label, discard_label) = match pending {
                Pending::Quit => ("Guardar y salir", "Salir sin guardar"),
                Pending::Close(_) => ("Guardar y cerrar", "Cerrar sin guardar"),
            };
            egui::Modal::new(Id::new("unsaved")).show(ctx, |ui| {
                ui.heading("Hay cambios sin guardar");
                match pending {
                    Pending::Close(i) => { ui.label(format!("Se cerrará {}. Cerrar sin guardar pierde sus cambios.", self.documents[i].editor.title())); }
                    Pending::Quit => { ui.label("Se cerrará la aplicación. Salir sin guardar pierde los cambios de los documentos abiertos."); }
                }
                ui.horizontal_wrapped(|ui| {
                    if action(ui, save_label, true, "Guarda los cambios y completa el cierre. Si el guardado falla o se cancela, el documento sigue abierto.").clicked() { choice = 1; }
                    if action(ui, discard_label, true, "Cierra y descarta los cambios sin guardar.").clicked() { choice = 2; }
                    if action(ui, "Cancelar", true, "Cancela el cierre y conserva los documentos abiertos.").clicked() { choice = 3; }
                });
            });
            match choice {
                1 => {
                    let saved = match pending {
                        Pending::Quit => self.save_all(),
                        Pending::Close(i) => self.save_document(i, false),
                    };
                    if saved {
                        self.finish_close(pending, ctx);
                    }
                }
                2 => self.finish_close(pending, ctx),
                3 => self.pending = None,
                _ => {}
            }
        }
    }
}
