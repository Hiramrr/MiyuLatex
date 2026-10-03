//! Maquetado del texto del editor. Cada línea se maqueta por separado y se
//! guarda por contenido: un cuadro sin cambios no cuesta nada y una edición
//! solo maqueta las líneas que tocó.

use std::{
    any::TypeId,
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Color32, FontId, Galley, Stroke, TextBuffer, TextFormat,
    text::{ByteIndex, LayoutJob, LayoutSection, TextWrapping},
};

use crate::{
    editor::Editor,
    highlight::Tok,
    syntax::{Ink, Run, Style},
    theme::{Theme, col},
};

/// Las claves ya son huellas: no hace falta volver a mezclarlas.
#[derive(Default)]
struct Identity(u64);
impl Hasher for Identity {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, _: &[u8]) {
        unreachable!("solo claves u64")
    }
    fn write_u64(&mut self, key: u64) {
        self.0 = key;
    }
}

/// Tiempo por cuadro para maquetar con color las filas lejos del cursor.
/// Lo que no cabe se maqueta como texto plano, que mide lo mismo (todas las
/// fichas usan la misma fuente monoespaciada) y es unas treinta veces más
/// barato: egui da forma a cada tramo de color por separado.
const BUDGET: Duration = Duration::from_millis(4);
/// Filas alrededor del cursor que siempre se maquetan con color.
const NEAR: usize = 120;
/// Marca la huella de una fila maquetada en plano, pendiente de color.
const PLAIN: u64 = 0x9e37_79b9_7f4a_7c15;
/// Marca la huella de una fila oculta por un pliegue.
const HIDDEN: u64 = 0xc2b2_ae3d_27d4_eb4f;

/// Aspecto con que se maqueta el texto.
pub struct Look<'a> {
    pub theme: &'a Theme,
    pub size: f32,
    pub line_height: Option<f32>,
    /// Cambia cuando egui rehace las fuentes y lo maquetado deja de valer.
    pub fonts: u64,
}

impl Look<'_> {
    fn format(&self, style: Style) -> TextFormat {
        let mut format = match style.ink {
            Ink::Tok(tok) => self.theme.syntax(tok, self.size),
            ink => TextFormat {
                font_id: FontId::monospace(self.size),
                color: match ink {
                    Ink::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
                    _ => col(self.theme.fg),
                },
                ..Default::default()
            },
        };
        format.italics |= style.italic;
        if style.underline {
            let color = match style.ink {
                Ink::Rgb(..) => format.color,
                _ => self.theme.syntax(Tok::Underline, self.size).color,
            };
            format.underline = Stroke::new(1.0, color);
        }
        format.line_height = self.line_height;
        format
    }
}

/// El editor detrás del búfer que egui pasa al maquetar.
pub fn editor(buffer: &dyn TextBuffer) -> Option<&Editor> {
    (buffer.type_id() == TypeId::of::<Editor>()).then(|| {
        // SAFETY: `type_id` confirma que el búfer es un `Editor`.
        unsafe { &*(buffer as *const dyn TextBuffer).cast::<Editor>() }
    })
}

/// Cuenta las veces que egui ha rehecho las fuentes: lo maquetado antes ya no
/// vale. Hay que llamarla en cada cuadro para que el testigo siga en caché.
pub fn fonts_epoch(ui: &egui::Ui) -> u64 {
    let witness = ui.fonts_mut(|fonts| {
        fonts.layout_job(LayoutJob::simple_singleline(
            "·".into(),
            FontId::monospace(10.0),
            Color32::WHITE,
        ))
    });
    ui.data_mut(|data| {
        let (epoch, last) = data
            .get_temp_mut_or_default::<(u64, Option<Arc<Galley>>)>(egui::Id::new("fonts_epoch"));
        if !last
            .as_ref()
            .is_some_and(|last| Arc::ptr_eq(last, &witness))
        {
            *epoch += 1;
            *last = Some(witness);
        }
        *epoch
    })
}

