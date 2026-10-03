//! Menús de macOS. Las acciones del editor son las de la paleta existente.

use super::{dialogs::Command, *};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSEventType, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Command(Command),
    Palette,
    Quit,
    Recent(usize),
    Friend,
    Dog,
    Jasmine,
    Sidebar(bool),
    Preview(bool),
    Wrap(&'static str, &'static str),
    Environment(&'static str),
}

struct Entry {
    group: &'static str,
    label: String,
    shortcut: &'static str,
    enabled: bool,
    checked: bool,
    action: Action,
}

struct TargetState {
    pending: RefCell<Vec<(usize, bool)>>,
    ctx: egui::Context,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. AppKit calls this on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetState]
    struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(performMiyuCommand:))]
        fn perform(&self, sender: &NSMenuItem) {
            let keyboard = NSApplication::sharedApplication(self.mtm()).currentEvent()
                .is_some_and(|event| event.r#type() == NSEventType::KeyDown);
            self.ivars().pending.borrow_mut().push((sender.tag() as usize, keyboard));
            self.ivars().ctx.request_repaint();
        }
    }
);

pub(super) struct NativeMenu {
    target: Retained<MenuTarget>,
    entries: Vec<Entry>,
    items: Vec<Retained<NSMenuItem>>,
}

pub(super) fn shortcut_event(shortcut: &str) -> Option<egui::Event> {
    let key = Key::from_name(shortcut.rsplit('+').next()?.to_uppercase().as_str())?;
    Some(egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers {
            command: shortcut.contains("Mod+"),
            mac_cmd: shortcut.contains("Mod+"),
            shift: shortcut.contains("Shift+"),
            ctrl: shortcut.contains("Ctrl+"),
            ..Default::default()
        },
    })
}

fn item(
    mtm: MainThreadMarker,
    title: &str,
    selector: Option<objc2::runtime::Sel>,
    shortcut: &str,
) -> Retained<NSMenuItem> {
    let mut modifiers = NSEventModifierFlags::empty();
    if shortcut.contains("Mod+") {
        modifiers |= NSEventModifierFlags::Command;
    }
    if shortcut.contains("Shift+") {
        modifiers |= NSEventModifierFlags::Shift;
    }
    if shortcut.contains("Ctrl+") {
        modifiers |= NSEventModifierFlags::Control;
    }
    let key = shortcut.rsplit('+').next().unwrap_or("");
    let key = if let Some(number) = key.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        char::from_u32(0xf703 + number)
            .unwrap_or_default()
            .to_string()
    } else {
        key.to_lowercase()
    };
    // SAFETY: Each selector is implemented by MenuTarget or by AppKit's responder chain.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            selector,
            &NSString::from_str(&key),
        )
    };
    item.setKeyEquivalentModifierMask(modifiers);
    item
}

impl NativeMenu {
    pub(super) fn new(ctx: &egui::Context) -> Self {
        let mtm = MainThreadMarker::new().expect("Los menús requieren el hilo principal");
        let target = MenuTarget::alloc(mtm).set_ivars(TargetState {
            pending: RefCell::new(Vec::new()),
            ctx: ctx.clone(),
        });
        // SAFETY: NSObject's init has the standard signature and initializes our target.
        let target = unsafe { msg_send![super(target), init] };
        Self {
            target,
            entries: Vec::new(),
            items: Vec::new(),
        }
    }

