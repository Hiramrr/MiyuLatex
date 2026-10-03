//! Archivos del proyecto, fuentes LaTeX, búsqueda, importación y exportación.

use super::*;

impl App {
    pub(super) fn root(&self) -> Option<PathBuf> {
        let path = self.editor().path.as_ref()?;
        if !latex::is_source(path) {
            return None;
        }
        if path.starts_with(&self.project)
            && let Some(main) = &self.project_settings.main
        {
            return Some(self.project.join(main));
        }
        // Se consulta en cada cuadro desde la barra de herramientas.
        let editor = self.editor();
        let document = self.documents[self.active].id;
        let mut cache = self.root_cache.borrow_mut();
        let cached = cache.as_ref().filter(|c| c.document == document);
        if let Some(cached) = cached.filter(|c| c.revision == editor.revision) {
            return Some(cached.root.clone());
        }
        let signature = compiler::root_signature(editor.source());
        let root = match cached.filter(|c| c.signature == signature) {
            Some(cached) => cached.root.clone(),
            None => compiler::find_root(path, editor.source()),
        };
        *cache = Some(RootCache {
            document,
            revision: editor.revision,
            signature,
            root: root.clone(),
        });
        Some(root)
    }
    pub(super) fn source_overlays(&self) -> Vec<Source> {
        self.documents
            .iter()
            .filter_map(|d| {
                d.editor
                    .path
                    .as_ref()
                    .filter(|path| latex::is_source(path))
                    .map(|path| Source {
                        path: path.clone(),
                        text: d.editor.text(),
                    })
            })
            .collect()
    }
    pub(super) fn refresh_sources(&mut self) {
        // Los archivos del proyecto pueden haber cambiado en disco.
        self.root_cache.take();
        self.source_cache.clear();
        self.source_outline.clear();
        let Some(root) = self.root() else { return };
        self.source_cache = latex::sources(&root, &self.source_overlays());
        for file in &self.files {
            if !self.source_cache.iter().any(|s| s.path == *file) {
                let text = if file.extension().is_some_and(|e| e == "bib") {
                    fs::read_to_string(file).unwrap_or_default()
                } else {
                    String::new()
                };
                self.source_cache.push(Source {
                    path: file.clone(),
                    text,
                });
            }
        }
        for source in &self.source_cache {
            if source.text.is_empty() || !latex::is_source(&source.path) {
                continue;
            }
            let editor = Editor::new(source.text.clone(), Some(source.path.clone()));
            for (row, level, title) in editor.outline().iter().cloned() {
                self.source_outline.push((
                    Target {
                        path: source.path.clone(),
                        row,
                        col: 0,
                        label: title,
                        detail: String::new(),
                    },
                    level,
                ));
            }
        }
    }
    pub(super) fn completion_sources(&self) -> Vec<Source> {
        let mut sources = self.source_cache.clone();
        for source in self.source_overlays() {
            if let Some(cached) = sources.iter_mut().find(|s| s.path == source.path) {
                cached.text = source.text;
            } else {
                sources.push(source);
            }
        }
        sources
    }
    pub(super) fn update_completion(&mut self) {
        if !self.editor().wants_completion() {
            self.editor_mut().completions.clear();
            return;
        }
        let sources = if self.editor().format == Format::Latex {
            self.completion_sources()
        } else {
            Vec::new()
        };
        self.editor_mut().update_completion_from(&sources);
    }
    pub(super) fn project_changed(&mut self) {
        if let Err(e) = self.project_settings.save(&self.project) {
            self.message = e.to_string();
        }
        self.refresh_sources();
        let active = self.active;
        self.activate(active);
    }
    pub(super) fn search_project(&mut self) {
        self.search_results.clear();
        if self.project_query.is_empty() {
            return;
        }
        let pattern = if self.project_query.chars().any(char::is_uppercase) {
            regex::escape(&self.project_query)
        } else {
            format!("(?i){}", regex::escape(&self.project_query))
        };
        let re = regex::Regex::new(&pattern).unwrap();
        for path in &self.files {
            if !Format::detect(path).editable()
                || fs::metadata(path).is_ok_and(|m| m.len() > 2 * 1024 * 1024)
            {
                continue;
            }
            let text = self
                .documents
                .iter()
                .find(|d| d.editor.path.as_ref() == Some(path))
                .map(|d| d.editor.text())
                .or_else(|| fs::read_to_string(path).ok())
                .unwrap_or_default();
            for (row, line) in text.lines().enumerate() {
                if let Some(found) = re.find(line) {
                    self.search_results.push(Target {
                        path: path.clone(),
                        row,
                        col: line[..found.start()].chars().count(),
                        label: line.trim().into(),
                        detail: String::new(),
                    });
                    if self.search_results.len() >= 500 {
                        return;
                    }
                }
            }
        }
    }
    pub(super) fn export_project(&mut self, ctx: &egui::Context) {
        if !self.save_all() {
            return;
        }
        let Some(destination) = rfd::FileDialog::new()
            .set_title("Exportar proyecto ZIP")
            .set_file_name(format!(
                "{}.zip",
                self.project
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ))
            .add_filter("Proyecto ZIP", &["zip"])
            .save_file()
        else {
            return;
        };
        let destination = if destination.extension().is_none() {
            destination.with_extension("zip")
        } else {
            destination
        };
        let project = self.project.clone();
        self.message = "Exportando el proyecto…".into();
        self.start_tool(ctx, move || {
            latex::export_zip(&project, &destination)
                .map(|_| {
                    ToolResult::Message(format!("Proyecto exportado en {}", destination.display()))
                })
                .map_err(|e| e.to_string())
        });
    }
    pub(super) fn import_project(&mut self, ctx: &egui::Context) {
        let Some(archive) = rfd::FileDialog::new()
            .set_title("Importar proyecto de Overleaf u otro ZIP")
            .add_filter("Proyecto ZIP", &["zip"])
            .pick_file()
        else {
            return;
        };
        let Some(parent) = rfd::FileDialog::new()
            .set_title("Elegir carpeta donde crear el proyecto importado")
            .pick_folder()
        else {
            return;
        };
        let name = archive.file_stem().unwrap_or_default().to_string_lossy();
        let mut destination = parent.join(name.as_ref());
        let mut suffix = 1;
        while destination.exists() {
            destination = parent.join(format!("{name}-{suffix}"));
            suffix += 1;
        }
        self.message = "Importando el proyecto…".into();
        self.start_tool(ctx, move || {
            latex::import_zip(&archive, &destination)
                .map(|_| ToolResult::Imported(destination))
                .map_err(|e| e.to_string())
        });
    }
    pub(super) fn add_files(&mut self) {
        let Some(files) = rfd::FileDialog::new()
            .set_title("Añadir archivos al proyecto")
            .pick_files()
        else {
            return;
        };
        for file in files {
            let destination = self.project.join(file.file_name().unwrap_or_default());
            if file == destination {
                continue;
            }
            let result = (|| {
                let mut input = fs::File::open(&file)?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                if let Err(e) = std::io::copy(&mut input, &mut output) {
                    let _ = fs::remove_file(&destination);
                    return Err(e);
                }
                Ok::<_, std::io::Error>(())
            })();
            if let Err(e) = result {
                self.message = format!("No pude añadir {}: {e}", destination.display());
                break;
            }
            self.message = format!("Añadido {}", destination.display());
        }
        self.files = project_files(&self.project);
        self.refresh_sources();
    }
}

pub(super) fn project_files(root: &Path) -> Vec<PathBuf> {
    fn walk(path: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > 12 || out.len() >= 3000 {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || ["target", "node_modules", "__pycache__", "build", "dist"]
                    .contains(&name.as_ref())
            {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                walk(&entry.path(), depth + 1, out);
            } else if kind.is_file() && Format::listed(&entry.path()) {
                out.push(entry.path());
            }
            if out.len() >= 3000 {
                break;
            }
        }
    }
    let mut files = Vec::new();
    walk(root, 0, &mut files);
    files
}