#[derive(Default)]
pub struct Layout {
    /// Huella del aspecto y el ajuste de línea con que se maquetó.
    look: u64,
    width: f32,
    /// Revisión del texto, versión del resaltado y de los pliegues ya maquetadas.
    seen: Option<(u64, u64, u64)>,
    /// Párrafos ya maquetados, por huella de su contenido.
    cache: HashMap<u64, Arc<Galley>, BuildHasherDefault<Identity>>,
    /// Huella y párrafo de cada fila del último maquetado.
    keys: Vec<u64>,
    rows: Vec<Arc<Galley>>,
    /// El último maquetado completo.
    pub galley: Option<Arc<Galley>>,
}

impl Layout {
    /// El texto maquetado. `width` es el ancho de ajuste, infinito si no se ajusta.
    pub fn galley(
        &mut self,
        ui: &egui::Ui,
        editor: &Editor,
        look: &Look,
        width: f32,
    ) -> Arc<Galley> {
        let width = width.round();
        let scale = ui.ctx().pixels_per_point();
        let theme = look.theme;
        let key = egui::util::hash((
            (theme.fg, theme.bg, theme.primary, theme.secondary),
            (theme.accent, theme.warning, theme.error, theme.success),
            look.size.to_bits(),
            look.line_height.map(f32::to_bits),
            look.fonts,
            scale.to_bits(),
            width.is_finite(),
        ));
        if self.look != key {
            self.cache.clear();
        } else if self.width != width {
            // Una línea que cabe entera en una fila no cambia con el ancho.
            self.cache
                .retain(|_, galley| galley.rows.len() == 1 && galley.rect.width() < width - 1.0);
        }
        if self.look != key || self.width != width {
            self.look = key;
            self.width = width;
            self.keys.clear();
            self.rows.clear();
            self.seen = None;
        }
        let now = Some((editor.revision, editor.syntax.version, editor.fold_version));
        if self.seen == now
            && let Some(galley) = &self.galley
        {
            return galley.clone();
        }
        self.seen = now;

        // Como hace egui: el salto final va con el último párrafo.
        let lines = &editor.lines;
        let count = lines.len() - usize::from(lines.len() > 1 && lines[lines.len() - 1].is_empty());
        let trailing = count < lines.len();
        let keys: Vec<u64> = (0..count)
            .map(|row| {
                let key = editor.syntax.lines[row].key ^ u64::from(trailing && row + 1 == count);
                if editor.is_hidden(row) {
                    key ^ HIDDEN
                } else {
                    key
                }
            })
            .collect();
        // Lo habitual es que cambien unas pocas filas seguidas.
        let head = self
            .keys
            .iter()
            .zip(&keys)
            .take_while(|(old, new)| old == new)
            .count();
        let tail = self.keys[head..]
            .iter()
            .rev()
            .zip(keys[head..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        let wrap = TextWrapping {
            max_width: width,
            ..Default::default()
        };
        let started = Instant::now();
        let cursor = editor.cursor.row;
        let mut keys = keys;
        let mut pending = false;
        let changed: Vec<Arc<Galley>> = (head..count - tail)
            .map(|row| {
                let last = trailing && row + 1 == count;
                if let Some(galley) = self.cache.get(&keys[row]) {
                    return galley.clone();
                }
                if editor.is_hidden(row) {
                    let galley = hidden(ui, &lines[row], last);
                    self.cache.insert(keys[row], galley.clone());
                    return galley;
                }
                if row.abs_diff(cursor) > NEAR && started.elapsed() > BUDGET {
                    pending = true;
                    keys[row] ^= PLAIN;
                    return self
                        .cache
                        .entry(keys[row])
                        .or_insert_with(|| paragraph(ui, &lines[row], &[], last, look, &wrap))
                        .clone();
                }
                let runs = &editor.syntax.lines[row].runs;
                let galley = paragraph(ui, &lines[row], runs, last, look, &wrap);
                self.cache.insert(keys[row], galley.clone());
                galley
            })
            .collect();
        let removed = self.keys.len() - head - tail;
        self.rows.splice(head..head + removed, changed);
        self.keys = keys;
        if pending {
            // Las filas en plano se colorean en los cuadros siguientes.
            self.seen = None;
            ui.ctx().request_repaint();
        }
        if self.cache.len() > 2 * count + 1024 {
            self.cache = self
                .keys
                .iter()
                .copied()
                .zip(self.rows.iter().cloned())
                .collect();
        }

        // Las filas ya llevan su geometría: el conjunto solo necesita el texto.
        let mut job = LayoutJob {
            text: editor.source().to_owned(),
            wrap,
            ..Default::default()
        };
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(0)..ByteIndex(job.text.len()),
            format: look.format(Style::default()),
        });
        let galley = Arc::new(Galley::concat(Arc::new(job), &self.rows, scale));
        self.galley = Some(galley.clone());
        galley
    }
}

