//! Terminales y tareas del proyecto. Cada tarea usa su propio proceso.

use super::*;
use crate::terminal::{self, Terminal};
use portable_pty::CommandBuilder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TaskKind {
    Run,
    Test,
    Check,
    Build,
    Other,
}

#[derive(Clone, Debug)]
pub(super) struct Task {
    pub name: String,
    pub kind: TaskKind,
    pub program: String,
    pub args: Vec<String>,
    pub directory: PathBuf,
}

impl Task {
    fn description(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn command(&self) -> CommandBuilder {
        let program = compiler::which(&self.program).unwrap_or_else(|| self.program.clone().into());
        let mut command = CommandBuilder::new(program);
        command.args(&self.args);
        command
    }
}

#[derive(Default)]
pub(super) struct State {
    pub terminals: Vec<Terminal>,
    pub selected: bool,
    active: usize,
    focus: bool,
    next_id: u64,
    error: Option<String>,
    tasks: Vec<Task>,
    task_index: usize,
    context: Option<(PathBuf, Option<PathBuf>)>,
    checked: Option<Instant>,
}

impl App {
    pub(super) fn refresh_tasks(&mut self) {
        let context = (self.project.clone(), self.editor().path.clone());
        if self.developer.context.as_ref() == Some(&context)
            && self
                .developer
                .checked
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        let tasks = tasks(&context.0, context.1.as_deref());
        let selected = self
            .developer
            .tasks
            .get(self.developer.task_index)
            .map(|t| (t.program.clone(), t.args.clone()));
        self.developer.task_index = selected
            .and_then(|(program, args)| {
                tasks
                    .iter()
                    .position(|t| t.program == program && t.args == args)
            })
            .unwrap_or(0);
        self.developer.tasks = tasks;
        self.developer.context = Some(context);
        self.developer.checked = Some(Instant::now());
    }

    pub(super) fn has_task(&self, kind: TaskKind) -> bool {
        self.developer.tasks.iter().any(|t| t.kind == kind)
    }

    pub(super) fn terminal_focused(&self, ctx: &egui::Context) -> bool {
        self.panel
            && self.developer.selected
            && self
                .developer
                .terminals
                .get(self.developer.active)
                .is_some_and(|t| ctx.memory(|m| m.has_focus(t.id)))
    }

    pub(super) fn poll_terminals(&mut self, ctx: &egui::Context) {
        for terminal in &mut self.developer.terminals {
            terminal.poll(ctx);
        }
    }

    fn add_terminal(
        &mut self,
        name: String,
        directory: PathBuf,
        command: CommandBuilder,
        ctx: &egui::Context,
    ) {
        self.developer.next_id += 1;
        let id = Id::new(("terminal", self.developer.next_id));
        match Terminal::spawn(id, name, &directory, command, ctx) {
            Ok(terminal) => {
                self.developer.terminals.push(terminal);
                self.developer.active = self.developer.terminals.len() - 1;
                self.developer.error = None;
                self.developer.focus = true;
                self.focus_editor = false;
            }
            Err(e) => {
                self.message = format!("No pude abrir la terminal: {e}");
                self.developer.error = Some(self.message.clone());
            }
        }
        self.panel = true;
        self.developer.selected = true;
    }

    pub(super) fn new_terminal(&mut self, ctx: &egui::Context) {
        let name = format!("Shell {}", self.developer.next_id + 1);
        self.add_terminal(name, self.project.clone(), terminal::shell(), ctx);
    }

    pub(super) fn show_terminal(&mut self, ctx: &egui::Context) {
        self.panel = true;
        self.developer.selected = true;
        self.focus_editor = false;
        if self.developer.terminals.is_empty() {
            self.new_terminal(ctx);
        } else {
            self.developer.focus = true;
        }
    }

    pub(super) fn toggle_terminal(&mut self, ctx: &egui::Context) {
        if self.panel && self.developer.selected {
            self.panel = false;
            self.focus_editor = true;
        } else {
            self.show_terminal(ctx);
        }
    }

    pub(super) fn toggle_problems(&mut self) {
        self.panel = !self.panel || self.developer.selected;
        self.developer.selected = false;
        self.focus_editor = true;
    }

