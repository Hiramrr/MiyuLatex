use super::*;
use crate::editor::Pos;

fn tick(app: &mut App, ctx: &egui::Context, events: Vec<egui::Event>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 820.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        app.draw(ui);
    });
    assert!(!output.shapes.is_empty());
    output.textures_delta.clear();
}
fn key(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn markdown_code_pdf_images_and_binary_protection() {
    let folder = std::env::temp_dir().join(format!("miyu-multi-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap();
    let md_path = folder.join("README.md");
    let source = "# Nota ñ\n\n**Negrita** y *cursiva*.\n\n- [ ] tarea\n\n```rust\nfn main() {}\n```\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n![Imagen](imagen.png)\n\n[Texto](texto.txt)\n";
    fs::write(&md_path, source).unwrap();
    fs::write(folder.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(folder.join("texto.txt"), "texto ñ\n").unwrap();
    fs::write(folder.join("binario.bin"), [0, 1, 2, 3]).unwrap();
    fs::write(folder.join("bad.pdf"), "no es un PDF").unwrap();
    fs::write(folder.join("empty.pdf"), "").unwrap();
    image::RgbImage::new(8, 8)
        .save(folder.join("imagen.png"))
        .unwrap();
    let mut data = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
    ]
    .iter()
    .enumerate()
    {
        offsets.push(data.len());
        data.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = data.len();
    data.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in &offsets[1..] {
        data.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    data.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    let pdf_path = folder.join("documento.PDF");
    fs::write(&pdf_path, &data).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(folder.clone()), &ctx).unwrap();
    app.config.autocompile = true;
    app.config.show_preview = true;
    app.config.auto_pairs = true;
    app.backdrop = Backdrop::default();
    assert_eq!(app.editor().format, Format::Markdown);
    assert_eq!(app.editor().outline(), [(0, 2, "Nota ñ".into())]);
    assert!(app.files.contains(&pdf_path));
    assert!(app.files.contains(&folder.join("main.rs")));
    assert!(app.files.contains(&folder.join("imagen.png")));
    assert!(!app.files.contains(&folder.join("binario.bin")));
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![key(Key::B, Modifiers::COMMAND)]);
    assert!(app.editor().text().starts_with("****# Nota ñ"));
    assert!(app.edited_at.is_none());
    assert!(app.compile_rx.is_none());
    assert!(app.save_document(0, false));
    let saved_md = fs::read_to_string(&md_path).unwrap();
    assert!(app.open(&folder.join("binario.bin")).is_err());
    assert!(app.open(&folder.join("empty.pdf")).is_err());
    assert_eq!(app.documents.len(), 1);
    app.open(&folder.join("main.rs")).unwrap();
    assert_eq!(app.editor().format.label(), "Rust");
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![key(Key::B, Modifiers::COMMAND)]);
    assert_eq!(app.editor().text(), "fn main() {}\n");
    tick(&mut app, &ctx, vec![key(Key::Slash, Modifiers::COMMAND)]);
    assert_eq!(app.editor().text(), "// fn main() {}\n");
    assert!(app.edited_at.is_none());
    app.open(&pdf_path).unwrap();
    let pdf_index = app.active;
    app.open(&pdf_path).unwrap();
    assert_eq!(app.active, pdf_index);
    tick(&mut app, &ctx, vec![key(Key::S, Modifiers::COMMAND)]);
    tick(&mut app, &ctx, vec![key(Key::F, Modifiers::COMMAND)]);
    tick(&mut app, &ctx, vec![egui::Event::Text("no cambiar".into())]);
    app.compile(false, &ctx);
    assert!(!app.find);
    assert!(!app.editor().dirty());
    assert!(app.compile_rx.is_none());
    assert_eq!(fs::read(&pdf_path).unwrap(), data);
    let started = Instant::now();
    while app.pdf().loading && started.elapsed() < Duration::from_secs(10) {
        tick(&mut app, &ctx, vec![]);
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.pdf().count, 2, "{}", app.pdf().error);
    assert!(app.pdf().rendered() > 0);
    app.pdf_mut().change_page(1);
    app.pdf_mut().change_zoom(1);
    app.activate(0);
    tick(&mut app, &ctx, vec![]);
    assert_eq!(app.editor().format, Format::Markdown);
    assert_eq!(app.editor().text(), saved_md);
    app.activate(pdf_index);
    assert_eq!(app.pdf().page, 1);
    assert_eq!(app.pdf().zoom, 125.0);
    tick(&mut app, &ctx, vec![]);
    app.open(&folder.join("imagen.png")).unwrap();
    tick(&mut app, &ctx, vec![]);
    assert_eq!(app.editor().format, Format::Image);
    assert!(!app.save_document(app.active, false));
    app.request_close(Pending::Close(app.active), &ctx);
    assert!(app.pending.is_none());
    app.open(&folder.join("bad.pdf")).unwrap();
    let started = Instant::now();
    while app.pdf().loading && started.elapsed() < Duration::from_secs(10) {
        tick(&mut app, &ctx, vec![]);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!app.pdf().error.is_empty());
    assert_eq!(app.pdf().rendered(), 0);
    app.templates = true;
    tick(&mut app, &ctx, vec![]);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn native_editor_save_compile_preview_and_close() {
    let folder = std::env::temp_dir().join(format!("miyu-gui-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("main.tex");
    let source = "\\documentclass{article}\n\\begin{document}\nHola\n\\end{document}\n";
    fs::write(&path, source).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path.clone()), &ctx).unwrap();
    app.config.autocompile = false;
    app.backdrop
        .load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icon.png"
        )))
        .unwrap();
    app.config.background_palette = false;
    app.apply_theme(&ctx);
    app.editor_mut().goto(2, 4);
    tick(&mut app, &ctx, vec![]);
    assert_eq!(app.background_texture.as_ref().unwrap().size(), [640, 410]);
    assert!(app.panel_fill().a() < 255);
    tick(&mut app, &ctx, vec![egui::Event::Text(" ñ".into())]);
    assert_eq!(app.editor().lines[2], "Hola ñ");
    assert_eq!(app.editor().cursor, Pos::new(2, 6));
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().lines[2], "Hola");
    tick(
        &mut app,
        &ctx,
        vec![key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    assert_eq!(app.editor().lines[2], "Hola ñ");
    assert!(app.save_document(0, false));
    assert!(fs::read_to_string(&path).unwrap().contains("Hola ñ"));
    tick(&mut app, &ctx, vec![egui::Event::Text("{".into())]);
    assert_eq!(app.editor().lines[2], "Hola ñ{}");
    tick(&mut app, &ctx, vec![key(Key::Backspace, Modifiers::NONE)]);
    assert_eq!(app.editor().lines[2], "Hola ñ");
    tick(&mut app, &ctx, vec![egui::Event::Text(" \\sect".into())]);
    assert!(!app.editor().completions.is_empty());
    tick(&mut app, &ctx, vec![key(Key::Tab, Modifiers::NONE)]);
    assert!(app.editor().lines[2].contains("\\section{}"));
    app.editor_mut().search("Hola");
    app.editor_mut().find_next(false);
    assert_eq!(app.editor().selected(), "Hola");
    app.editor_mut().insert("Adiós");
    app.changed_editor();
    app.request_close(Pending::Close(0), &ctx);
    assert!(app.pending.is_some());
    tick(&mut app, &ctx, vec![]);
    app.pending = None;
    fs::write(&path, "cambio externo").unwrap();
    assert!(!app.save_document(0, false));
    assert_eq!(fs::read_to_string(&path).unwrap(), "cambio externo");
    fs::write(&path, source).unwrap();
    app.documents[0].editor = Editor::new(source.into(), Some(path.clone()));
    app.sync_cursor = true;
    if compiler::which("tectonic").is_some() {
        app.config.engine = "tectonic".into();
        app.compile(false, &ctx);
        assert!(app.compile_rx.is_some());
        let started = Instant::now();
        while (app.compile_rx.is_some() || app.preview.loading)
            && started.elapsed() < Duration::from_secs(60)
        {
            app.poll(&ctx);
            thread::sleep(Duration::from_millis(25));
        }
        let result = app.result.as_ref().expect("resultado del motor");
        assert!(result.ok, "{}", result.output);
        assert_eq!(app.preview.count, 1);
        let started = Instant::now();
        while app.preview.rendered() == 0 && started.elapsed() < Duration::from_secs(10) {
            tick(&mut app, &ctx, vec![]);
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(app.preview.rendered(), 1);
    }
    tick(&mut app, &ctx, vec![]);
    let galley = app.documents[0].layout.galley.clone().unwrap();
    assert_eq!(galley.text(), app.editor().source());
    app.finish_close(Pending::Close(0), &ctx);
    assert_eq!(app.documents.len(), 1);
    assert!(!app.editor().dirty());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn code_shortcuts_completion_and_search() {
    let folder = std::env::temp_dir().join(format!("miyu-code-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("main.rs");
    let source = "fn main() {\n    let total = 1;\n    let other = total;\n}\n";
    fs::write(&path, source).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.config.autocompile = false;
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![]);
    let place = |app: &mut App, row, col| {
        app.editor_mut().goto(row, col);
        app.sync_cursor = true;
        tick(app, &ctx, vec![]);
    };
    let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
    place(&mut app, 1, 4);
    tick(&mut app, &ctx, vec![key(Key::ArrowDown, Modifiers::ALT)]);
    assert_eq!(app.editor().lines[2], "    let total = 1;");
    assert_eq!(app.editor().cursor, Pos::new(2, 4));
    tick(&mut app, &ctx, vec![key(Key::ArrowUp, Modifiers::ALT)]);
    assert_eq!(app.editor().text(), source);
    tick(&mut app, &ctx, vec![key(Key::D, shifted)]);
    assert_eq!(app.editor().lines[1], app.editor().lines[2]);
    assert_eq!(app.editor().cursor, Pos::new(2, 4));
    tick(&mut app, &ctx, vec![key(Key::K, shifted)]);
    assert_eq!(app.editor().text(), source);
    tick(&mut app, &ctx, vec![key(Key::Tab, Modifiers::SHIFT)]);
    assert_eq!(app.editor().lines[2], "let other = total;");
    assert!(ctx.memory(|m| m.has_focus(app.documents[app.active].id)));
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().text(), source);
    // Sin selección, cortar se lleva la línea entera.
    tick(&mut app, &ctx, vec![egui::Event::Cut]);
    assert_eq!(app.editor().lines[2], "}");
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    // Las letras seguidas completan con palabras del documento y se deshacen juntas.
    place(&mut app, 1, 18);
    tick(&mut app, &ctx, vec![key(Key::Enter, Modifiers::COMMAND)]);
    assert_eq!(app.editor().cursor, Pos::new(2, 4));
    for letter in ["o", "t", "h"] {
        tick(&mut app, &ctx, vec![egui::Event::Text(letter.into())]);
    }
    assert_eq!(app.editor().completions[0].label, "other");
    let cursor = app.editor().cursor;
    tick(&mut app, &ctx, vec![key(Key::ArrowDown, Modifiers::NONE)]);
    tick(&mut app, &ctx, vec![key(Key::ArrowUp, Modifiers::NONE)]);
    assert_eq!(app.editor().cursor, cursor);
    tick(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(app.editor().completions.is_empty());
    assert!(ctx.memory(|m| m.has_focus(app.documents[app.active].id)));
    tick(&mut app, &ctx, vec![key(Key::Space, Modifiers::CTRL)]);
    tick(&mut app, &ctx, vec![key(Key::Tab, Modifiers::NONE)]);
    assert_eq!(app.editor().lines[2], "    other");
    assert!(app.editor().completions.is_empty());
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().lines[2], "    oth");
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().lines[2], "    ");
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().text(), source);
    // Cmd+D selecciona la palabra, Cmd+F la busca y Enter recorre las coincidencias.
    place(&mut app, 1, 10);
    tick(&mut app, &ctx, vec![key(Key::D, Modifiers::COMMAND)]);
    assert_eq!(app.editor().selected(), "total");
    tick(&mut app, &ctx, vec![key(Key::F, Modifiers::COMMAND)]);
    assert!(app.find);
    assert_eq!(app.query, "total");
    tick(&mut app, &ctx, vec![]);
    assert_eq!(app.editor().matches.len(), 2);
    assert_eq!(marks::current_match(app.editor()), Some(1));
    tick(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
    assert_eq!(marks::current_match(app.editor()), Some(2));
    assert_eq!(app.editor().text(), source);
    let find_focus = ctx.memory(|m| m.focused());
    assert_ne!(find_focus, Some(app.documents[app.active].id));
    tick(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
    assert_eq!(marks::current_match(app.editor()), Some(1));
    assert_eq!(ctx.memory(|m| m.focused()), find_focus);
    tick(&mut app, &ctx, vec![key(Key::Enter, Modifiers::SHIFT)]);
    assert_eq!(marks::current_match(app.editor()), Some(2));
    tick(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!app.find);
    assert!(!app.editor().dirty());
    fs::remove_dir_all(folder).unwrap();
}
#[test]
fn buttons_keep_their_purpose_across_documents() {
    fn frame(
        app: &mut App,
        ctx: &egui::Context,
        width: f32,
        events: Vec<egui::Event>,
    ) -> Vec<egui::accesskit::Node> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 820.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.textures_delta.clear();
        output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .map(|(_, node)| node)
            .collect()
    }
    fn button(app: &mut App, ctx: &egui::Context, label: &str) -> egui::accesskit::Node {
        frame(app, ctx, 1280.0, vec![]);
        let nodes = frame(app, ctx, 1280.0, vec![]);
        nodes
            .iter()
            .find(|node| {
                node.label() == Some(label) && node.role() == egui::accesskit::Role::Button
            })
            .unwrap_or_else(|| {
                panic!(
                    "No aparece el botón {label}: {:?}",
                    nodes
                        .iter()
                        .filter_map(|node| node.label())
                        .collect::<Vec<_>>()
                )
            })
            .clone()
    }
    fn click(app: &mut App, ctx: &egui::Context, label: &str) {
        let node = button(app, ctx, label);
        assert!(!node.is_disabled(), "{label} está desactivado");
        let rect = node.bounds().unwrap();
        let pos = egui::pos2(
            ((rect.x0 + rect.x1) / 2.0) as f32,
            ((rect.y0 + rect.y1) / 2.0) as f32,
        );
        frame(app, ctx, 1280.0, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                1280.0,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }
    let folder = std::env::temp_dir().join(format!("miyu-buttons-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap();
    let tex = folder.join("main.tex");
    let md = folder.join("nota.md");
    let code = folder.join("main.rs");
    fs::write(
        &tex,
        "\\documentclass{article}\n\\begin{document}\nHola\n\\end{document}\n",
    )
    .unwrap();
    fs::write(&md, "casa casa\n").unwrap();
    fs::write(&code, "fn main() {}\n").unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::new(Some(tex.clone()), &ctx).unwrap();
    app.config.autocompile = false;
    app.config.autosave = false;
    app.config.show_preview = false;
    app.config.show_sidebar = false;
    app.backdrop = Backdrop::default();

    // The compact toggle exposes its state and project actions stay together.
    click(&mut app, &ctx, "Mostrar panel lateral");
    assert!(app.config.show_sidebar);
    click(&mut app, &ctx, "Ocultar panel lateral");
    assert!(!app.config.show_sidebar);
    click(&mut app, &ctx, "Proyecto");
    assert!(!button(&mut app, &ctx, "Nuevo proyecto…").is_disabled());
    assert!(!button(&mut app, &ctx, "Abrir carpeta de proyecto…").is_disabled());
    assert!(!button(&mut app, &ctx, "Importar proyecto ZIP…").is_disabled());
    click(&mut app, &ctx, "Proyecto");

    // Project settings and app preferences open different windows.
    click(&mut app, &ctx, "LaTeX");
    click(&mut app, &ctx, "Configurar proyecto LaTeX…");
    assert!(app.project_options && !app.settings);
    click(&mut app, &ctx, "Cerrar configuración");
    assert!(!app.project_options);
    click(&mut app, &ctx, "Preferencias");
    assert!(app.settings && !app.project_options);
    click(&mut app, &ctx, "Cerrar preferencias");
    assert!(!app.settings);

    // Cancelling document creation preserves the current tabs.
    let documents = app.documents.len();
    click(&mut app, &ctx, "Nuevo documento…");
    assert!(app.templates);
    click(&mut app, &ctx, "Cancelar");
    assert!(!app.templates);
    assert_eq!(app.documents.len(), documents);

    // Each tab's close button targets that tab, including an inactive one.
    app.open(&md).unwrap();
    let md_index = app.active;
    app.editor_mut().goto(0, 0);
    app.editor_mut().insert("nuevo ");
    app.changed_editor();
    click(&mut app, &ctx, "Cerrar main.tex");
    assert_eq!(app.documents.len(), 1);
    assert_eq!(app.editor().path.as_ref(), Some(&md));
    assert_eq!(md_index, 1);
    click(&mut app, &ctx, "Guardar");
    assert!(fs::read_to_string(&md).unwrap().starts_with("nuevo "));

    // Search includes unsaved Markdown and code. A single replacement changes one match.
    app.editor_mut().goto(0, 0);
    app.editor_mut().insert("pendiente ");
    app.changed_editor();
    app.search.query = "pendiente".into();
    app.search_project();
    assert_eq!(app.search.results.len(), 1);
    assert_eq!(app.search.results[0].path, md);
    click(&mut app, &ctx, "Editar");
    click(&mut app, &ctx, "Buscar y reemplazar…");
    app.query = "casa".into();
    app.replacement = "hogar".into();
    assert!(button(&mut app, &ctx, "Reemplazar coincidencia").is_disabled());
    click(&mut app, &ctx, "Siguiente");
    assert_eq!(app.editor().selected(), "casa");
    click(&mut app, &ctx, "Reemplazar coincidencia");
    assert_eq!(app.editor().text().matches("casa").count(), 1);
    assert_eq!(app.editor().text().matches("hogar").count(), 1);
    click(&mut app, &ctx, "Reemplazar todas");
    assert!(!app.editor().text().contains("casa"));
    click(&mut app, &ctx, "Cerrar búsqueda");
    click(&mut app, &ctx, "Editar");
    click(&mut app, &ctx, "Deshacer");
    assert_eq!(app.editor().text().matches("casa").count(), 1);
    app.open(&code).unwrap();
    app.editor_mut().goto(0, 0);
    app.editor_mut().insert("// pendiente\n");
    app.search.query = "pendiente".into();
    app.search_project();
    assert_eq!(app.search.results.len(), 2);

    // Stopping a compile remains available from a code tab.
    let (_sender, receiver) = mpsc::channel();
    app.compile_rx = Some(receiver);
    app.cancel = Arc::new(AtomicBool::new(false));
    click(&mut app, &ctx, "Detener compilación");
    assert!(app.cancel.load(Ordering::Relaxed));
    app.compile_rx = None;
    app.preview.path = Some(folder.join("otro.pdf"));
    assert!(app.pdf_path().is_none());
    click(&mut app, &ctx, "Archivo");
    assert!(button(&mut app, &ctx, "Exportar PDF…").is_disabled());
    click(&mut app, &ctx, "Archivo");

    // Auxiliary cleanup works from a bibliography source and never runs during compilation.
    app.open(&tex).unwrap();
    fs::write(tex.with_extension("aux"), "temporal").unwrap();
    let (_sender, receiver) = mpsc::channel();
    app.compile_rx = Some(receiver);
    assert!(!app.clean_aux());
    assert!(tex.with_extension("aux").exists());
    app.compile_rx = None;
    assert!(app.clean_aux());
    assert!(!tex.with_extension("aux").exists());
    fs::write(folder.join("referencias.bib"), "").unwrap();
    app.open(&folder.join("referencias.bib")).unwrap();
    fs::write(tex.with_extension("aux"), "temporal").unwrap();
    assert!(app.clean_aux());
    assert!(!tex.with_extension("aux").exists());

    // Toolbar actions remain within a small window even with larger UI text.
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    app.config.ui_font_size = 22.0;
    app.apply_theme(&ctx);
    frame(&mut app, &ctx, 480.0, vec![]);
    let nodes = frame(&mut app, &ctx, 480.0, vec![]);
    for label in [
        "Nuevo documento…",
        "Abrir archivo…",
        "Guardar",
        "Proyecto",
        "Compilar",
        "Preferencias",
        "Ayuda",
    ] {
        let node = nodes
            .iter()
            .find(|n| n.label() == Some(label) && n.role() == egui::accesskit::Role::Button)
            .unwrap();
        let bounds = node.bounds().unwrap();
        assert!(
            bounds.x0 >= 0.0 && bounds.x1 <= 480.0,
            "{label}: {bounds:?}"
        );
    }
    fs::remove_dir_all(folder).unwrap();
}

/// Tiempos por cuadro con documentos grandes, en reposo y tecleando:
/// `cargo test --release rendimiento -- --ignored --nocapture`
#[test]
#[ignore]
fn rendimiento_documentos_grandes() {
    let folder = std::env::temp_dir().join(format!("miyu-perf-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let tex: String = (0..2500)
        .map(|i| {
            format!(
                "\\section{{Sección {i}}}\\label{{sec:{i}}}\nEl resultado de \\textbf{{la medición}} número {i} se resume en $x_{{{i}}}^2 + \\alpha$, como muestra \\cite{{ref{i}}} y explica la figura~\\ref{{fig:{i}}} con todo detalle para el lector atento que llega hasta aquí.\n\\begin{{equation}}\n    a_{{{i}}} = \\frac{{1}}{{2}} \\sum_k b_k % comentario\n\\end{{equation}}\n\n"
            )
        })
        .collect();
    let md: String = (0..2500)
        .map(|i| {
            format!(
                "## Sección {i}\n\nUn párrafo con **negrita**, *cursiva* y `código` número {i}.\n\n- [ ] tarea {i}\n\n"
            )
        })
        .collect();
    let rs: String = (0..2500)
        .map(|i| {
            format!(
                "/// Función {i}.\nfn f{i}(x: usize) -> String {{\n    let y = x * {i} + 1; // nota\n    format!(\"valor {{y}}\")\n}}\n\n"
            )
        })
        .collect();
    for (name, source) in [("grande.tex", tex), ("grande.md", md), ("grande.rs", rs)] {
        let path = folder.join(name);
        fs::write(&path, &source).unwrap();
        let ctx = egui::Context::default();
        let started = Instant::now();
        let mut app = App::new(Some(path), &ctx).unwrap();
        app.config.autocompile = false;
        app.backdrop = Backdrop::default();
        let row = app.editor().lines.len() / 2;
        app.editor_mut().goto(row, 0);
        tick(&mut app, &ctx, vec![]);
        let open = started.elapsed();
        let time = |app: &mut App, events: &dyn Fn(usize) -> Vec<egui::Event>| {
            let mut frames: Vec<_> = (0..40)
                .map(|i| {
                    let started = Instant::now();
                    tick(app, &ctx, events(i));
                    started.elapsed()
                })
                .collect();
            frames.sort();
            (frames[frames.len() / 2], frames[frames.len() - 1])
        };
        for _ in 0..200 {
            tick(&mut app, &ctx, vec![]);
        }
        let idle = time(&mut app, &|_| vec![]);
        let typing = time(&mut app, &|i| {
            vec![egui::Event::Text(
                ((b'a' + (i % 26) as u8) as char).to_string(),
            )]
        });
        let enter = time(&mut app, &|_| vec![key(Key::Enter, Modifiers::NONE)]);
        let command = time(&mut app, &|i| {
            vec![egui::Event::Text(["\\", "s", "e", "c", " "][i % 5].into())]
        });
        println!(
            "{name}: {} líneas, {} KiB · abrir {open:.1?} · reposo {:.2?} (máx {:.2?}) · tecla {:.2?} (máx {:.2?}) · intro {:.2?} (máx {:.2?}) · comando {:.2?} (máx {:.2?})",
            app.editor().lines.len(),
            source.len() / 1024,
            idle.0,
            idle.1,
            typing.0,
            typing.1,
            enter.0,
            enter.1,
            command.0,
            command.1,
        );
    }
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn panels_follow_the_chosen_side() {
    let folder = std::env::temp_dir().join(format!("miyu-sides-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("nota.md");
    fs::write(&path, "# Nota\n").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.config.autosave = false;
    app.config.show_sidebar = true;
    app.config.show_preview = true;
    app.backdrop = Backdrop::default();
    let center = |ctx: &egui::Context, id: &'static str| {
        egui::containers::panel::PanelState::load(ctx, egui::Id::new(id))
            .unwrap()
            .outer_rect
            .center()
            .x
    };
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![]);
    assert!(center(&ctx, "files") < 640.0);
    assert!(center(&ctx, "markdown_preview") > 640.0);

    app.config.sidebar_right = true;
    app.config.preview_left = true;
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![]);
    assert!(center(&ctx, "files") > 640.0);
    assert!(center(&ctx, "markdown_preview") < 640.0);

    // En el mismo lado, el panel de archivos queda en el borde de la ventana.
    app.config.preview_left = false;
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![]);
    assert!(center(&ctx, "files") > center(&ctx, "markdown_preview"));
    assert!(center(&ctx, "markdown_preview") > 640.0);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn clicking_below_the_text_moves_the_cursor_to_the_end() {
    let folder = std::env::temp_dir().join(format!("miyu-click-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("main.rs");
    fs::write(&path, "fn main() {}").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.config.autosave = false;
    app.config.show_preview = false;
    app.config.show_sidebar = false;
    app.backdrop = Backdrop::default();
    let frame = |app: &mut App, events: Vec<egui::Event>| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 820.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.textures_delta.clear();
    };
    frame(&mut app, vec![]);
    frame(&mut app, vec![]);
    assert_eq!(app.editor().cursor.col, 0);
    // Muy por debajo de la única línea del documento.
    let pos = egui::pos2(700.0, 600.0);
    frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
    for pressed in [true, false] {
        frame(
            &mut app,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    frame(&mut app, vec![]);
    assert_eq!((app.editor().cursor.row, app.editor().cursor.col), (0, 12));
    assert!(ctx.memory(|m| m.has_focus(app.documents[app.active].id)));
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn history_window_opens_only_for_saved_latex() {
    let folder = std::env::temp_dir().join(format!("miyu-history-gui-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("main.tex");
    fs::write(&path, "nuevo").unwrap();
    latex::checkpoint(&path, "viejo").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.show_history();
    tick(&mut app, &ctx, vec![]);
    assert!(app.history.is_some());
    app.new_document(0);
    app.history = None;
    app.show_history();
    assert!(app.history.is_none());
    assert!(!app.message.is_empty());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn definition_problems_and_command_palette() {
    let folder = std::env::temp_dir().join(format!("miyu-navegar-{}", std::process::id()));
    fs::create_dir_all(folder.join("cap")).unwrap();
    let folder = folder.canonicalize().unwrap();
    let main = folder.join("main.tex");
    let chapter = folder.join("cap/uno.tex");
    let bib = folder.join("refs.bib");
    fs::write(
        &main,
        "\\documentclass{article}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\input{cap/uno}\nVer \\ref{sec:uno} y \\cite{knuth}.\n\\ref{nada}\n\\end{document}\n",
    )
    .unwrap();
    fs::write(&chapter, "Texto\n  \\section{Uno}\\label{sec:uno}\n").unwrap();
    fs::write(
        &bib,
        "@book{otro,\n title = {A}\n}\n@book{knuth,\n title = {B}\n}\n",
    )
    .unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(main.clone()), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);

    // De la referencia a su etiqueta en otro archivo.
    app.goto_definition(Pos::new(4, 10));
    assert_eq!(app.editor().path.as_ref(), Some(&chapter));
    assert_eq!(app.editor().cursor, Pos::new(1, 15));
    // De la cita a su entrada de bibliografía.
    app.open(&main).unwrap();
    app.goto_definition(Pos::new(4, 27));
    assert_eq!(app.editor().path.as_ref(), Some(&bib));
    assert_eq!(app.editor().cursor.row, 3);
    // De \input al archivo; F12 usa la posición del cursor.
    app.open(&main).unwrap();
    app.editor_mut().goto(3, 9);
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![key(Key::F12, Modifiers::NONE)]);
    assert_eq!(app.editor().path.as_ref(), Some(&chapter));
    // Una etiqueta que no existe deja un aviso y no cambia de archivo.
    app.open(&main).unwrap();
    app.goto_definition(Pos::new(5, 6));
    assert_eq!(app.editor().path.as_ref(), Some(&main));
    assert!(app.message.contains("nada"));

    // Los problemas con línea se pintan en el editor y se recorren con F8.
    for (path, row, error) in [(&main, 4, false), (&main, 1, true), (&chapter, 0, true)] {
        app.diagnostics.push(Diagnostic {
            path: path.clone(),
            row,
            error,
            message: format!("problema {row}"),
        });
    }
    app.editor_mut().goto(0, 0);
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![key(Key::F8, Modifiers::NONE)]);
    assert_eq!(app.editor().cursor.row, 1);
    assert_eq!(app.message, "problema 1");
    tick(&mut app, &ctx, vec![key(Key::F8, Modifiers::NONE)]);
    assert_eq!(app.editor().cursor.row, 4);
    tick(&mut app, &ctx, vec![key(Key::F8, Modifiers::NONE)]);
    assert_eq!(app.editor().cursor.row, 1);
    tick(&mut app, &ctx, vec![key(Key::F8, Modifiers::SHIFT)]);
    assert_eq!(app.editor().cursor.row, 4);

    // La paleta encuentra acciones sin tildes y las ejecuta con Enter.
    tick(
        &mut app,
        &ctx,
        vec![key(Key::P, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    assert!(app.palette.open);
    assert!(!app.settings);
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![egui::Event::Text("simbolo".into())]);
    assert_eq!(app.palette.query, "simbolo");
    assert_eq!(app.palette_matches()[0].0, "Insertar símbolo LaTeX…");
    tick(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(!app.palette.open);
    assert!(app.symbols);
    // Una acción no disponible queda al final y no se ejecuta.
    app.symbols = false;
    app.open_palette();
    app.palette.query = "detener".into();
    let found = app.palette_matches();
    assert_eq!((found[0].0, found[0].2), ("Detener compilación", false));
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn renames_labels_and_inserts_dropped_images() {
    let folder = std::env::temp_dir().join(format!("miyu-etiquetas-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!("miyu-fuera-{}", std::process::id()));
    fs::create_dir_all(folder.join("cap")).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let folder = folder.canonicalize().unwrap();
    let main = folder.join("main.tex");
    let chapter = folder.join("cap/uno.tex");
    fs::write(
        &main,
        "\\documentclass{article}\n\\begin{document}\n\\input{cap/uno}\nVer \\ref{sec:uno} y \\ref{otra}.\n\\label{otra}\n\\end{document}\n",
    )
    .unwrap();
    fs::write(
        &chapter,
        "\\section{Uno}\\label{sec:uno}\n\\eqref{sec:uno}\n",
    )
    .unwrap();
    let photo = outside.join("mi foto.png");
    image::RgbImage::new(4, 4).save(&photo).unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(main.clone()), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);

    // Fuera de una etiqueta no se abre la ventana.
    app.editor_mut().goto(0, 3);
    app.start_rename_label();
    assert!(app.rename_label.is_none());
    app.editor_mut().goto(3, 10);
    app.start_rename_label();
    tick(&mut app, &ctx, vec![]);
    assert_eq!(app.rename_label.as_ref().unwrap().new, "sec:uno");
    // Un nombre que ya existe o con caracteres inválidos se rechaza.
    assert!(app.apply_rename_label("sec:uno", "otra").is_err());
    assert!(app.apply_rename_label("sec:uno", "con espacio").is_err());
    assert_eq!(
        fs::read_to_string(&chapter)
            .unwrap()
            .matches("sec:uno")
            .count(),
        2
    );
    app.apply_rename_label("sec:uno", "sec:primera").unwrap();
    // El documento abierto cambia en el editor y se puede deshacer.
    assert!(
        app.editor()
            .text()
            .contains("\\ref{sec:primera} y \\ref{otra}")
    );
    assert!(app.editor().dirty());
    assert_eq!(app.editor().cursor, Pos::new(3, 10));
    assert!(
        fs::read_to_string(&main)
            .unwrap()
            .contains("\\ref{sec:uno}")
    );
    // El archivo cerrado se reescribe y guarda la versión anterior.
    assert_eq!(
        fs::read_to_string(&chapter).unwrap(),
        "\\section{Uno}\\label{sec:primera}\n\\eqref{sec:primera}\n"
    );
    assert_eq!(latex::versions(&chapter).len(), 1);
    assert!(app.message.contains("3 apariciones en 2 archivos"));

    // Una imagen de fuera del proyecto se copia a images/ y entra como figura.
    assert!(app.accepts_image(&photo));
    assert!(!app.accepts_image(&outside.join("datos.csv")));
    app.editor_mut().goto(4, 12);
    app.insert_image(&photo);
    assert!(folder.join("images/figura-1.png").is_file());
    assert!(
        app.editor()
            .text()
            .contains("\\includegraphics[width=0.8\\linewidth]{images/figura-1.png}")
    );
    assert!(app.message.contains("graphicx"));
    // En Markdown entra como imagen; la que ya está en la carpeta no se copia.
    let notes = folder.join("notas.md");
    fs::write(&notes, "# Notas\n").unwrap();
    app.open(&notes).unwrap();
    assert!(app.accepts_image(&folder.join("images/figura-1.png")));
    app.editor_mut().goto(1, 0);
    app.insert_image(&folder.join("images/figura-1.png"));
    // El cursor queda en el texto alternativo, listo para escribirlo.
    assert_eq!(app.editor().cursor, Pos::new(1, 2));
    let end = app.editor().end();
    app.editor_mut().goto(end.row, end.col);
    app.insert_image(&photo);
    assert_eq!(
        app.editor().text(),
        "# Notas\n![](images/figura-1.png)![](images/figura-2.png)"
    );
    // Una imagen pegada se guarda como PNG junto a las demás.
    let end = app.editor().end();
    app.editor_mut().goto(end.row, end.col);
    app.insert_pasted(&image::RgbaImage::new(3, 2));
    assert!(app.editor().text().ends_with("![](images/figura-3.png)"));
    assert_eq!(
        image::image_dimensions(folder.join("images/figura-3.png")).unwrap(),
        (3, 2)
    );
    // En código, soltar una imagen la abre como siempre.
    fs::write(folder.join("main.rs"), "fn main() {}\n").unwrap();
    app.open(&folder.join("main.rs")).unwrap();
    assert!(!app.accepts_image(&photo));
    fs::remove_dir_all(folder).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn bibliography_report_and_downloaded_citations() {
    let folder = std::env::temp_dir().join(format!("miyu-biblio-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap();
    let main = folder.join("main.tex");
    let bib = folder.join("refs.bib");
    fs::write(&main, "\\documentclass{article}\n\\addbibresource{refs.bib}\n\\begin{document}\n\\cite{uno}\n\n\\end{document}\n").unwrap();
    fs::write(&bib, "@book{uno,\n  title = {T}\n}\n").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(main.clone()), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);
    app.check_bibliography();
    tick(&mut app, &ctx, vec![]);
    let report = app.bib_report.as_ref().unwrap();
    assert_eq!(report.len(), 1);
    assert!(report[0].label.contains("no tiene author, publisher, year"));

    // La entrada va al disco si el .bib está cerrado, con \\cite en el cursor.
    app.open_citation();
    assert!(app.citation.insert);
    tick(&mut app, &ctx, vec![]);
    app.editor_mut().goto(4, 0);
    let entry = "@article{Backus_1978,\n  title={T},\n  year={1978}\n}";
    app.add_citation(entry).unwrap();
    assert_eq!(
        fs::read_to_string(&bib).unwrap(),
        format!("@book{{uno,\n  title = {{T}}\n}}\n\n{entry}\n")
    );
    assert_eq!(app.editor().lines[4], "\\cite{Backus_1978}");
    assert!(!app.citation.open);
    assert!(app.message.contains("Añadida «Backus_1978» a refs.bib"));
    // Repetirla no la duplica.
    app.add_citation(entry).unwrap();
    assert_eq!(
        fs::read_to_string(&bib)
            .unwrap()
            .matches("Backus_1978")
            .count(),
        1
    );
    assert!(app.message.contains("ya estaba"));
    // Con el .bib abierto, la entrada se añade en el editor y queda sin guardar.
    app.open(&bib).unwrap();
    app.add_citation("@misc{otra,\n  title={O}\n}").unwrap();
    assert!(
        app.editor()
            .text()
            .ends_with("}\n\n@misc{otra,\n  title={O}\n}\n")
    );
    assert!(app.editor().dirty());
    assert!(!fs::read_to_string(&bib).unwrap().contains("otra"));
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn formats_the_document_with_the_language_tool() {
    if compiler::which("rustfmt").is_none() {
        return;
    }
    let folder = std::env::temp_dir().join(format!("miyu-formatear-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("main.rs");
    fs::write(&path, "fn main(){let x=1;}\n").unwrap();
    fs::write(folder.join("notas.txt"), "hola\n").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);
    tick(
        &mut app,
        &ctx,
        vec![key(Key::I, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    let started = Instant::now();
    while app.tool_rx.is_some() && started.elapsed() < Duration::from_secs(20) {
        tick(&mut app, &ctx, vec![]);
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(app.editor().text(), "fn main() {\n    let x = 1;\n}\n");
    assert!(app.editor().dirty());
    // Se deshace en un solo paso.
    app.editor_mut().undo(false);
    assert_eq!(app.editor().text(), "fn main(){let x=1;}\n");
    // Sin formateador conocido no se lanza nada.
    app.open(&folder.join("notas.txt")).unwrap();
    assert!(app.formatter_name().is_none());
    app.format_document(&ctx);
    assert!(app.tool_rx.is_none());
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn previews_the_equation_under_the_cursor() {
    if compiler::engines().is_empty() {
        return;
    }
    let folder = std::env::temp_dir().join(format!("miyu-ecuacion-gui-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let main = folder.join("main.tex");
    fs::write(
        &main,
        "\\documentclass{article}\n\\newcommand{\\R}{\\mathbb{R}}\n\\usepackage{amssymb}\n\\begin{document}\nSea $x \\in \\R$ y $\\noexiste$.\n\\end{document}\n",
    )
    .unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(main), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    tick(&mut app, &ctx, vec![]);
    let wait = |app: &mut App| {
        let started = Instant::now();
        tick(app, &ctx, vec![]);
        while !app.equation.ready() && started.elapsed() < Duration::from_secs(60) {
            thread::sleep(Duration::from_millis(20));
            tick(app, &ctx, vec![]);
        }
    };
    // Fuera de una fórmula no se compila nada.
    tick(
        &mut app,
        &ctx,
        vec![key(Key::M, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    assert!(app.equation.open);
    wait(&mut app);
    assert!(app.equation.size().is_none());
    // La macro del preámbulo vale dentro de la fórmula.
    app.editor_mut().goto(4, 6);
    app.sync_cursor = true;
    wait(&mut app);
    assert!(app.equation.error.is_empty(), "{}", app.equation.error);
    let size = app.equation.size().unwrap();
    assert!(size.x > 20.0 && size.y > 8.0, "{size:?}");
    // Una fórmula que no compila deja el motivo y conserva la imagen anterior.
    app.editor_mut().goto(4, 20);
    app.sync_cursor = true;
    wait(&mut app);
    assert!(!app.equation.error.is_empty());
    assert!(app.equation.size().is_some());
    tick(
        &mut app,
        &ctx,
        vec![key(Key::M, Modifiers::COMMAND | Modifiers::SHIFT)],
    );
    assert!(!app.equation.open);
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn types_with_several_cursors() {
    let folder = std::env::temp_dir().join(format!("miyu-cursores-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("lista.txt");
    fs::write(&path, "uno\ndos\ntres").unwrap();
    let ctx = egui::Context::default();
    let mut app = App::new(Some(path), &ctx).unwrap();
    app.backdrop = Backdrop::default();
    app.config.completions = false;
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![]);
    let both = Modifiers::COMMAND | Modifiers::ALT;
    tick(&mut app, &ctx, vec![key(Key::ArrowDown, both)]);
    tick(&mut app, &ctx, vec![key(Key::ArrowDown, both)]);
    assert_eq!(app.editor().extras().len(), 2);
    // Las letras y el borrado llegan a los tres cursores.
    tick(&mut app, &ctx, vec![egui::Event::Text("- ".into())]);
    assert_eq!(app.editor().text(), "- uno\n- dos\n- tres");
    tick(&mut app, &ctx, vec![key(Key::Backspace, Modifiers::NONE)]);
    tick(&mut app, &ctx, vec![key(Key::End, Modifiers::NONE)]);
    tick(&mut app, &ctx, vec![egui::Event::Text(";".into())]);
    assert_eq!(app.editor().text(), "-uno;\n-dos;\n-tres;");
    assert!(app.editor().dirty());
    // Esc deja un solo cursor, que sigue donde estaba el principal.
    tick(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(app.editor().extras().is_empty());
    tick(&mut app, &ctx, vec![]);
    tick(&mut app, &ctx, vec![egui::Event::Text("!".into())]);
    assert_eq!(app.editor().text(), "-uno;\n-dos;\n-tres;!");
    // Deshacer devuelve el texto anterior a cada edición múltiple.
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    tick(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(app.editor().text(), "-uno\n-dos\n-tres");
    fs::remove_dir_all(folder).unwrap();
}
