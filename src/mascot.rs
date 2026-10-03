//! Mascota: un gatito de píxeles que vive sobre la barra de estado.

use eframe::egui::{self, Color32, Event, Id, Painter, Pos2, Rect, Sense, pos2, vec2};

use crate::theme::{Theme, col};

/// Lado de un píxel del dibujo, en puntos.
const PIXEL: f32 = 2.0;
const WIDTH: usize = 12;
const HEIGHT: usize = 8;
/// Separación con los bordes de la ventana, en puntos. Ahí caben el portátil
/// y el ovillo, que se dibujan delante del gatito.
const MARGIN: f32 = 14.0;
/// Segundos sin teclado ni ratón hasta que se duerme.
const SLEEP_AFTER: f64 = 60.0;
/// Segundos que sigue tecleando después de la última tecla.
const TYPING_FOR: f64 = 1.2;
/// Puntos por segundo al caminar.
const SPEED: f32 = 28.0;
const JUMP_TIME: f64 = 0.45;
const JUMP_HEIGHT: f32 = 10.0;
/// Segundos que dura el susto cuando la compilación falla, y su temblor.
const ALARM_TIME: f64 = 1.4;
const SHAKE_TIME: f64 = 0.4;

/// `X` es cuerpo, `o` un ojo que solo se rellena al parpadear, `#` un objeto,
/// `*` el color de acento, `!` el de error, `w` el pelo claro del schnauzer
/// y los pétalos del jazmín, y `g` sus hojas. Mira a la izquierda: la cola
/// queda detrás.
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
/// Se lame una pata con los ojos cerrados; la pata va aparte, en `PAW`.
const GROOM: Sprite = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XXXXXXX.....",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    "..X.X.X.....",
];
/// Se estira con la cabeza en el suelo y la grupa en alto.
const STRETCH: [Sprite; 2] = [
    [
        "..........X.",
        "..........X.",
        "........XXX.",
        "X.....XXXXX.",
        "XX...XXXXXX.",
        "XXXXXXXXX.X.",
        "XoXXXoX.X.X.",
        "XXXXXXX.X.X.",
    ],
    [
        "............",
        "...........X",
        "........XXX.",
        "X.....XXXXX.",
        "XX...XXXXXX.",
        "XXXXXXXXX.X.",
        "XoXXXoX.X.X.",
        "XXXXXXX.X.X.",
    ],
];

// Lo que se dibuja alrededor del gatito, con su columna y su fila.
const HEART: [&str; 3] = ["!.!", "!!!", ".!."];
const EXCLAIM: [&str; 5] = ["!", "!", "!", ".", "!"];
const ZETA: [&str; 3] = ["###", ".#.", "###"];
const DOTS: [[&str; 1]; 4] = [["....."], ["#...."], ["#.#.."], ["#.#.#"]];
const PAW: [&str; 1] = ["X"];
const BALL: [&str; 2] = ["**", "**"];
/// De perfil: la tapa inclinada a la izquierda, el brillo de la pantalla y
/// el teclado en el suelo, con las patas del gatito encima.
const LAPTOP: [[&str; 5]; 2] = [
    ["#.....", "#*....", ".#*...", ".#....", "..####"],
    ["#*....", "#.....", ".#*.XX", ".#....", "..####"],
];