    pub(super) fn run_task_kind(&mut self, kind: TaskKind, ctx: &egui::Context) {
        self.developer.checked = None;
        self.refresh_tasks();
        if let Some(index) = self.developer.tasks.iter().position(|t| t.kind == kind) {
            self.developer.task_index = index;
            self.run_task(ctx);
        } else {
            self.message = "No hay una tarea para esta acción. Abre la terminal o guarda el archivo de código.".into();
        }
    }

    fn run_task(&mut self, ctx: &egui::Context) {
        let Some(task) = self.developer.tasks.get(self.developer.task_index).cloned() else {
            return;
        };
        if matches!(self.editor().format, Format::Code(_))
            && self.editor().path.is_none()
            && !self.save_document(self.active, false)
        {
            return;
        }
        // Solo se guardan fuentes de esta tarea. Una pestaña ajena no abre un diálogo.
        for i in 0..self.documents.len() {
            let editor = &self.documents[i].editor;
            if editor.format.editable()
                && editor.dirty()
                && editor
                    .path
                    .as_ref()
                    .is_some_and(|p| p.starts_with(&task.directory))
                && !self.save_document(i, false)
            {
                return;
            }
        }
        self.message = format!("Ejecutando {}", task.description());
        self.add_terminal(
            task.name.clone(),
            task.directory.clone(),
            task.command(),
            ctx,
        );
    }

    pub(super) fn developer_menu(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        for (label, enabled, command, help) in [
            (
                "Mostrar u ocultar terminal",
                true,
                dialogs::Command::ToggleTerminal,
                "Ctrl+`. Conserva la sesión al ocultar el panel.",
            ),
            (
                "Nueva terminal",
                true,
                dialogs::Command::NewTerminal,
                "Ctrl+Mayús+`. Abre una shell en la carpeta del proyecto.",
            ),
            (
                "Ejecutar código",
                self.has_task(TaskKind::Run),
                dialogs::Command::RunCode,
                "Guarda los cambios y ejecuta la tarea del proyecto. F5 o Cmd/Ctrl+R en código.",
            ),
            (
                "Ejecutar pruebas",
                self.has_task(TaskKind::Test),
                dialogs::Command::TestCode,
                "Guarda los cambios y ejecuta las pruebas en una terminal propia.",
            ),
            (
                "Comprobar código",
                self.has_task(TaskKind::Check),
                dialogs::Command::CheckCode,
                "Guarda los cambios y comprueba el código con la herramienta del proyecto.",
            ),
        ] {
            if action(ui, label, enabled, help).clicked() {
                self.run_command(command, &ctx);
                ui.close();
            }
        }
    }

