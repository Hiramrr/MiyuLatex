//! Diálogo para fijar la meta de palabras del documento activo.

use super::*;

pub(in crate::app) struct GoalDialog {
    text: String,
    focus: bool,
}

impl GoalDialog {
    pub(in crate::app) fn new(target: usize) -> Self {
        Self {
            text: target.to_string(),
            focus: true,
        }
    }
}

impl App {
    pub(in crate::app) fn goal_dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.goal_dialog else {
            return;
        };
        let mut open = true;
        let mut set = None;
        let mut clear = false;
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            open = false;
        }
        let has_goal = self.documents[self.active].goal.is_some();
        egui::Window::new("Meta de palabras")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(340.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Palabras que quieres escribir en esta sesión. Cuentan las que añadas desde ahora, no el total del documento.");
                let response = ui.add(
                    TextEdit::singleline(&mut dialog.text)
                        .hint_text("Por ejemplo, 500")
                        .desired_width(120.0),
                );
                if std::mem::take(&mut dialog.focus) {
                    response.request_focus();
                }
                let target = dialog
                    .text
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=writing::MAX_GOAL).contains(n));
                if target.is_none() {
                    ui.label(
                        RichText::new(format!("Escribe un número entre 1 y {}.", writing::MAX_GOAL))
                            .color(col(self.theme.warning)),
                    );
                }
                ui.horizontal(|ui| {
                    let fix = ui.add_enabled(target.is_some(), egui::Button::new("Fijar meta"));
                    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if fix.clicked() || (enter && target.is_some()) {
                        set = target;
                    }
                    if has_goal && ui.button("Quitar meta").clicked() {
                        clear = true;
                    }
                });
            });
        if let Some(target) = set {
            self.set_goal(target);
            open = false;
        }
        if clear {
            self.clear_goal();
            open = false;
        }
        if !open {
            self.goal_dialog = None;
            self.focus_editor = true;
        }
    }
}