/// El cangrejito es más pequeño y simétrico: no hace falta reflejarlo.
type Crab = [&'static str; 6];

const CRAB: Crab = [
    "...........",
    "XX.......XX",
    ".X.XXXXX.X.",
    ".XXXoXoXXX.",
    "..XXXXXXX..",
    "..X.X.X.X..",
];
const CRAB_STEP: Crab = [
    "...........",
    "XX.......XX",
    ".X.XXXXX.X.",
    ".XXXoXoXXX.",
    "..XXXXXXX..",
    ".X..X.X..X.",
];
/// Con las pinzas abiertas en alto: saluda o celebra.
const CRAB_UP: Crab = [
    "X.X.....X.X",
    ".X.......X.",
    ".X.XXXXX.X.",
    ".XXXoXoXXX.",
    "..XXXXXXX..",
    "..X.X.X.X..",
];
const CRAB_SLEEP: Crab = [
    "...........",
    "...........",
    "...........",
    "XX.XXXXX.XX",
    ".XXXXXXXXX.",
    "..XXXXXXX..",
];
/// El schnauzer mira a la izquierda, con barba, cejas y patas claras y la
/// cola corta en alto. Mide lo mismo que el gatito y se refleja como él.
const DOG: Sprite = [
    "..X.X.......",
    ".wwXX.....X.",
    ".XoXX.....X.",
    "wwwXXXXXXXX.",
    "wwwXXXXXXXX.",
    ".w.XXXXXXXX.",
    "...w.w..w.w.",
    "...w.w..w.w.",
];
const DOG_STEP: Sprite = [
    "..X.X.......",
    ".wwXX.....X.",
    ".XoXX.....X.",
    "wwwXXXXXXXX.",
    "wwwXXXXXXXX.",
    ".w.XXXXXXXX.",
    "...w.w..w.w.",
    "..w..w..w..w",
];
/// Con la cola hacia el otro lado: alternado con `DOG`, la menea.
const DOG_WAG: Sprite = [
    "..X.X.......",
    ".wwXX......X",
    ".XoXX.....X.",
    "wwwXXXXXXXX.",
    "wwwXXXXXXXX.",
    ".w.XXXXXXXX.",
    "...w.w..w.w.",
    "...w.w..w.w.",
];
const DOG_SLEEP: Sprite = [
    "............",
    "............",
    "............",
    "............",
    "..X.X.......",
    ".XXXXXXXXXX.",
    "wwwXXXXXXXXX",
    "wwwwwXXXXXww",
];
/// El ladrido, delante del hocico.
const BARK: [&str; 3] = [".!", "!.", ".!"];

/// El jazmín, en su maceta en la esquina izquierda. Las ramas van aparte de
/// la maceta para mecerse con la brisa: de día tienen capullos y de noche,
/// mientras el gatito duerme, se abren las flores blancas de cinco pétalos.
type Bush = [&'static str; 10];

const JASMINE_BUDS: Bush = [
    "...w........",
    "...w....w...",
    "...g....w...",
    ".gg.g..g....",
    "....g.g.....",
    ".w...gg.....",
    ".wgg.gg.....",
    "....gg..w...",
    ".....g..wg..",
    ".....gggg...",
];
const JASMINE_OPEN: Bush = [
    "...w........",
    "..w*w...w...",
    "...w...w*w..",
    ".gg.g..gw...",
    ".w..g.g.....",
    "w*w..gg.....",
    ".wgg.gg.w...",
    "....gg.w*w..",
    ".....g..wg..",
    ".....gggg...",
];
/// Filas de arriba que se mecen; las de abajo siguen unidas a la maceta.
const JASMINE_SWAYS: usize = 5;
const JASMINE_POT: [&str; 4] = ["..########..", "...######...", "...######...", "....####...."];
/// Una nota del perfume, que sube desde las flores.
const SCENT: [&str; 1] = ["*"];
/// Segundos que siguen abiertas las flores tras una compilación correcta o un clic.
const BLOOM_TIME: f64 = 8.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Crab,
    Dog,
}

/// Los dibujos de un amigo: quieto, a medio paso, contento y dormido.
struct Look {
    still: &'static [&'static str],
    step: &'static [&'static str],
    happy: &'static [&'static str],
    sleep: &'static [&'static str],
}

impl Kind {
    fn look(self) -> Look {
        match self {
            Kind::Crab => Look {
                still: &CRAB,
                step: &CRAB_STEP,
                happy: &CRAB_UP,
                sleep: &CRAB_SLEEP,
            },
            Kind::Dog => Look {
                still: &DOG,
                step: &DOG_STEP,
                happy: &DOG_WAG,
                sleep: &DOG_SLEEP,
            },
        }
    }

    fn hide(self) -> &'static str {
        match self {
            Kind::Crab => "Ocultar el cangrejito",
            Kind::Dog => "Ocultar el schnauzer",
        }
    }
}

/// Lo que tarda un amigo en saltar detrás del gatito.
const HOP_DELAY: f64 = 0.15;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Company {
    Idle,
    /// Camina hasta esa posición, de 0 a 1 a lo ancho de la ventana.
    Walk(f32),
    /// Va hasta el gatito, aunque se mueva.
    Visit,
    Greet,
}