    fn update(&mut self, entries: Vec<Entry>) {
        let mtm = self.target.mtm();
        let rebuild = entries.len() != self.entries.len()
            || entries.iter().zip(&self.entries).any(|(a, b)| {
                (a.group, &a.label, a.shortcut, a.action)
                    != (b.group, &b.label, b.shortcut, b.action)
            });
        if rebuild {
            let main = NSMenu::new(mtm);
            let mut menus = HashMap::new();
            self.items.clear();
            for group in [
                "MiyuLaTeX",
                "Archivo",
                "Proyecto",
                "Editar",
                "Ver",
                "Insertar",
                "LaTeX",
                "Desarrollo",
                "Ventana",
                "Ayuda",
            ] {
                let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(group));
                menu.setAutoenablesItems(false);
                let parent = item(mtm, group, None, "");
                parent.setSubmenu(Some(&menu));
                main.addItem(&parent);
                menus.insert(group, menu);
            }
            for (index, entry) in entries.iter().enumerate() {
                if let Some((parent, label)) = entry.group.split_once('/')
                    && !menus.contains_key(entry.group)
                {
                    let menu =
                        NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(label));
                    menu.setAutoenablesItems(false);
                    let parent_item = item(mtm, label, None, "");
                    parent_item.setSubmenu(Some(&menu));
                    menus[parent].addItem(&parent_item);
                    menus.insert(entry.group, menu);
                }
                let menu_item = item(
                    mtm,
                    &entry.label,
                    Some(sel!(performMiyuCommand:)),
                    entry.shortcut,
                );
                menu_item.setTag(index as isize);
                // SAFETY: NativeMenu retains the target for longer than its menu items.
                unsafe {
                    menu_item.setTarget(Some(&self.target));
                }
                menus[entry.group].addItem(&menu_item);
                self.items.push(menu_item);
            }
            let app_menu = &menus["MiyuLaTeX"];
            app_menu.addItem(&NSMenuItem::separatorItem(mtm));
            app_menu.addItem(&item(mtm, "Ocultar MiyuLaTeX", Some(sel!(hide:)), "Mod+H"));
            app_menu.addItem(&item(
                mtm,
                "Mostrar todo",
                Some(sel!(unhideAllApplications:)),
                "",
            ));
            let window_menu = &menus["Ventana"];
            window_menu.setAutoenablesItems(true);
            window_menu.addItem(&item(
                mtm,
                "Minimizar",
                Some(sel!(performMiniaturize:)),
                "Mod+M",
            ));
            window_menu.addItem(&item(mtm, "Ampliar ventana", Some(sel!(performZoom:)), ""));
            NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
        }
        for (item, entry) in self.items.iter().zip(&entries) {
            item.setEnabled(entry.enabled);
            item.setState(if entry.checked { 1 } else { 0 });
        }
        self.entries = entries;
    }

    fn take_actions(&self) -> Vec<(Action, Option<egui::Event>)> {
        self.target
            .ivars()
            .pending
            .borrow_mut()
            .drain(..)
            .filter_map(|(index, keyboard)| {
                self.entries.get(index).map(|e| {
                    (
                        e.action,
                        keyboard.then(|| shortcut_event(e.shortcut)).flatten(),
                    )
                })
            })
            .collect()
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        // NSMenuItem no retiene su target. Quitar el menú antes de soltarlo.
        NSApplication::sharedApplication(self.target.mtm()).setMainMenu(None);
    }
}