    pub(super) fn terminal_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("Sesión", |ui| {
                if action(
                    ui,
                    "Nueva terminal",
                    true,
                    "Abre una shell en el proyecto. Ctrl+Mayús+`.",
                )
                .clicked()
                {
                    self.new_terminal(&ctx);
                    ui.close();
                }
                let alive = self
                    .developer
                    .terminals
                    .get(self.developer.active)
                    .is_some_and(|t| t.exit.is_none());
                if action(
                    ui,
                    "Interrumpir",
                    alive,
                    "Envía Ctrl+C al proceso de esta sesión.",
                )
                .clicked()
                {
                    self.developer.terminals[self.developer.active].interrupt();
                    ui.close();
                }
                if action(
                    ui,
                    "Limpiar",
                    !self.developer.terminals.is_empty(),
                    "Limpia la pantalla y el historial de esta terminal.",
                )
                .clicked()
                {
                    self.developer.terminals[self.developer.active].clear();
                    ui.close();
                }
            });
            if self.developer.tasks.is_empty() {
                ui.label(RichText::new("Sin tareas detectadas").color(col(self.theme.muted())));
            } else {
                let task = &self.developer.tasks[self.developer.task_index];
                egui::ComboBox::from_id_salt("developer_tasks")
                    .selected_text(&task.name)
                    .width(180.0)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for (index, task) in self.developer.tasks.iter().enumerate() {
                            ui.selectable_value(&mut self.developer.task_index, index, &task.name)
                                .on_hover_text(task.description());
                        }
                    });
                let task = &self.developer.tasks[self.developer.task_index];
                if action(
                    ui,
                    "Ejecutar tarea",
                    true,
                    &format!("{}\n{}", task.description(), task.directory.display()),
                )
                .clicked()
                {
                    self.run_task(&ctx);
                }
            }
        });
        let mut close = None;
        ScrollArea::horizontal()
            .id_salt("terminal_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, terminal) in self.developer.terminals.iter().enumerate() {
                        ui.push_id(terminal.id, |ui| {
                            if ui
                                .selectable_label(self.developer.active == index, &terminal.name)
                                .on_hover_text(terminal.directory.display().to_string())
                                .clicked()
                            {
                                self.developer.active = index;
                                self.developer.focus = true;
                                self.focus_editor = false;
                            }
                            if action(ui, "×", true, "Cierra esta sesión y detiene su proceso.")
                                .clicked()
                            {
                                close = Some(index);
                            }
                        });
                    }
                });
            });
        if let Some(index) = close {
            self.developer.terminals.remove(index);
            self.developer.active = if index < self.developer.active {
                self.developer.active - 1
            } else {
                self.developer
                    .active
                    .min(self.developer.terminals.len().saturating_sub(1))
            };
            self.developer.focus = true;
        }
        if let Some(error) = &self.developer.error {
            ui.colored_label(col(self.theme.error), error);
        }
        if self.developer.terminals.is_empty() {
            ui.label("Abre una terminal para usar la shell del sistema en este proyecto.");
            return;
        }
        let terminal = &mut self.developer.terminals[self.developer.active];
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(terminal.directory.display().to_string()).size(12.0).color(col(self.theme.muted())))
                .on_hover_text("Carpeta inicial de esta sesión. Cambiar de proyecto no mueve las terminales abiertas.");
            if let Some(exit) = &terminal.exit {
                let (text, color) = match exit {
                    Ok(status) => (format!("Proceso terminado · código {}", status.exit_code()), if status.success() { self.theme.muted() } else { self.theme.error }),
                    Err(e) => (e.clone(), self.theme.error),
                };
                ui.colored_label(col(color), text);
            }
            if let Some(error) = &terminal.error { ui.colored_label(col(self.theme.error), error); }
        });
        let focus = std::mem::take(&mut self.developer.focus);
        terminal.show(
            ui,
            focus,
            self.config.font_size.clamp(12.0, 24.0) as f32,
            col(self.theme.fg),
            col(self.theme.bg),
        );
    }
}