/// Un amigo del gatito: pasea por su cuenta y de vez en cuando va a
/// saludarlo.
struct Friend {
    kind: Kind,
    at: f32,
    right: bool,
    pose: Company,
    until: f64,
    hopped: f64,
    seed: u32,
}

impl Friend {
    fn new(kind: Kind) -> Self {
        let (at, until, seed) = match kind {
            Kind::Crab => (0.75, 3.0, 0x51F1_5EED),
            Kind::Dog => (0.15, 5.0, 0x0D06_CAFE),
        };
        Self {
            kind,
            at,
            right: false,
            pose: Company::Idle,
            until,
            hopped: f64::NEG_INFINITY,
            seed,
        }
    }
}

impl Friend {
    /// `beside` es el sitio junto al gatito. Devuelve `true` al llegar a
    /// saludarlo.
    fn advance(&mut self, now: f64, step: f32, beside: f32) -> bool {
        match self.pose {
            Company::Idle if now >= self.until => {
                let choice = random(&mut self.seed);
                if choice < 0.45 {
                    self.pose = Company::Walk(random(&mut self.seed));
                } else if choice < 0.75 {
                    // Si el gatito no se deja alcanzar, desiste.
                    self.pose = Company::Visit;
                    self.until = now + 15.0;
                } else {
                    self.until = now + rest(&mut self.seed);
                }
            }
            Company::Idle => {}
            Company::Walk(to) => {
                if self.reach(to, step) {
                    self.pose = Company::Idle;
                    self.until = now + rest(&mut self.seed);
                }
            }
            Company::Visit => {
                if self.reach(beside, step) {
                    self.pose = Company::Greet;
                    self.until = now + 1.6;
                    return true;
                }
                if now >= self.until {
                    self.pose = Company::Idle;
                    self.until = now + rest(&mut self.seed);
                }
            }
            Company::Greet => {
                if now >= self.until {
                    self.pose = Company::Idle;
                    self.until = now + rest(&mut self.seed);
                }
            }
        }
        false
    }