impl App {
    pub(super) fn native_menus(&mut self, ctx: &egui::Context) {
        let Some(menu) = &self.native_menu else {
            return;
        };
        let actions = menu.take_actions();
        for (action, event) in actions {
            if self.pending.is_some() {
                break;
            }
            if let Some(event) = event {
                // El atajo sigue las reglas de foco del editor y de los campos de texto.
                ctx.input_mut(|input| input.events.push(event));
                continue;
            }
            match action {
                Action::Command(command) => self.run_command(command, ctx),
                Action::Quit => self.request_close(Pending::Quit, ctx),
                Action::Palette => self.open_palette(),
                Action::Recent(index) => {
                    if let Some(path) = self.config.recent_projects.get(index) {
                        self.open_project(PathBuf::from(path));
                    }
                }
                Action::Wrap(before, after) => {
                    self.editor_mut().wrap(before, after);
                    self.changed_editor();
                }
                Action::Environment(name) => {
                    let args = catalog().env_args.get(name).map_or("", String::as_str);
                    let body = &catalog().environments[name];
                    self.insert_snippet(&format!(
                        "\\begin{{{name}}}{args}\n    {}\n\\end{{{name}}}",
                        body.replace('\n', "\n    ")
                    ));
                }
                _ => {
                    match action {
                        Action::Friend => self.config.mascot_friend = !self.config.mascot_friend,
                        Action::Dog => self.config.mascot_dog = !self.config.mascot_dog,
                        Action::Jasmine => self.config.mascot_jasmine = !self.config.mascot_jasmine,
                        Action::Sidebar(right) => self.config.sidebar_right = right,
                        Action::Preview(left) => self.config.preview_left = left,
                        _ => unreachable!(),
                    }
                    self.preferences_changed(ctx);
                }
            }
        }
        let mut entries = Vec::new();
        for (label, shortcut, enabled, command) in self.commands() {
            use Command::*;
            let group = match command {
                Settings => "MiyuLaTeX",
                Help => "Ayuda",
                NewDocument | Open | OpenQuick | Save | SaveAs | SaveAll | History | Close
                | ExportPdf | ExportHtml | ExportMarkdownPdf | ExportEpub | MakePresentation => {
                    "Archivo"
                }
                NewProject | OpenFolder | NewFile | AddFiles | ImportZip | ExportZip => "Proyecto",
                Undo | Redo | Find | GotoLine | SearchProject | Comment | Format | Definition
                | RenameLabel | NextProblem | PreviousProblem => "Editar",
                ToggleSidebar | ShowGit | TogglePreview | ToggleWrap | ToggleSplit | Fold | FoldAll
                | UnfoldAll | ToggleProblems | ToggleMascot | ToggleTypewriter | ToggleFocus
                | WordGoal => "Ver",
                ToggleTerminal | NewTerminal | RunCode | TestCode | CheckCode => "Desarrollo",
                Bold | Italic | Symbol | Table | Figure | PasteImage | Reference | Citation => {
                    "Insertar"
                }
                _ => "LaTeX",
            };
            let checked = match command {
                ToggleSidebar => self.config.show_sidebar && !self.zen,
                TogglePreview => self.config.show_preview && !self.zen,
                ToggleWrap => self.config.soft_wrap,
                ToggleSplit => self.split.is_some(),
                ToggleProblems => self.panel && !self.developer.selected,
                ToggleTerminal => self.panel && self.developer.selected,
                ToggleMascot => self.config.mascot,
                ToggleTypewriter => self.config.typewriter,
                ToggleFocus => self.zen,
                ToggleAutocompile => self.config.autocompile,
                ToggleAutosave => self.config.autosave,
                Equation => self.equation.open,
                _ => false,
            };
            entries.push(Entry {
                group,
                label: label.into(),
                shortcut,
                enabled,
                checked,
                action: Action::Command(command),
            });
        }
        for (group, label, shortcut, enabled, checked, action) in [
            (
                "MiyuLaTeX",
                "Paleta de comandos…",
                "Mod+Shift+P",
                true,
                false,
                Action::Palette,
            ),
            (
                "MiyuLaTeX",
                "Salir de MiyuLaTeX",
                "Mod+Q",
                true,
                false,
                Action::Quit,
            ),
            (
                "Ver",
                "Cangrejito amigo del gatito",
                "",
                self.config.mascot,
                self.config.mascot_friend,
                Action::Friend,
            ),
            (
                "Ver",
                "Schnauzer amigo del gatito",
                "",
                self.config.mascot,
                self.config.mascot_dog,
                Action::Dog,
            ),
            (
                "Ver",
                "Jazmín en su maceta",
                "",
                self.config.mascot,
                self.config.mascot_jasmine,
                Action::Jasmine,
            ),
            (
                "Ver/Posición del panel de archivos",
                "Izquierda",
                "",
                true,
                !self.config.sidebar_right,
                Action::Sidebar(false),
            ),
            (
                "Ver/Posición del panel de archivos",
                "Derecha",
                "",
                true,
                self.config.sidebar_right,
                Action::Sidebar(true),
            ),
            (
                "Ver/Posición de la vista previa",
                "Izquierda",
                "",
                true,
                self.config.preview_left,
                Action::Preview(true),
            ),
            (
                "Ver/Posición de la vista previa",
                "Derecha",
                "",
                true,
                !self.config.preview_left,
                Action::Preview(false),
            ),
        ] {
            entries.push(Entry {
                group,
                label: label.into(),
                shortcut,
                enabled,
                checked,
                action,
            });
        }
        for (index, project) in self.config.recent_projects.iter().enumerate() {
            let path = Path::new(project);
            entries.push(Entry {
                group: "Proyecto/Proyectos recientes",
                label: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                shortcut: "",
                enabled: path.is_dir(),
                checked: path == self.project,
                action: Action::Recent(index),
            });
        }
        let latex = self.editor().format == Format::Latex;
        for (label, before, after) in [
            ("Matemática en línea", "\\(", "\\)"),
            ("Ecuación centrada", "\\[\n", "\n\\]"),
            ("Sección", "\\section{", "}"),
            ("Subsección", "\\subsection{", "}"),
            ("Subrayado", "\\underline{", "}"),
        ] {
            entries.push(Entry {
                group: "Insertar",
                label: label.into(),
                shortcut: "",
                enabled: latex,
                checked: false,
                action: Action::Wrap(before, after),
            });
        }
        for name in catalog().environments.keys() {
            entries.push(Entry {
                group: "Insertar/Entorno LaTeX",
                label: name.clone(),
                shortcut: "",
                enabled: latex,
                checked: false,
                action: Action::Environment(name),
            });
        }
        for entry in &mut entries {
            entry.enabled &= self.pending.is_none();
        }
        self.native_menu.as_mut().unwrap().update(entries);
    }
}
