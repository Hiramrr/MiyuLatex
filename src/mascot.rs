//! Mascota: un gatito de píxeles que vive sobre la barra de estado.

use eframe::egui::{self, Color32, Id, Painter, Pos2, Rect, Sense, pos2, vec2};

use crate::theme::{Theme, col};

/// Lado de un píxel del dibujo, en puntos.
const PIXEL: f32 = 2.0;
const WIDTH: usize = 12;
const HEIGHT: usize = 8;
/// Separación con los bordes de la ventana, en puntos.
const MARGIN: f32 = 10.0;
/// Segundos sin teclado ni ratón hasta que se duerme.
const SLEEP_AFTER: f64 = 60.0;
/// Puntos por segundo al caminar.
const SPEED: f32 = 28.0;
const JUMP_TIME: f64 = 0.45;
const JUMP_HEIGHT: f32 = 10.0;

/// `X` es cuerpo y `o` un ojo, que solo se rellena al parpadear. Mira a la
/// izquierda: la cola queda detrás.
type Sprite = [&'static str; HEIGHT];

const SIT: Sprite = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX...X.",
    "XoXXXoX...X.",
    "XXXXXXX..X..",
    "XXXXXXXXX...",
    "XXXXXXX.....",
    "X.X.X.X.....",
];
const WAG: Sprite = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XoXXXoX....X",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    "X.X.X.X.....",
];
const STEP: Sprite = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XoXXXoX....X",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    ".X.X.X......",
];
const SLEEP: Sprite = [
    "............",
    "............",
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XXXXXXXXXX..",
    "XXXXXXXXXXX.",
    "XXXXXXXXXX..",
];
const HEART: [&str; 3] = ["X.X", "XXX", ".X."];
const ZETA: [&str; 3] = ["XXX", ".X.", "XXX"];

#[derive(Clone, Copy, PartialEq)]
enum Pose {
    Sit,
    /// Camina hasta esa posición, de 0 a 1 a lo ancho de la ventana.
    Walk(f32),
    Sleep,
}

pub struct Mascot {
    /// Posición de 0 a 1 a lo ancho de la ventana.
    at: f32,
    pose: Pose,
    right: bool,
    /// Instante de egui en que deja de estar sentado.
    until: f64,
    /// Última vez que hubo teclado, ratón o compilación.
    active: f64,
    jumped: f64,
    busy: bool,
    last: f64,
    seed: u32,
}

impl Default for Mascot {
    fn default() -> Self {
        Self {
            at: 0.9,
            pose: Pose::Sit,
            right: false,
            until: 6.0,
            active: 0.0,
            jumped: f64::NEG_INFINITY,
            busy: false,
            last: 0.0,
            seed: 0x9E37_79B9,
        }
    }
}