    /// Da un paso hacia `to` y dice si ya llegó.
    fn reach(&mut self, to: f32, step: f32) -> bool {
        if (to - self.at).abs() <= step {
            self.at = to;
            return true;
        }
        self.right = to > self.at;
        self.at += step.copysign(to - self.at);
        false
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Pose {
    Sit,
    /// Camina hasta esa posición, de 0 a 1 a lo ancho de la ventana.
    Walk(f32),
    Sleep,
    Groom,
    Play,
    Stretch,
}

pub struct Mascot {
    /// Posición de 0 a 1 a lo ancho de la ventana.
    at: f32,
    pose: Pose,
    right: bool,
    /// Instante de egui en que termina la pose actual.
    until: f64,
    /// Última vez que hubo teclado, ratón o compilación.
    active: f64,
    typed: f64,
    jumped: f64,
    alarmed: f64,
    busy: bool,
    last: f64,
    seed: u32,
    /// El cangrejito y el schnauzer.
    friends: [Friend; 2],
    /// Última vez que se abrió el jazmín de día.
    bloomed: f64,
}

impl Default for Mascot {
    fn default() -> Self {
        Self {
            at: 0.9,
            pose: Pose::Sit,
            right: false,
            until: 6.0,
            active: 0.0,
            typed: f64::NEG_INFINITY,
            jumped: f64::NEG_INFINITY,
            alarmed: f64::NEG_INFINITY,
            busy: false,
            last: 0.0,
            seed: 0x9E37_79B9,
            friends: [Friend::new(Kind::Crab), Friend::new(Kind::Dog)],
            bloomed: f64::NEG_INFINITY,
        }
    }
}

impl Mascot {
    /// Lo dibuja de pie sobre `floor`. Al terminar una compilación salta si
    /// salió bien (`ok`) y se asusta si no. `company` dice si lo acompañan el
    /// cangrejito, el schnauzer y el jazmín; se apagan si el usuario pide
    /// ocultarlos. Devuelve `true` si pidió ocultar al gatito.
    pub fn show(
        &mut self,
        ui: &egui::Ui,
        floor: f32,
        theme: &Theme,
        busy: bool,
        ok: bool,
        company: &mut [bool; 3],
    ) -> bool {
        let ctx = ui.ctx();
        let (now, input, keys, focused) = ctx.input(|i| {
            let keys = i
                .events
                .iter()
                .any(|e| matches!(e, Event::Text(_) | Event::Key { pressed: true, .. }));
            (i.time, !i.events.is_empty(), keys, i.focused)
        });
        let elapsed = (now - self.last).clamp(0.0, 0.25) as f32;
        self.last = now;
        if input || busy {
            self.active = now;
        }
        if keys {
            self.typed = now;
        }
        if self.busy && !busy {
            if ok {
                self.jumped = now;
                self.bloomed = now;
            } else {
                self.alarmed = now;
            }
        }
        self.busy = busy;
        let typing = now - self.typed < TYPING_FOR;

        let points = ctx.pixels_per_point();
        let snap = |v: f32| (v * points).round() / points;
        let pixel = snap(PIXEL).max(1.0 / points);
        let size = vec2(WIDTH as f32 * pixel, HEIGHT as f32 * pixel);
        let screen = ctx.content_rect();
        let span = (screen.width() - size.x - 2.0 * MARGIN).max(1.0);

        self.advance(now, elapsed * SPEED / span, busy, typing);

        let frame = |rate: f64| (now * rate).floor() as i64;
        let jump = ((now - self.jumped) / JUMP_TIME) as f32;
        let jumping = (0.0..1.0).contains(&jump);
        let lift = if jumping {
            snap(4.0 * jump * (1.0 - jump) * JUMP_HEIGHT)
        } else {
            0.0
        };
        let alarmed = now - self.alarmed < ALARM_TIME;
        let shaking = now - self.alarmed < SHAKE_TIME;
        let shake = if shaking && frame(14.0) % 2 == 0 {
            pixel
        } else {
            0.0
        };
        let origin = pos2(
            snap(screen.left() + MARGIN + self.at * span) + shake,
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

        // Cada pose cambia de dibujo a su ritmo, en cuadros por segundo.
        let mut around: Vec<(&[&str], i32, i32)> = Vec::new();
        let mut blink = false;
        let (sprite, rate): (&Sprite, f64) = match self.pose {
            Pose::Sleep => {
                let step = frame(1.0) % 3;
                if step >= 1 {
                    around.push((&ZETA, 8, -3));
                }
                if step == 2 {
                    around.push((&ZETA, 12, -7));
                }
                (&SLEEP, 1.0)
            }
            // Avanza un píxel del dibujo unas catorce veces por segundo.
            Pose::Walk(_) => (if frame(6.0) % 2 == 0 { &SIT } else { &STEP }, 15.0),
            Pose::Groom => {
                around.push((&PAW, -1, if frame(4.0) % 2 == 0 { 4 } else { 3 }));
                (&GROOM, 4.0)
            }
            Pose::Play => {
                let step = (frame(6.0) % 4) as usize;
                around.push((&BALL, [-3, -3, -5, -4][step], if step == 2 { 5 } else { 6 }));
                if step == 1 {
                    around.push((&PAW, -1, 6));
                }
                (if step.is_multiple_of(2) { &SIT } else { &WAG }, 6.0)
            }
            Pose::Stretch => (&STRETCH[(frame(2.0) % 2) as usize], 2.0),
            Pose::Sit if typing && !jumping => {
                around.push((&LAPTOP[(frame(8.0) % 2) as usize], -6, 3));
                blink = frame(8.0) % 30 == 0;
                (&SIT, 8.0)
            }
            Pose::Sit => {
                let tick = frame(4.0);
                if busy {
                    around.push((&DOTS[(tick % 4) as usize], 1, -2));
                }
                blink = tick % 15 == 0;
                let wagging = jumping || busy || tick % 24 < 6;
                (if wagging && tick % 2 == 1 { &WAG } else { &SIT }, 4.0)
            }
        };
        if jumping {
            around.push((&HEART, 2, -5));
        } else if alarmed {
            around.push((&EXCLAIM, 3, -7));
        }

        let (body, object) = (col(theme.primary), col(theme.muted()));
        let (accent, alert) = (col(theme.accent), col(theme.error));
        let (light, leaf) = (col(theme.fg), col(theme.success));
        let inks = |body: Color32, blink: bool| {
            move |cell: u8| match cell {
                b'X' => Some(body),
                b'o' if blink => Some(body),
                b'#' => Some(object),
                b'*' => Some(accent),
                b'!' => Some(alert),
                b'w' => Some(light),
                b'g' => Some(leaf),
                _ => None,
            }
        };
        let painter = ui.painter();

        let asleep = self.pose == Pose::Sleep;
        // El jazmín va detrás de todos: el gatito y sus amigos pasan por delante.
        if company[2] {
            let size = vec2(JASMINE_BUDS[0].len() as f32, (JASMINE_BUDS.len() + JASMINE_POT.len()) as f32) * pixel;
            let origin = pos2(snap(screen.left() + MARGIN), snap(floor) - size.y);
            let response = ui
                .interact(Rect::from_min_size(origin, size), Id::new("jazmín de la mascota"), Sense::click())
                .on_hover_text("Clic derecho para ocultarlo");
            if response.clicked() {
                self.bloomed = now;
                self.active = now;
            }
            response.context_menu(|ui| {
                if ui.button("Ocultar el jazmín").clicked() {
                    company[2] = false;
                    ui.close();
                }
            });
            // Huele de noche, como el de verdad.
            let open = asleep || now - self.bloomed < BLOOM_TIME;
            let ink = inks(leaf, false);
            // Una racha de brisa cada pocos segundos mece las ramas.
            let sway = if frame(2.0) % 9 == 0 { 1 } else { 0 };
            let branches: &[&str] = if open { &JASMINE_OPEN } else { &JASMINE_BUDS };
            paint(painter, origin, pixel, &branches[..JASMINE_SWAYS], (sway, 0), false, &ink);
            paint(painter, origin, pixel, &branches[JASMINE_SWAYS..], (0, JASMINE_SWAYS as i32), false, &ink);
            paint(painter, origin, pixel, &JASMINE_POT, (0, JASMINE_BUDS.len() as i32), false, &ink);
            if asleep {
                let rise = (frame(2.0) % 4) as i32;
                paint(painter, origin, pixel, &SCENT, (2 + rise % 2, -1 - 2 * rise), false, &ink);
                paint(painter, origin, pixel, &SCENT, (8 - rise % 2, -2 * ((rise + 2) % 4)), false, &ink);
            }
            if open && !asleep && now - self.bloomed < BLOOM_TIME {
                // Que se cierre a su hora aunque no haya más eventos.
                let left = self.bloomed + BLOOM_TIME - now;
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(left.max(0.01)));
            }
        }
        for (index, pet) in self.friends.iter_mut().enumerate() {
            if !company[index] {
                continue;
            }
            let look = pet.kind.look();
            let width = look.still[0].len();
            // Se pone del lado en que ya está, salvo que ahí no quepa.
            let gap = |pixels: usize| (pixels + 2) as f32 * pixel / span;
            let after = self.at + gap(WIDTH);
            let before = self.at - gap(width);
            let beside = if (pet.at > self.at && after <= 1.0) || before < 0.0 {
                after
            } else {
                before
            }
            .clamp(0.0, 1.0);
            // Corretea algo más deprisa que el gatito.
            if !asleep && pet.advance(now, 1.3 * elapsed * SPEED / span, beside) {
                self.jumped = now;
            }
            if pet.pose == Company::Greet {
                pet.right = self.at > pet.at;
            }
            let hop = ((now - pet.hopped.max(self.jumped + HOP_DELAY)) / JUMP_TIME) as f32;
            let hopping = (0.0..1.0).contains(&hop);
            let lift = if hopping {
                snap(4.0 * hop * (1.0 - hop) * JUMP_HEIGHT)
            } else {
                0.0
            };
            let size = vec2(width as f32, look.still.len() as f32) * pixel;
            let origin = pos2(
                snap(screen.left() + MARGIN + pet.at * span),
                snap(floor) - size.y - lift,
            );
            let response = ui
                .interact(
                    Rect::from_min_size(origin, size),
                    Id::new(("amigo de la mascota", index)),
                    Sense::click(),
                )
                .on_hover_text("Clic derecho para ocultarlo");
            if response.clicked() && !hopping {
                pet.hopped = now;
                self.active = now;
            }
            response.context_menu(|ui| {
                if ui.button(pet.kind.hide()).clicked() {
                    company[index] = false;
                    ui.close();
                }
            });
            // Cada uno hace su gracia a un ritmo distinto: el cangrejito
            // chasquea las pinzas y el schnauzer menea la cola.
            let tick = frame(4.0);
            let dog = pet.kind == Kind::Dog;
            let (every, shift) = if dog { (22, 3) } else { (28, 9) };
            let (sprite, rate) = match pet.pose {
                _ if asleep => (look.sleep, 1.0),
                _ if hopping => (look.happy, 4.0),
                Company::Walk(_) | Company::Visit => (
                    if frame(8.0) % 2 == 0 {
                        look.still
                    } else {
                        look.step
                    },
                    15.0,
                ),
                Company::Greet | Company::Idle => {
                    let happy = pet.pose == Company::Greet || (tick + shift) % every < 6;
                    (
                        if happy && tick % 2 == 0 {
                            look.happy
                        } else {
                            look.still
                        },
                        4.0,
                    )
                }
            };
            let body = if dog { object } else { col(theme.secondary) };
            let ink = inks(body, (tick + shift) % 17 == 0);
            // Solo el schnauzer tiene perfil; mide lo mismo que el gatito.
            let mirror = dog && pet.right;
            paint(painter, origin, pixel, sprite, (0, 0), mirror, &ink);
            // Si la compilación falla, ladra.
            if dog && alarmed && tick % 2 == 0 {
                paint(painter, origin, pixel, &BARK, (-3, 2), mirror, &ink);
            }
            if hopping {
                ctx.request_repaint();
            } else if focused || busy {
                let wait = (frame(rate) + 1) as f64 / rate - now;
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.01)));
            }
        }