/// Una línea oculta por un pliegue: conserva su texto, que el widget sigue
/// editando, pero no ocupa alto ni se ve.
fn hidden(ui: &egui::Ui, text: &str, newline: bool) -> Arc<Galley> {
    let format = TextFormat {
        font_id: FontId::monospace(1.0),
        color: Color32::TRANSPARENT,
        line_height: Some(0.0),
        ..Default::default()
    };
    let text = if newline {
        format!("{text}\n")
    } else {
        text.to_owned()
    };
    ui.fonts_mut(|fonts| fonts.layout_job(LayoutJob::single_section(text, format)))
}

fn paragraph(
    ui: &egui::Ui,
    text: &str,
    runs: &[Run],
    newline: bool,
    look: &Look,
    wrap: &TextWrapping,
) -> Arc<Galley> {
    let mut job = LayoutJob {
        text: if newline {
            format!("{text}\n")
        } else {
            text.to_owned()
        },
        wrap: wrap.clone(),
        ..Default::default()
    };
    let mut start = 0;
    for run in runs {
        let end = (run.end as usize).min(text.len());
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(start)..ByteIndex(end),
            format: look.format(run.style),
        });
        start = end;
    }
    if start < job.text.len() || job.sections.is_empty() {
        job.sections.push(LayoutSection {
            leading_space: 0.0,
            byte_range: ByteIndex(start)..ByteIndex(job.text.len()),
            format: look.format(Style::default()),
        });
    }
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme;

    /// Maquetar por líneas da la misma geometría que maquetar el texto entero,
    /// también tras editar.
    #[test]
    fn matches_whole_text_layout() {
        let source = "\\section{Uno} texto $x^2$ % nota\n\nun párrafo largo que se ajusta al ancho disponible varias veces seguidas\n\\begin{verbatim}\n  ñ\n\\end{verbatim}\n";
        let theme = theme::builtin().remove(0);
        let ctx = egui::Context::default();
        let mut editor = Editor::new(source.into(), None);
        let mut layout = Layout::default();
        let mut check = |editor: &Editor| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let look = Look {
                    theme: &theme,
                    size: 16.0,
                    line_height: None,
                    fonts: fonts_epoch(ui),
                };
                for width in [180.0, f32::INFINITY] {
                    let galley = layout.galley(ui, editor, &look, width);
                    let mut job = LayoutJob {
                        text: editor.source().to_owned(),
                        wrap: TextWrapping {
                            max_width: width,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let mut start = 0;
                    for (line, syntax) in editor.lines.iter().zip(&editor.syntax.lines) {
                        let mut at = start;
                        for run in &syntax.runs {
                            let end = start + run.end as usize;
                            job.sections.push(LayoutSection {
                                leading_space: 0.0,
                                byte_range: ByteIndex(at)..ByteIndex(end),
                                format: look.format(run.style),
                            });
                            at = end;
                        }
                        let end = (start + line.len() + 1).min(job.text.len());
                        job.sections.push(LayoutSection {
                            leading_space: 0.0,
                            byte_range: ByteIndex(at)..ByteIndex(end),
                            format: look.format(Style::default()),
                        });
                        start = end;
                    }
                    let whole = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    assert_eq!(galley.text(), editor.source());
                    assert_eq!(galley.rect, whole.rect);
                    let rows = |galley: &Galley| -> Vec<_> {
                        galley
                            .rows
                            .iter()
                            .map(|row| (row.rect(), row.glyphs.len(), row.ends_with_newline))
                            .collect()
                    };
                    assert_eq!(rows(&galley), rows(&whole));
                    assert!(galley.rows.len() > editor.lines.len() || width.is_infinite());
                }
            });
            output.textures_delta.clear();
        };
        check(&editor);
        editor.goto(2, 3);
        editor.insert("más texto\ny otra línea ");
        check(&editor);
        editor.undo(false);
        check(&editor);
        assert_eq!(editor.source(), source);
    }
}
