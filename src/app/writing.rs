//! Modo escritura: cursor centrado, modo sin distracciones y meta de palabras.

use super::*;

/// Hasta este tamaño el recuento acompaña a cada edición; en textos mayores
/// espera a que se deje de escribir, porque el criterio de LaTeX tokeniza todo.
const LIVE_LIMIT: usize = 32 * 1024;
const PAUSE: Duration = Duration::from_millis(600);
/// Tope del diálogo; evita metas que ni caben en la barra de estado.
pub(super) const MAX_GOAL: usize = 1_000_000;

/// Lo que recuerda el editor del cursor centrado entre cuadros.
#[derive(Default)]
pub(super) struct Typewriter {
    /// Revisión del texto e índice del cursor la última vez que se centró.
    pub(super) seen: Option<(u64, usize)>,
    /// Alto en pantalla del cursor, y centro vertical y ancho del editor, para las pruebas.
    #[cfg(test)]
    pub(super) line: f32,
    #[cfg(test)]
    pub(super) center: f32,
    #[cfg(test)]
    pub(super) width: f32,
}

/// Meta de una sesión: palabras netas añadidas desde que se fijó.
pub(super) struct Goal {
    pub(super) target: usize,
    /// Palabras del documento al fijar la meta.
    baseline: usize,
    words: usize,
    /// Revisión del texto con que se contó `words`.
    pub(super) counted: u64,
    /// Última revisión vista y cuándo cambió, para esperar a una pausa.
    seen: (u64, Instant),
    reached: bool,
}

impl Goal {
    fn new(target: usize, words: usize, revision: u64) -> Self {
        Self {
            target,
            baseline: words,
            words,
            counted: revision,
            seen: (revision, Instant::now()),
            reached: false,
        }
    }
    /// Palabras añadidas; si se borró más de lo escrito, ninguna.
    pub(super) fn done(&self) -> usize {
        self.words.saturating_sub(self.baseline)
    }
}

/// Palabras de la prosa: en LaTeX sin comandos ni fórmulas; en el resto,
/// las secuencias de letras, que dejan fuera la puntuación de Markdown.
pub(super) fn count_words(format: &Format, text: &str) -> usize {
    if *format == Format::Latex {
        return latex::prose_words(text);
    }
    text.split(|c: char| !c.is_alphabetic() && c != '\'')
        .filter(|word| word.chars().any(char::is_alphabetic))
        .count()
}

impl App {
    pub(super) fn toggle_zen(&mut self) {
        self.zen = !self.zen;
        self.focus_editor = true;
        self.sync_cursor = true;
        self.message = if self.zen {
            "Modo sin distracciones. Esc o la misma acción para salir.".into()
        } else {
            String::new()
        };
    }
    /// Quien pide mostrar un panel sale antes del modo sin distracciones.
    /// Devuelve si estaba activo.
    pub(super) fn leave_zen(&mut self) -> bool {
        let was = self.zen;
        if was {
            self.toggle_zen();
        }
        was
    }
    pub(super) fn toggle_typewriter(&mut self, ctx: &egui::Context) {
        self.config.typewriter = !self.config.typewriter;
        self.preferences_changed(ctx);
        // Centra la línea actual sin esperar a la próxima tecla.
        self.sync_cursor = true;
    }
    pub(super) fn open_goal_dialog(&mut self) {
        if !self.editor().format.editable() {
            return;
        }
        let target = self.documents[self.active]
            .goal
            .as_ref()
            .map_or(500, |g| g.target);
        self.goal_dialog = Some(dialogs::GoalDialog::new(target));
    }
    /// Fija la meta del documento activo: desde ahora cuentan las palabras que se añadan.
    pub(super) fn set_goal(&mut self, target: usize) {
        let doc = &mut self.documents[self.active];
        let words = count_words(&doc.editor.format, doc.editor.source());
        doc.goal = Some(Goal::new(target, words, doc.editor.revision));
        self.message = format!("Meta fijada: {target} palabras nuevas en este documento.");
    }
    pub(super) fn clear_goal(&mut self) {
        if self.documents[self.active].goal.take().is_some() {
            self.message = "Meta de palabras quitada.".into();
        }
    }
    /// Pone al día el recuento del documento activo, solo si cambió el texto.
    pub(super) fn update_goal(&mut self, ctx: &egui::Context) {
        let doc = &mut self.documents[self.active];
        let Some(goal) = &mut doc.goal else {
            return;
        };
        let revision = doc.editor.revision;
        if goal.counted == revision {
            return;
        }
        if goal.seen.0 != revision {
            goal.seen = (revision, Instant::now());
        }
        let text = doc.editor.source();
        if text.len() > LIVE_LIMIT {
            let quiet = goal.seen.1.elapsed();
            if quiet < PAUSE {
                ctx.request_repaint_after(PAUSE - quiet);
                return;
            }
        }
        goal.words = count_words(&doc.editor.format, text);
        goal.counted = revision;
        if !goal.reached && goal.done() >= goal.target {
            goal.reached = true;
            self.message = format!("Meta alcanzada: {} palabras. ¡Buen trabajo!", goal.target);
        }
    }
    /// Texto de la barra de estado, ayuda y si ya se alcanzó.
    pub(super) fn goal_status(&self) -> Option<(String, String, bool)> {
        let goal = self.documents[self.active].goal.as_ref()?;
        Some((
            format!("{} / {} palabras", goal.done(), goal.target),
            "Palabras añadidas desde que fijaste la meta, no el total del documento.".into(),
            goal.reached,
        ))
    }
}