        let ink = inks(body, blink);
        paint(painter, origin, pixel, sprite, (0, 0), self.right, &ink);
        for (sprite, column, row) in around {
            paint(
                painter,
                origin,
                pixel,
                sprite,
                (column, row),
                self.right,
                &ink,
            );
        }

        // Solo pide los cuadros que cambian el dibujo, y ninguno si la ventana
        // está en segundo plano.
        if jumping || shaking {
            ctx.request_repaint();
        } else if focused || busy {
            let wait = (frame(rate) + 1) as f64 / rate - now;
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait.max(0.01)));
        }
        hide
    }

    /// `step` es lo que avanza caminando desde el cuadro anterior.
    fn advance(&mut self, now: f64, step: f32, busy: bool, typing: bool) {
        if now - self.active > SLEEP_AFTER {
            self.pose = Pose::Sleep;
            return;
        }
        match self.pose {
            Pose::Sleep => {
                self.pose = Pose::Sit;
                self.until = now + 3.0;
            }
            // Mientras escribes o compila se queda sentado: teclea o espera.
            Pose::Sit if now >= self.until && !busy && !typing => {
                let choice = random(&mut self.seed);
                let (pose, lasts) = if choice < 0.45 {
                    let to = random(&mut self.seed);
                    self.right = to > self.at;
                    (Pose::Walk(to), 0.0)
                } else if choice < 0.6 {
                    (Pose::Groom, 2.5)
                } else if choice < 0.75 {
                    (Pose::Play, 4.0)
                } else if choice < 0.85 {
                    (Pose::Stretch, 2.0)
                } else {
                    (Pose::Sit, rest(&mut self.seed))
                };
                self.pose = pose;
                self.until = now + lasts;
            }
            Pose::Sit => {}
            _ if busy || typing => {
                self.pose = Pose::Sit;
                self.until = now + 3.0;
            }
            Pose::Walk(to) => {
                if (to - self.at).abs() <= step {
                    self.at = to;
                    self.pose = Pose::Sit;
                    self.until = now + rest(&mut self.seed);
                } else {
                    self.at += step.copysign(to - self.at);
                }
            }
            Pose::Groom | Pose::Play | Pose::Stretch => {
                if now >= self.until {
                    self.pose = Pose::Sit;
                    self.until = now + rest(&mut self.seed);
                }
            }
        }
    }
}