impl Mascot {
    /// Lo dibuja de pie sobre `floor`. Salta cuando una compilación termina
    /// bien (`ok`). Devuelve `true` si el usuario pidió ocultarlo.
    pub fn show(&mut self, ui: &egui::Ui, floor: f32, theme: &Theme, busy: bool, ok: bool) -> bool {
        let ctx = ui.ctx();
        let (now, input, focused) = ctx.input(|i| (i.time, !i.events.is_empty(), i.focused));
        let elapsed = (now - self.last).clamp(0.0, 0.25) as f32;
        self.last = now;
        if input || busy {
            self.active = now;
        }
        if self.busy && !busy && ok {
            self.jumped = now;
        }
        self.busy = busy;

        let points = ctx.pixels_per_point();
        let snap = |v: f32| (v * points).round() / points;
        let pixel = snap(PIXEL).max(1.0 / points);
        let size = vec2(WIDTH as f32 * pixel, HEIGHT as f32 * pixel);
        let screen = ctx.content_rect();
        let span = (screen.width() - size.x - 2.0 * MARGIN).max(1.0);

        self.advance(now, elapsed * SPEED / span, busy);

        let jump = ((now - self.jumped) / JUMP_TIME) as f32;
        let jumping = (0.0..1.0).contains(&jump);
        let lift = if jumping {
            snap(4.0 * jump * (1.0 - jump) * JUMP_HEIGHT)
        } else {
            0.0
        };
        let origin = pos2(
            snap(screen.left() + MARGIN + self.at * span),
            snap(floor) - size.y - lift,
        );

        let response = ui
            .interact(
                Rect::from_min_size(origin, size),
                Id::new("mascota"),
                Sense::click(),
            )
            .on_hover_text("Clic derecho para ocultarlo");
        if response.clicked() && !jumping {
            self.jumped = now;
            self.active = now;
        }
        let mut hide = false;
        response.context_menu(|ui| {
            if ui.button("Ocultar el gatito").clicked() {
                hide = true;
                ui.close();
            }
        });

        // Parpadeo y cola van a cuatro pasos por segundo.
        let tick = (now * 4.0).floor() as i64;
        let (sprite, blink) = match self.pose {
            Pose::Sleep => (&SLEEP, false),
            Pose::Walk(_) => (
                if (now * 6.0) as i64 % 2 == 0 {
                    &SIT
                } else {
                    &STEP
                },
                false,
            ),
            Pose::Sit => {
                let wagging = jumping || busy || tick % 24 < 6;
                let sprite = if wagging && tick % 2 == 1 { &WAG } else { &SIT };
                (sprite, tick % 15 == 0)
            }
        };
        let painter = ui.painter();
        let body = col(theme.primary);
        paint(painter, origin, pixel, sprite, self.right, blink, body);
        let above = |column: f32, row: f32| origin + vec2(column * pixel, -row * pixel);
        if jumping {
            paint(
                painter,
                above(2.0, 5.0),
                pixel,
                &HEART,
                false,
                false,
                col(theme.error),
            );
        } else if self.pose == Pose::Sleep {
            let muted = col(theme.muted());
            let step = now.floor() as i64 % 3;
            if step >= 1 {
                paint(painter, above(8.0, 3.0), pixel, &ZETA, false, false, muted);
            }
            if step == 2 {
                paint(painter, above(12.0, 7.0), pixel, &ZETA, false, false, muted);
            }
        }

        // Solo pide los cuadros que cambian el dibujo, y ninguno si la ventana
        // está en segundo plano.
        if jumping {
            ctx.request_repaint();
        } else if focused || busy {
            let wait = match self.pose {
                // Avanza un píxel del dibujo unas catorce veces por segundo.
                Pose::Walk(_) => 1.0 / 15.0,
                Pose::Sit => (tick + 1) as f64 / 4.0 - now,
                Pose::Sleep => now.floor() + 1.0 - now,
            };
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.01)));
        }
        hide
    }

    /// `step` es lo que avanza caminando desde el cuadro anterior.
    fn advance(&mut self, now: f64, step: f32, busy: bool) {
        if now - self.active > SLEEP_AFTER {
            self.pose = Pose::Sleep;
            return;
        }
        match self.pose {
            Pose::Sleep => {
                self.pose = Pose::Sit;
                self.until = now + 3.0;
            }
            // Mientras compila se queda mirando.
            Pose::Sit if now >= self.until && !busy => {
                if self.random() < 0.6 {
                    let to = self.random();
                    self.right = to > self.at;
                    self.pose = Pose::Walk(to);
                } else {
                    self.until = now + 4.0 + 8.0 * self.random() as f64;
                }
            }
            Pose::Sit => {}
            Pose::Walk(to) => {
                if (to - self.at).abs() <= step {
                    self.at = to;
                    self.pose = Pose::Sit;
                    self.until = now + 4.0 + 8.0 * self.random() as f64;
                } else {
                    self.at += step.copysign(to - self.at);
                }
            }
        }
    }

    /// De 0 a 1.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Un rectángulo por cada tramo horizontal de píxeles seguidos.
fn paint(
    painter: &Painter,
    origin: Pos2,
    pixel: f32,
    sprite: &[&str],
    mirror: bool,
    blink: bool,
    color: Color32,
) {
    for (row, line) in sprite.iter().enumerate() {
        let mut cells: Vec<bool> = line
            .bytes()
            .map(|c| c == b'X' || (blink && c == b'o'))
            .collect();
        if mirror {
            cells.reverse();
        }
        let mut column = 0;
        while column < cells.len() {
            let start = column;
            while column < cells.len() && cells[column] {
                column += 1;
            }
            if column > start {
                let min = origin + vec2(start as f32 * pixel, row as f32 * pixel);
                let size = vec2((column - start) as f32 * pixel, pixel);
                painter.rect_filled(Rect::from_min_size(min, size), 0.0, color);
            }
            column += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_dibujos_miden_lo_mismo() {
        for sprite in [SIT, WAG, STEP, SLEEP] {
            assert!(sprite.iter().all(|line| line.len() == WIDTH));
        }
    }

    #[test]
    fn se_duerme_sin_actividad_y_despierta() {
        let mut cat = Mascot::default();
        cat.advance(SLEEP_AFTER + 1.0, 0.0, false);
        assert!(cat.pose == Pose::Sleep);
        cat.active = SLEEP_AFTER + 2.0;
        cat.advance(SLEEP_AFTER + 2.0, 0.0, false);
        assert!(cat.pose == Pose::Sit);
    }

    #[test]
    fn camina_sin_salirse_de_la_ventana() {
        let mut cat = Mascot::default();
        for frame in 0..20_000 {
            let now = frame as f64 / 60.0;
            cat.active = now;
            cat.advance(now, 0.001, false);
            assert!((0.0..=1.0).contains(&cat.at));
        }
        assert!(cat.at != Mascot::default().at, "nunca caminó");
    }
}
