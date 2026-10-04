//! Barra superior con menús y acciones rápidas.

use super::*;

impl App {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        if self.has_native_menu() {
            self.titlebar(ui);
            return;
        }
        let ctx = ui.ctx().clone();
        let editable = self.editor().format.editable();
        let latex = self.editor().format == Format::Latex;
        let saved_source = self.root().is_some();
        let compiling = self.compile_rx.is_some();
        let tool_ready = self.tool_rx.is_none();
        let has_pdf = self.pdf_path().is_some();
        egui::Panel::top("toolbar")
            .frame(egui::Frame::side_top_panel(ui.style()).fill(col(self.theme.surface)).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
            ui.spacing_mut().interact_size.y = 26.0;
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            egui::MenuBar::new().style(|style: &mut egui::Style| {
                egui::containers::menu::menu_style(style);
                style.spacing.button_padding = egui::vec2(8.0, 2.0);
                style.spacing.interact_size.y = 26.0;
            }).ui(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                ui.menu_button("Archivo", |ui| {
                    if action(ui, "Nuevo documento…", true, "Crea un archivo en el proyecto. Cmd/Ctrl+N.").clicked() {
                        self.new_file("");
                        ui.close();
                    }
                    if action(ui, "Abrir archivo…", true, "Abre texto, código, un PDF o una imagen. Cmd/Ctrl+O.").clicked() {
                        self.open_dialog();
                        ui.close();
                    }
                    if action(ui, "Abrir archivo del proyecto…", !self.files.is_empty(), "Busca por nombre en los archivos del proyecto. Cmd/Ctrl+Mayús+O. Requiere archivos en el proyecto.").clicked() {
                        self.open_quick();
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Guardar", editable, "Guarda el documento activo. Cmd/Ctrl+S. PDF e imágenes son de solo lectura.").clicked() {
                        self.save_document(self.active, false);
                        ui.close();
                    }
                    if action(ui, "Guardar como…", editable, "Guarda el documento activo con otro nombre o ubicación. Cmd/Ctrl+Mayús+S. Requiere un documento editable.").clicked() {
                        self.save_document(self.active, true);
                        ui.close();
                    }
                    let unsaved = self.documents.iter().any(|d| d.editor.format.editable() && (d.editor.dirty() || d.editor.path.is_none()));
                    if action(ui, "Guardar todos", unsaved, "Guarda los documentos abiertos que tienen cambios o aún no tienen nombre.").clicked() {
                        self.save_all();
                        ui.close();
                    }
                    if action(ui, "Historial del archivo LaTeX…", saved_source, "Consulta y restaura versiones guardadas del archivo LaTeX activo. Requiere guardarlo primero.").clicked() {
                        self.show_history();
                        ui.close();
                    }
                    if action(ui, "Cerrar documento", true, "Cierra la pestaña activa y pregunta si tiene cambios sin guardar. Cmd/Ctrl+W.").clicked() {
                        self.request_close(Pending::Close(self.active), &ctx);
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Exportar PDF…", has_pdf, "Guarda una copia del PDF del documento activo. Abre un PDF o compila un documento LaTeX primero.").clicked() {
                        self.export_pdf();
                        ui.close();
                    }
                    ui.menu_button("Exportar", |ui| {
                        if action(ui, "Markdown a HTML…", self.can_export_markdown(), "Guarda el documento Markdown activo como una página HTML autónoma. Las imágenes relativas se incrustan si la guardas fuera de su carpeta.").clicked() {
                            self.export_html(&ctx);
                            ui.close();
                        }
                        if action(ui, "Markdown a PDF…", self.can_export_markdown(), "Convierte el documento Markdown activo a LaTeX y lo compila con el motor instalado, sin cambiar el documento.").clicked() {
                            self.export_markdown_pdf(&ctx);
                            ui.close();
                        }
                        if action(ui, "Markdown a EPUB…", self.can_export_epub(), export::EPUB_HELP).clicked() {
                            self.export_epub(&ctx);
                            ui.close();
                        }
                        ui.separator();
                        if action(ui, "Crear presentación a partir de este documento", self.can_make_presentation(), "Crea junto a este documento LaTeX guardado un .tex Beamer con una diapositiva por sección, sin tocar el original.").clicked() {
                            self.create_presentation();
                            ui.close();
                        }
                    });
                    ui.separator();
                    if action(ui, "Salir", true, "Cierra la aplicación y pregunta si hay cambios sin guardar. Cmd/Ctrl+Q.").clicked() {
                        self.request_close(Pending::Quit, &ctx);
                        ui.close();
                    }
                });
                ui.menu_button("Proyecto", |ui| {
                    if action(ui, "Nuevo proyecto…", true, "Crea una carpeta de proyecto vacía, con código o con una plantilla. Cmd/Ctrl+Mayús+N.").clicked() {
                        self.new_project();
                        ui.close();
                    }
                    if action(ui, "Abrir carpeta de proyecto…", true, "Elige una carpeta existente y muestra sus archivos.").clicked() {
                        self.folder_dialog();
                        ui.close();
                    }
                    self.recent_menu(ui);
                    if action(ui, "Crear archivo en el proyecto…", true, "Crea y abre un archivo dentro de la carpeta del proyecto.").clicked() {
                        self.new_file("");
                        ui.close();
                    }
                    if action(ui, "Añadir archivos al proyecto…", true, "Copia archivos existentes al proyecto sin sobrescribir archivos con el mismo nombre.").clicked() {
                        self.add_files();
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Importar proyecto ZIP…", tool_ready, "Extrae un ZIP en una carpeta nueva y abre el proyecto. Espera si hay otra operación en curso.").clicked() {
                        self.import_project(&ctx);
                        ui.close();
                    }
                    if action(ui, "Exportar proyecto ZIP…", tool_ready, "Guarda los cambios y copia el proyecto a un ZIP. Espera si hay otra operación en curso.").clicked() {
                        self.export_project(&ctx);
                        ui.close();
                    }
                });
                ui.menu_button("Editar", |ui| {
                    if action(ui, "Deshacer", editable && self.editor().can_undo(), "Deshace el último cambio del documento. Cmd/Ctrl+Z. Requiere un cambio que deshacer.").clicked() {
                        self.editor_mut().undo(false);
                        self.changed_editor();
                        ui.close();
                    }
                    if action(ui, "Rehacer", editable && self.editor().can_redo(), "Recupera el último cambio deshecho. Cmd/Ctrl+Mayús+Z. Requiere un cambio que rehacer.").clicked() {
                        self.editor_mut().undo(true);
                        self.changed_editor();
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Buscar y reemplazar…", editable, "Busca texto en el documento activo. Cmd/Ctrl+F. Requiere un documento editable.").clicked() {
                        self.start_find();
                        ui.close();
                    }
                    if action(ui, "Ir a línea…", editable, "Lleva el cursor al número de línea que elijas. Cmd/Ctrl+G. Requiere un documento editable.").clicked() {
                        self.start_goto();
                        ui.close();
                    }
                    if action(ui, "Buscar en el proyecto…", !self.files.is_empty(), "Busca texto en los archivos del proyecto, incluidos los cambios abiertos sin guardar. Cmd/Ctrl+Mayús+F.").clicked() {
                        self.search.open = true;
                        self.search_project();
                        ui.close();
                    }
                    if action(ui, "Formatear documento", self.formatter_name().is_some() && tool_ready, "Ordena la sangría y el estilo con la herramienta del lenguaje: latexindent, rustfmt, gofmt, ruff, clang-format o prettier. Cmd/Ctrl+Mayús+I. Requiere tenerla instalada.").clicked() {
                        self.format_document(&ctx);
                        ui.close();
                    }
                    if action(ui, "Ir a la definición", latex || self.has_language_server(), "Lleva a la etiqueta, la entrada de bibliografía o el archivo del comando bajo el cursor; con un servidor de lenguaje, a la definición del símbolo. F12 o Cmd/Ctrl+clic. Disponible en LaTeX o con servidor.").clicked() {
                        self.goto_definition(self.editor().cursor);
                        ui.close();
                    }
                    if action(ui, "Renombrar etiqueta LaTeX…", latex, "Cambia la etiqueta bajo el cursor en su \\label y en todas sus referencias del proyecto. Disponible en LaTeX.").clicked() {
                        self.start_rename_label();
                        ui.close();
                    }
                    if action(ui, "Problema siguiente", self.all_diagnostics().next().is_some(), "Lleva el cursor al siguiente problema de la última compilación. F8; con Mayús, al anterior.").clicked() {
                        self.next_problem(false);
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Paleta de comandos…", true, "Busca cualquier acción por su nombre. Cmd/Ctrl+Mayús+P.").clicked() {
                        self.open_palette();
                        ui.close();
                    }
                    if action(ui, "Comentar o descomentar líneas", editable && self.editor().format.comment().is_some(), "Alterna los comentarios de las líneas seleccionadas según el lenguaje. Cmd/Ctrl+/. Requiere un lenguaje con comentarios.").clicked() {
                        self.editor_mut().rewrite_lines(true, false);
                        self.changed_editor();
                        ui.close();
                    }
                });
                ui.menu_button("Ver", |ui| {
                    let mut changed = ui.checkbox(&mut self.config.show_sidebar, "Panel de archivos, esquema y referencias").on_hover_text("Muestra u oculta el panel lateral. F2.").changed();
                    if action(ui, "Git: cambios, commit, ramas e historial", true, "Abre la pestaña Git del panel lateral. Cmd/Ctrl+Mayús+G.").clicked() {
                        self.show_git(&ctx);
                        ui.close();
                    }
                    changed |= ui.checkbox(&mut self.config.show_preview, "Vista previa").on_hover_text("Muestra el PDF de LaTeX o la vista previa de Markdown. F3.").changed();
                    changed |= ui.checkbox(&mut self.config.soft_wrap, "Ajustar líneas al ancho del editor").changed();
                    if ui.checkbox(&mut self.config.typewriter, "Cursor centrado (máquina de escribir)").on_hover_text("Mantiene la línea del cursor en el centro del editor al escribir y al mover el cursor con el teclado. Si te desplazas con la rueda, no vuelve hasta que escribas o muevas el cursor.").changed() {
                        self.sync_cursor = true;
                        changed = true;
                    }
                    if ui.selectable_label(self.zen, "Modo sin distracciones").on_hover_text("Oculta los paneles, la vista previa, la barra de herramientas y las mascotas, y deja el texto en una columna centrada. Cmd/Ctrl+Mayús+E; Esc para salir.").clicked() {
                        self.toggle_zen();
                        ui.close();
                    }
                    if action(ui, "Fijar meta de palabras…", editable, "Elige cuántas palabras quieres escribir en esta sesión y muestra el progreso en la barra de estado. Requiere un documento editable.").clicked() {
                        self.open_goal_dialog();
                        ui.close();
                    }
                    let mut split = self.split.is_some();
                    if ui.checkbox(&mut split, "Dividir el editor en dos paneles").on_hover_text("Muestra dos documentos uno junto al otro. Un clic en un panel lo activa y las pestañas cambian el documento del panel activo. Cmd/Ctrl+\\.").changed() {
                        self.toggle_split();
                        ui.close();
                    }
                    if action(ui, "Plegar o desplegar el bloque del cursor", editable, "Oculta la sección, el entorno o el bloque de código del cursor bajo su primera línea, o lo vuelve a mostrar. F9. También con el triángulo del margen.").clicked() {
                        self.toggle_fold_at_cursor();
                        ui.close();
                    }
                    if action(ui, "Plegar todo", editable, "Pliega las secciones de primer nivel o los bloques de código de primer nivel.").clicked() {
                        self.fold_everything(true);
                        ui.close();
                    }
                    if action(ui, "Desplegar todo", editable && self.editor().has_folds(), "Muestra todo lo plegado. Mayús+F9.").clicked() {
                        self.fold_everything(false);
                        ui.close();
                    }
                    if ui.selectable_label(self.panel && !self.developer.selected, "Problemas y registro de compilación").on_hover_text("Muestra u oculta los resultados de la última compilación. F4.").clicked() {
                        self.toggle_problems();
                    }
                    changed |= ui.checkbox(&mut self.config.mascot, "Gatito en la barra de estado").on_hover_text("Muestra u oculta la mascota. Teclea en su portátil mientras escribes, espera la compilación y se duerme si no hay actividad.").changed();
                    changed |= ui.add_enabled(self.config.mascot, egui::Checkbox::new(&mut self.config.mascot_friend, "Cangrejito amigo del gatito")).on_hover_text("Un cangrejito que pasea por la barra de estado y va a saludar al gatito.").changed();
                    changed |= ui.add_enabled(self.config.mascot, egui::Checkbox::new(&mut self.config.mascot_dog, "Schnauzer amigo del gatito")).on_hover_text("Un schnauzer que pasea por la barra de estado, menea la cola y ladra si la compilación falla.").changed();
                    changed |= ui.add_enabled(self.config.mascot, egui::Checkbox::new(&mut self.config.mascot_jasmine, "Jazmín en su maceta")).on_hover_text("Un jazmín en la esquina de la barra de estado. Abre sus flores de noche, mientras el gatito duerme, y al compilar bien.").changed();
                    ui.separator();
                    ui.menu_button("Posición del panel de archivos", |ui| {
                        changed |= ui.radio_value(&mut self.config.sidebar_right, false, "Izquierda").changed();
                        changed |= ui.radio_value(&mut self.config.sidebar_right, true, "Derecha").changed();
                    });
                    ui.menu_button("Posición de la vista previa", |ui| {
                        changed |= ui.radio_value(&mut self.config.preview_left, true, "Izquierda").changed();
                        changed |= ui.radio_value(&mut self.config.preview_left, false, "Derecha").changed();
                    });
                    if changed { self.preferences_changed(&ctx); }
                });
                ui.menu_button("Desarrollo", |ui| self.developer_menu(ui));
                ui.menu_button("Insertar", |ui| {
                    let prose = latex || self.editor().format == Format::Markdown;
                    if action(ui, "Negrita", prose, "Aplica negrita al texto seleccionado o inserta sus marcas. Cmd/Ctrl+B. Disponible en LaTeX y Markdown.").clicked() {
                        self.editor_mut().emphasize(true);
                        self.changed_editor();
                        ui.close();
                    }
                    if action(ui, "Cursiva", prose, "Aplica cursiva al texto seleccionado o inserta sus marcas. Cmd/Ctrl+I. Disponible en LaTeX y Markdown.").clicked() {
                        self.editor_mut().emphasize(false);
                        self.changed_editor();
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Símbolo LaTeX…", latex, "Elige un símbolo y lo inserta en el cursor. Cmd/Ctrl+T. Disponible en documentos LaTeX.").clicked() {
                        self.symbols = true;
                        ui.close();
                    }
                    if action(ui, "Tabla LaTeX…", latex, "Elige filas, columnas y alineación antes de insertar la tabla. Disponible en documentos LaTeX.").clicked() {
                        self.table.open = true;
                        ui.close();
                    }
                    if action(ui, "Figura LaTeX…", latex && saved_source, "Elige una imagen e inserta una figura con pie y etiqueta. Guarda el documento LaTeX primero.").clicked() {
                        self.insert_figure();
                        ui.close();
                    }
                    if action(ui, "Pegar imagen del portapapeles", self.accepts_image(Path::new("x.png")), "Guarda la imagen copiada, por ejemplo una captura de pantalla, en images/ y la inserta en el cursor. También con Cmd/Ctrl+V si el portapapeles no tiene texto. Requiere un documento LaTeX o Markdown guardado.").clicked() {
                        self.paste_image();
                        ui.close();
                    }
                    if action(ui, "Cita o referencia LaTeX…", latex, "Abre las etiquetas y la bibliografía del proyecto para insertar una referencia o una cita. Disponible en LaTeX.").clicked() {
                        self.references = true;
                        self.outline = false;
                        self.config.show_sidebar = true;
                        ui.close();
                    }
                    if action(ui, "Cita por DOI o arXiv…", saved_source, "Descarga la entrada BibTeX de un DOI o de un artículo de arXiv y la añade a la bibliografía del proyecto. Requiere un archivo LaTeX guardado.").clicked() {
                        self.open_citation();
                        ui.close();
                    }
                    ui.add_enabled_ui(latex, |ui| {
                        for (label, before, after) in [
                            ("Matemática en línea", "\\(", "\\)"),
                            ("Ecuación centrada", "\\[\n", "\n\\]"),
                            ("Sección", "\\section{", "}"),
                            ("Subsección", "\\subsection{", "}"),
                            ("Subrayado", "\\underline{", "}"),
                        ] {
                            if action(ui, label, true, "Inserta las marcas LaTeX alrededor de la selección o en el cursor.").clicked() {
                                self.editor_mut().wrap(before, after);
                                self.changed_editor();
                                ui.close();
                            }
                        }
                        ui.menu_button("Entorno LaTeX", |ui| {
                            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                for (name, body) in &catalog().environments {
                                    if action(ui, name, true, "Inserta el inicio, el contenido y el cierre de este entorno.").clicked() {
                                        let args = catalog().env_args.get(name).map_or("", String::as_str);
                                        self.insert_snippet(&format!("\\begin{{{name}}}{args}\n    {}\n\\end{{{name}}}", body.replace('\n', "\n    ")));
                                        ui.close();
                                    }
                                }
                            });
                        });
                    });
                });
                ui.menu_button("LaTeX", |ui| {
                    if action(ui, "Compilar", !compiling && (latex || saved_source), "Guarda los archivos LaTeX del documento y genera su PDF. F5 o Cmd/Ctrl+R. Requiere un documento LaTeX y ninguna compilación en curso.").clicked() {
                        self.compile(false, &ctx);
                        ui.close();
                    }
                    if action(ui, "Detener compilación", compiling && !self.cancel.load(Ordering::Relaxed), "Detiene la compilación en curso, aunque hayas cambiado de pestaña.").clicked() {
                        self.cancel.store(true, Ordering::Relaxed);
                        self.message = "Deteniendo la compilación…".into();
                        ui.close();
                    }
                    if action(ui, "Recompilar desde cero", !compiling && saved_source, "Elimina los archivos auxiliares y genera de nuevo el PDF. Requiere un archivo LaTeX guardado y ninguna compilación en curso.").clicked() {
                        if self.clean_aux() { self.compile(false, &ctx); }
                        ui.close();
                    }
                    if action(ui, "Limpiar archivos auxiliares", !compiling && saved_source, "Elimina solo los archivos temporales de LaTeX. Conserva los archivos originales y el PDF. Requiere un archivo LaTeX guardado y ninguna compilación en curso.").clicked() {
                        self.clean_aux();
                        ui.close();
                    }
                    ui.separator();
                    if action(ui, "Configurar proyecto LaTeX…", true, "Elige el archivo principal y el motor para la carpeta de proyecto actual.").clicked() {
                        self.project_options = true;
                        ui.close();
                    }
                    if ui.checkbox(&mut self.config.autocompile, "Compilar al dejar de escribir").on_hover_text("Compila automáticamente los documentos LaTeX guardados.").changed() {
                        self.preferences_changed(&ctx);
                    }
                    if ui.checkbox(&mut self.config.autosave, "Guardar LaTeX y bibliografía automáticamente").on_hover_text("Guarda los archivos LaTeX abiertos tras 2 segundos sin escribir.").changed() {
                        self.preferences_changed(&ctx);
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.equation.open, "Vista previa de la ecuación").on_hover_text("Muestra en una ventana la fórmula que rodea al cursor, compilada con el preámbulo del documento. Cmd/Ctrl+Mayús+M.").changed() {
                        self.equation.open = !self.equation.open;
                        self.toggle_equation();
                        ui.close();
                    }
                    if action(ui, "Revisar bibliografía…", saved_source, "Busca claves repetidas, campos obligatorios que faltan y entradas que ningún documento cita. Requiere un archivo LaTeX guardado.").clicked() {
                        self.check_bibliography();
                        ui.close();
                    }
                    if action(ui, "Contar palabras del proyecto…", tool_ready && saved_source, "Cuenta la prosa del proyecto LaTeX. Requiere un archivo LaTeX guardado y ninguna otra operación en curso.").clicked() {
                        self.count_words(&ctx);
                        ui.close();
                    }
                    if action(ui, "Mostrar línea del cursor en PDF", tool_ready && saved_source && has_pdf, "Lleva el PDF a la línea del cursor. Cmd/Ctrl+Mayús+J. Requiere un archivo LaTeX guardado, su PDF y ninguna otra operación en curso.").clicked() {
                        self.sync_to_pdf(&ctx);
                        ui.close();
                    }
                });
                if action(ui, "Preferencias", true, "Configura el editor y la apariencia para todos los proyectos. Cmd/Ctrl+,.").clicked() { self.settings = true; }
                if action(ui, "Ayuda", true, "Consulta las funciones y los atajos de teclado. F1.").clicked() { self.help = true; }
                });
            });
            if !self.zen {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                self.sidebar_toggle(ui);
                ui.separator();
                if toolbar_action(ui, "Nuevo", "Nuevo documento…", true, "Crea un archivo en el proyecto. Cmd/Ctrl+N.").clicked() { self.new_file(""); }
                if toolbar_action(ui, "Abrir", "Abrir archivo…", true, "Abre un documento, código, PDF o imagen. Cmd/Ctrl+O.").clicked() { self.open_dialog(); }
                if toolbar_action(ui, "Guardar", "Guardar", editable, "Guarda la pestaña activa. Cmd/Ctrl+S. PDF e imágenes son de solo lectura.").clicked() { self.save_document(self.active, false); }
                if toolbar_action(ui, "Terminal", "Mostrar u ocultar terminal", true, "Abre la terminal integrada. Ctrl+`.").clicked() { self.toggle_terminal(&ctx); }
                ui.separator();
                if compiling {
                    ui.spinner();
                    if action(ui, "Detener compilación", !self.cancel.load(Ordering::Relaxed), "Detiene la compilación en curso. Espera mientras el motor termina de detenerse.").clicked() {
                        self.cancel.store(true, Ordering::Relaxed);
                        self.message = "Deteniendo la compilación…".into();
                    }
                } else if !latex && self.has_task(developer::TaskKind::Run) {
                    if toolbar_action(ui, "Ejecutar", "Ejecutar código", true, "Guarda y ejecuta código. F5 o Cmd/Ctrl+R.").clicked() {
                        self.run_task_kind(developer::TaskKind::Run, &ctx);
                    }
                } else {
                    let response = ui.add_enabled(latex || saved_source, egui::Button::new("Compilar")
                        .fill(col(theme::mix(self.theme.surface, self.theme.primary, 0.18)))
                        .stroke(Stroke::new(1.0, col(self.theme.primary))))
                        .on_hover_text("Guarda los archivos LaTeX y genera el PDF. F5 o Cmd/Ctrl+R.")
                        .on_disabled_hover_text("Abre o crea un documento LaTeX para compilar su PDF.");
                    if response.clicked() { self.compile(false, &ctx); }
                }
                if self.tool_rx.is_some() { ui.spinner(); }
                if let Some(root) = self.root() {
                    ui.label(RichText::new(format!("Principal: {}", root.file_name().unwrap_or_default().to_string_lossy())).color(col(self.theme.muted())))
                        .on_hover_text(root.display().to_string());
                }
            });
            }
        });
    }

    fn titlebar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("titlebar")
            .frame(
                egui::Frame::new()
                    .fill(col(self.theme.surface))
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                ui.spacing_mut().interact_size.y = 28.0;
                ui.spacing_mut().item_spacing.x = 6.0;
                let drag = ui.interact(
                    egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), list_row_height(ui)),
                    ),
                    Id::new("titlebar_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(
                        !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                    ));
                }
                ui.horizontal(|ui| {
                    // Los botones de macOS siguen siendo nativos, sobre esta fila.
                    if !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false)) {
                        ui.add_space(72.0);
                    }
                    self.sidebar_toggle(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if toolbar_action(
                            ui,
                            "⌘",
                            "Paleta de comandos",
                            true,
                            "Paleta de comandos. Cmd+Mayús+P.",
                        )
                        .clicked()
                        {
                            self.open_palette();
                        }
                        if toolbar_action(
                            ui,
                            "Terminal",
                            "Mostrar u ocultar terminal",
                            true,
                            "Terminal integrada. Ctrl+`.",
                        )
                        .clicked()
                        {
                            self.toggle_terminal(&ctx);
                        }
                        if self.compile_rx.is_some() {
                            if toolbar_action(
                                ui,
                                "Detener",
                                "Detener compilación",
                                !self.cancel.load(Ordering::Relaxed),
                                "Detener compilación",
                            )
                            .clicked()
                            {
                                self.cancel.store(true, Ordering::Relaxed);
                                self.message = "Deteniendo la compilación…".into();
                            }
                            ui.spinner();
                        } else if self.editor().format != Format::Latex
                            && self.has_task(developer::TaskKind::Run)
                        {
                            if toolbar_action(
                                ui,
                                "Ejecutar",
                                "Ejecutar código",
                                true,
                                "Guardar y ejecutar. F5 o Cmd+R.",
                            )
                            .clicked()
                            {
                                self.run_task_kind(developer::TaskKind::Run, &ctx);
                            }
                        } else if (self.editor().format == Format::Latex || self.root().is_some())
                            && toolbar_action(
                                ui,
                                "Compilar",
                                "Compilar",
                                true,
                                "Guardar y compilar. F5 o Cmd+R.",
                            )
                            .clicked()
                        {
                            self.compile(false, &ctx);
                        }
                        if ui.max_rect().width() >= 600.0
                            && self.editor().format.editable()
                            && toolbar_action(ui, "Guardar", "Guardar", true, "Guardar. Cmd+S.")
                                .clicked()
                        {
                            self.save_document(self.active, false);
                        }
                        if self.tool_rx.is_some() {
                            ui.spinner();
                        }
                        let name = self
                            .project
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        ui.add_sized(
                            [ui.available_width(), list_row_height(ui)],
                            egui::Label::new(name.as_ref())
                                .truncate()
                                .halign(egui::Align::Min),
                        )
                        .on_hover_text(self.project.display().to_string());
                    });
                });
            });
    }
}

/// Etiquetas breves en la barra, con el nombre completo para accesibilidad.
pub(super) fn toolbar_action(
    ui: &mut egui::Ui,
    label: &str,
    name: &str,
    enabled: bool,
    help: &str,
) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new(label).frame_when_inactive(false));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), name)
    });
    response.on_hover_text(help).on_disabled_hover_text(help)
}