fn tasks(project: &Path, file: Option<&Path>) -> Vec<Task> {
    let start = file.and_then(Path::parent).unwrap_or(project);
    let project = if start.starts_with(project) {
        project
    } else {
        start
    };
    let root = start
        .ancestors()
        .take_while(|p| p.starts_with(project))
        .find(|dir| {
            ["Cargo.toml", "package.json", "go.mod", "pyproject.toml"]
                .iter()
                .any(|name| dir.join(name).is_file())
        })
        .unwrap_or(project);
    let mut result = Vec::new();
    let mut add = |name: &str, kind: TaskKind, program: &str, args: &[&str]| {
        result.push(Task {
            name: name.into(),
            kind,
            program: program.into(),
            args: args.iter().map(|s| (*s).into()).collect(),
            directory: root.into(),
        });
    };
    if root.join("Cargo.toml").is_file() {
        add("Ejecutar · cargo run", TaskKind::Run, "cargo", &["run"]);
        add("Pruebas · cargo test", TaskKind::Test, "cargo", &["test"]);
        add(
            "Comprobar · cargo check",
            TaskKind::Check,
            "cargo",
            &["check"],
        );
        add(
            "Compilar · cargo build",
            TaskKind::Build,
            "cargo",
            &["build"],
        );
    }
    if let Ok(text) = fs::read_to_string(root.join("package.json"))
        && let Ok(package) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(scripts) = package
            .get("scripts")
            .and_then(serde_json::Value::as_object)
    {
        let manager = if root.join("pnpm-lock.yaml").is_file() {
            "pnpm"
        } else if root.join("yarn.lock").is_file() {
            "yarn"
        } else if root.join("bun.lock").is_file() || root.join("bun.lockb").is_file() {
            "bun"
        } else {
            "npm"
        };
        let mut scripts: Vec<_> = scripts
            .iter()
            .filter(|(_, value)| value.is_string())
            .collect();
        scripts.sort_by_key(|(name, _)| match name.as_str() {
            "dev" => 0,
            "start" => 1,
            "test" => 2,
            "check" | "typecheck" | "lint" => 3,
            _ => 4,
        });
        for (name, _) in scripts {
            let kind = if name == "test" {
                TaskKind::Test
            } else if ["lint", "check", "typecheck"].contains(&name.as_str()) {
                TaskKind::Check
            } else if name == "build" {
                TaskKind::Build
            } else if ["dev", "start", "serve"].contains(&name.as_str()) {
                TaskKind::Run
            } else {
                TaskKind::Other
            };
            add(
                &format!("{manager} run {name}"),
                kind,
                manager,
                &["run", name],
            );
        }
    }
    if root.join("go.mod").is_file() {
        add("Ejecutar · go run", TaskKind::Run, "go", &["run", "."]);
        add(
            "Pruebas · go test",
            TaskKind::Test,
            "go",
            &["test", "./..."],
        );
        add(
            "Comprobar · go vet",
            TaskKind::Check,
            "go",
            &["vet", "./..."],
        );
        add(
            "Compilar · go build",
            TaskKind::Build,
            "go",
            &["build", "./..."],
        );
    }
    let python_file = file.filter(|p| p.extension().is_some_and(|e| e == "py"));
    if python_file.is_some() || root.join("pyproject.toml").is_file() {
        let local = root.join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        });
        let python = if local.is_file() {
            local.to_string_lossy().into_owned()
        } else if cfg!(windows) {
            "python".into()
        } else {
            "python3".into()
        };
        if let Some(file) = python_file {
            let file = file.to_string_lossy();
            add("Ejecutar archivo Python", TaskKind::Run, &python, &[&file]);
            add(
                "Comprobar sintaxis Python",
                TaskKind::Check,
                &python,
                &["-m", "py_compile", &file],
            );
        }
        if root.join("pyproject.toml").is_file() {
            // ponytail: pytest se detecta como texto; leer TOML si hay que
            // distinguir dependencias de menciones en comentarios.
            let pytest =
                fs::read_to_string(root.join("pyproject.toml")).is_ok_and(|t| t.contains("pytest"));
            add(
                "Pruebas Python",
                TaskKind::Test,
                &python,
                if pytest {
                    &["-m", "pytest"]
                } else {
                    &["-m", "unittest", "discover"]
                },
            );
        }
    }
    if result.is_empty()
        && let Some(file) = file
    {
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("");
        let filename = file.to_string_lossy();
        let program = match extension {
            "js" | "mjs" | "cjs" => Some("node"),
            "sh" => Some("sh"),
            "rb" => Some("ruby"),
            _ => None,
        };
        if let Some(program) = program {
            result.push(Task {
                name: format!("Ejecutar archivo · {program}"),
                kind: TaskKind::Run,
                program: program.into(),
                args: vec![filename.into_owned()],
                directory: root.into(),
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_tasks_and_literal_arguments() {
        let folder = std::env::temp_dir().join(format!("miyu-tasks-{}", std::process::id()));
        fs::create_dir_all(folder.join("src")).unwrap();
        fs::write(folder.join("Cargo.toml"), "[package]\nname = 'app'\n").unwrap();
        let rust = tasks(&folder, Some(&folder.join("src/main.rs")));
        assert_eq!(rust.len(), 4);
        assert_eq!(rust[0].directory, folder);
        assert_eq!(rust[2].args, ["check"]);
        fs::remove_file(folder.join("Cargo.toml")).unwrap();
        fs::write(folder.join("package.json"), r#"{"scripts":{"test":"echo ok","dev":"node app.js","check":"tsc","test; echo nope":"echo safe"}}"#).unwrap();
        fs::write(folder.join("pnpm-lock.yaml"), "").unwrap();
        let js = tasks(&folder, None);
        assert_eq!(js[0].args, ["run", "dev"]);
        assert!(js.iter().all(|t| t.program == "pnpm"));
        let odd = js.iter().find(|t| t.args[1] == "test; echo nope").unwrap();
        assert_eq!(odd.command().get_argv()[2], "test; echo nope");
        fs::write(folder.join("package.json"), "invalid json").unwrap();
        let python = tasks(&folder, Some(&folder.join("hola ñ.py")));
        assert_eq!(python[0].args, [folder.join("hola ñ.py").to_string_lossy()]);
        assert_eq!(python[1].kind, TaskKind::Check);
        assert!(tasks(&folder, Some(&folder.join("main.tex"))).is_empty());
        fs::remove_dir_all(folder).unwrap();
    }
}