/// Segundos de descanso antes de hacer otra cosa.
fn rest(seed: &mut u32) -> f64 {
    4.0 + 8.0 * random(seed) as f64
}

/// De 0 a 1.
fn random(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// Un rectángulo por cada tramo horizontal del mismo color. `offset` es la
/// columna y la fila del dibujo respecto al gatito; con `mirror` todo se
/// refleja, a lo ancho del gatito, para que mire a la derecha.
fn paint(
    painter: &Painter,
    origin: Pos2,
    pixel: f32,
    sprite: &[&str],
    offset: (i32, i32),
    mirror: bool,
    ink: &impl Fn(u8) -> Option<Color32>,
) {
    for (row, line) in sprite.iter().enumerate() {
        let cells = line.as_bytes();
        let mut column = 0;
        while column < cells.len() {
            let start = column;
            let color = ink(cells[start]);
            while column < cells.len() && ink(cells[column]) == color {
                column += 1;
            }
            let Some(color) = color else { continue };
            let left = if mirror {
                WIDTH as i32 - column as i32 - offset.0
            } else {
                start as i32 + offset.0
            };
            let min = origin + vec2(left as f32, (row as i32 + offset.1) as f32) * pixel;
            let size = vec2((column - start) as f32 * pixel, pixel);
            painter.rect_filled(Rect::from_min_size(min, size), 0.0, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_dibujos_miden_lo_mismo() {
        for sprite in [SIT, WAG, STEP, SLEEP, GROOM, STRETCH[0], STRETCH[1]] {
            assert!(sprite.iter().all(|line| line.len() == WIDTH));
        }
        // El schnauzer se refleja a lo ancho del gatito.
        for sprite in [DOG, DOG_STEP, DOG_WAG, DOG_SLEEP] {
            assert!(sprite.iter().all(|line| line.len() == WIDTH));
        }
        for sprite in [CRAB, CRAB_STEP, CRAB_UP, CRAB_SLEEP] {
            assert!(sprite.iter().all(|line| line.len() == CRAB[0].len()));
        }
        let jasmine = JASMINE_BUDS.iter().chain(&JASMINE_OPEN).chain(&JASMINE_POT);
        assert!(jasmine.into_iter().all(|line| line.len() == JASMINE_BUDS[0].len()));
    }

    #[test]
    fn se_duerme_sin_actividad_y_despierta() {
        let mut cat = Mascot::default();
        cat.advance(SLEEP_AFTER + 1.0, 0.0, false, false);
        assert_eq!(cat.pose, Pose::Sleep);
        cat.active = SLEEP_AFTER + 2.0;
        cat.advance(SLEEP_AFTER + 2.0, 0.0, false, false);
        assert_eq!(cat.pose, Pose::Sit);
    }

    #[test]
    fn pasa_por_todas_las_poses_sin_salirse_de_la_ventana() {
        let mut cat = Mascot::default();
        let mut seen = Vec::new();
        for frame in 0..60_000 {
            let now = frame as f64 / 60.0;
            cat.active = now;
            cat.advance(now, 0.001, false, false);
            assert!((0.0..=1.0).contains(&cat.at));
            let pose = std::mem::discriminant(&cat.pose);
            if !seen.contains(&pose) {
                seen.push(pose);
            }
        }
        // Todas menos dormir.
        assert_eq!(seen.len(), 5);
    }

    /// Cuántos rectángulos de cada color pinta un cuadro con esos eventos.
    fn drawn(
        cat: &mut Mascot,
        ctx: &egui::Context,
        time: f64,
        events: Vec<Event>,
        busy: bool,
    ) -> Vec<Color32> {
        let theme = crate::theme::builtin().remove(0);
        let input = egui::RawInput {
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            cat.show(ui, 500.0, &theme, busy, false, &mut [false; 3]);
        });
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.fill),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn saca_el_portatil_al_escribir_y_avisa_si_la_compilacion_falla() {
        let ctx = egui::Context::default();
        let theme = crate::theme::builtin().remove(0);
        let (object, alert) = (col(theme.muted()), col(theme.error));
        let mut cat = Mascot::default();
        let quiet = drawn(&mut cat, &ctx, 1.0, vec![], false);
        assert!(!quiet.contains(&object) && !quiet.contains(&alert));
        let typing = drawn(&mut cat, &ctx, 1.1, vec![Event::Text("a".into())], false);
        assert!(typing.contains(&object), "falta el portátil");
        // Deja de teclear poco después de la última tecla.
        let later = drawn(&mut cat, &ctx, 1.2 + TYPING_FOR, vec![], false);
        assert!(!later.contains(&object));
        let thinking = drawn(&mut cat, &ctx, 3.3, vec![], true);
        assert!(thinking.contains(&object), "faltan los puntos de espera");
        let failed = drawn(&mut cat, &ctx, 3.5, vec![], false);
        assert!(failed.contains(&alert), "falta el aviso");
    }

    #[test]
    fn el_amigo_va_a_saludar_y_luego_sigue_a_lo_suyo() {
        let mut crab = Friend {
            pose: Company::Visit,
            until: 100.0,
            ..Friend::new(Kind::Crab)
        };
        let mut now = 0.0;
        while !crab.advance(now, 0.01, 0.3) {
            now += 0.1;
            assert!(now < 100.0, "nunca llegó");
        }
        assert_eq!((crab.at, crab.pose), (0.3, Company::Greet));
        let mut seen = vec![crab.pose];
        for _ in 0..60_000 {
            now += 1.0 / 60.0;
            // El gatito siempre queda lejos: no llega antes de desistir.
            let far = if crab.at < 0.5 { 1.0 } else { 0.0 };
            assert!(!crab.advance(now, 0.0005, far), "saludó sin alcanzarlo");
            assert!((0.0..=1.0).contains(&crab.at));
            if !seen.contains(&crab.pose) && !matches!(crab.pose, Company::Walk(_)) {
                seen.push(crab.pose);
            }
        }
        assert_eq!(seen, [Company::Greet, Company::Idle, Company::Visit]);
    }

    #[test]
    fn cada_amigo_se_dibuja_solo_si_esta_activo() {
        let ctx = egui::Context::default();
        let theme = crate::theme::builtin().remove(0);
        let count = |mut friends: [bool; 3]| {
            let mut cat = Mascot::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                cat.show(ui, 500.0, &theme, false, false, &mut friends);
            });
            output.textures_delta.clear();
            output.shapes.len()
        };
        let alone = count([false; 3]);
        let each = [[true, false, false], [false, true, false], [false, false, true]].map(count);
        assert!(each.iter().all(|&n| n > alone));
        let extra: usize = each.iter().map(|n| n - alone).sum();
        assert_eq!(count([true; 3]) - alone, extra);
    }

    #[test]
    fn el_jazmin_abre_de_noche_y_al_compilar_bien() {
        let ctx = egui::Context::default();
        let theme = crate::theme::builtin().remove(0);
        let (leaf, accent) = (col(theme.success), col(theme.accent));
        let mut cat = Mascot::default();
        let frame = |cat: &mut Mascot, time: f64, busy: bool, ok: bool, jasmine: bool| {
            let input = egui::RawInput { time: Some(time), ..Default::default() };
            let mut output = ctx.run_ui(input, |ui| {
                cat.show(ui, 500.0, &theme, busy, ok, &mut [false, false, jasmine]);
            });
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::Shape::Rect(rect) => Some(rect.fill),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        // Sin él no hay hojas; de día solo tiene capullos.
        assert!(!frame(&mut cat, 1.0, false, false, false).contains(&leaf));
        let day = frame(&mut cat, 1.1, false, false, true);
        assert!(day.contains(&leaf) && !day.contains(&accent));
        // Una compilación correcta abre las flores un rato.
        frame(&mut cat, 2.0, true, false, true);
        assert!(frame(&mut cat, 2.1, false, true, true).contains(&accent));
        assert!(!frame(&mut cat, 2.2 + BLOOM_TIME, false, true, true).contains(&accent));
        // De noche, mientras el gatito duerme, vuelven a abrirse.
        assert!(frame(&mut cat, 3.0 + SLEEP_AFTER + BLOOM_TIME, false, true, true).contains(&accent));
        assert_eq!(cat.pose, Pose::Sleep);
    }

    #[test]
    fn deja_lo_que_hacia_para_teclear() {
        for pose in [Pose::Walk(0.1), Pose::Groom, Pose::Play, Pose::Stretch] {
            let mut cat = Mascot {
                pose,
                until: 100.0,
                ..Default::default()
            };
            cat.advance(1.0, 0.001, false, true);
            assert_eq!(cat.pose, Pose::Sit);
            // Y no se levanta mientras sigas escribiendo.
            cat.advance(50.0, 0.001, false, true);
            assert_eq!(cat.pose, Pose::Sit);
        }
    }
}
