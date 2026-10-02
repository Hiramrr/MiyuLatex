//! Temas de color y estilos de sintaxis.

use eframe::egui::{Color32, FontId, Stroke, TextFormat};

use crate::highlight::Tok;

pub type Rgb = (u8, u8, u8);

pub const PHOTO_THEME: &str = "miyu-foto";
pub const DEFAULT_THEME: &str = "miyu-noche";

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    pub primary: Rgb,
    pub secondary: Rgb,
    pub accent: Rgb,
    pub warning: Rgb,
    pub error: Rgb,
    pub success: Rgb,
    pub fg: Rgb,
    pub bg: Rgb,
    pub surface: Rgb,
    pub panel: Rgb,
    pub border: Rgb,
}

pub const fn hex(value: u32) -> Rgb {
    ((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub fn col(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

/// Mezcla `a` hacia `b`; `t = 0` es `a`, `t = 1` es `b`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| {
        (x as f32 + (y as f32 - x as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

pub fn hsl(h: f32, s: f32, l: f32) -> Rgb {
    let h = h.rem_euclid(360.0);
    let f = |n: f32| {
        let k = (n + h / 30.0) % 12.0;
        let v = l - s * l.min(1.0 - l) * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    (f(0.0), f(8.0), f(4.0))
}

#[allow(clippy::too_many_arguments)]
fn theme(
    name: &str,
    dark: bool,
    primary: u32,
    secondary: u32,
    accent: u32,
    fg: u32,
    bg: u32,
    surface: u32,
    panel: u32,
    border: u32,
) -> Theme {
    let (warning, error, success) = if dark {
        (0xffd486, 0xff6b8b, 0x9be8a8)
    } else {
        (0xb7791f, 0xd63651, 0x2f9e58)
    };
    Theme {
        name: name.into(),
        dark,
        primary: hex(primary),
        secondary: hex(secondary),
        accent: hex(accent),
        warning: hex(warning),
        error: hex(error),
        success: hex(success),
        fg: hex(fg),
        bg: hex(bg),
        surface: hex(surface),
        panel: hex(panel),
        border: hex(border),
    }
}

pub fn builtin() -> Vec<Theme> {
    vec![
        theme(
            "miyu-noche",
            true,
            0xff8fb7,
            0xb69cff,
            0x7ee0d2,
            0xece6ff,
            0x16131f,
            0x1e1a2b,
            0x29233b,
            0x3a3254,
        ),
        theme(
            "miyu-dia", false, 0xd6457f, 0x7a55e0, 0x0e8f86, 0x3a2c45, 0xfdf7fa, 0xf7ebf2,
            0xefdde8, 0xe2c8d8,
        ),
        theme(
            "tokio-noche",
            true,
            0x7aa2f7,
            0xbb9af7,
            0x7dcfff,
            0xc0caf5,
            0x1a1b26,
            0x1f2335,
            0x292e42,
            0x3b4261,
        ),
        theme(
            "catppuccin",
            true,
            0xf5c2e7,
            0xcba6f7,
            0x94e2d5,
            0xcdd6f4,
            0x1e1e2e,
            0x242438,
            0x313244,
            0x45475a,
        ),
        theme(
            "gruvbox", true, 0xfabd2f, 0xd3869b, 0x8ec07c, 0xebdbb2, 0x1d2021, 0x282828, 0x3c3836,
            0x504945,
        ),
        theme(
            "nord", true, 0x88c0d0, 0xb48ead, 0xa3be8c, 0xd8dee9, 0x2e3440, 0x343b49, 0x3b4252,
            0x4c566a,
        ),
        theme(
            "rosa-pino",
            true,
            0xebbcba,
            0xc4a7e7,
            0x9ccfd8,
            0xe0def4,
            0x191724,
            0x1f1d2e,
            0x26233a,
            0x403d52,
        ),
        theme(
            "grafito", true, 0xd6d6a8, 0xb8c48a, 0x9fc5b0, 0xe6e6d8, 0x1b1c16, 0x22231c, 0x2c2e24,
            0x44463a,
        ),
        theme(
            "papel", false, 0x9a5b2e, 0x5b6ea8, 0x2f7d6d, 0x2f2a25, 0xfaf6ee, 0xf2ecdf, 0xe8dfcd,
            0xd6cab2,
        ),
    ]
}

/// Tema completo a partir del tono dominante de una foto: neutros teñidos y un acento saturado.
pub fn photo_theme(tone: (f32, f32), dark: bool) -> Theme {
    let (h, saturation) = tone;
    let sat = saturation.clamp(0.42, 0.75);
    if dark {
        Theme {
            name: PHOTO_THEME.into(),
            dark: true,
            primary: hsl(h, sat, 0.68),
            secondary: hsl(h + 35.0, sat * 0.75, 0.76),
            accent: hsl(h + 180.0, sat * 0.55, 0.72),
            warning: hex(0xffd486),
            error: hex(0xff6b8b),
            success: hex(0x9be8a8),
            fg: hsl(h, 0.18, 0.88),
            bg: hsl(h, 0.16, 0.085),
            surface: hsl(h, 0.14, 0.13),
            panel: hsl(h, 0.12, 0.19),
            border: hsl(h, 0.12, 0.30),
        }
    } else {
        Theme {
            name: PHOTO_THEME.into(),
            dark: false,
            primary: hsl(h, sat, 0.40),
            secondary: hsl(h + 35.0, sat * 0.85, 0.36),
            accent: hsl(h + 180.0, sat * 0.7, 0.32),
            warning: hex(0xb7791f),
            error: hex(0xd63651),
            success: hex(0x2f9e58),
            fg: hsl(h, 0.22, 0.17),
            bg: hsl(h, 0.30, 0.965),
            surface: hsl(h, 0.22, 0.925),
            panel: hsl(h, 0.16, 0.86),
            border: hsl(h, 0.14, 0.76),
        }
    }
}

impl Theme {
    pub fn muted(&self) -> Rgb {
        mix(self.fg, self.bg, 0.25)
    }

    pub fn selection(&self) -> Rgb {
        mix(self.bg, self.secondary, 0.32)
    }

    pub fn highlight(&self) -> Rgb {
        mix(self.bg, self.primary, 0.24)
    }

    pub fn syntax(&self, tok: Tok, size: f32) -> TextFormat {
        let color = match tok {
            Tok::Comment => self.muted(),
            Tok::Command => self.primary,
            Tok::Section | Tok::Env => self.secondary,
            Tok::Title => mix(self.secondary, self.fg, 0.45),
            Tok::Item | Tok::Ref => self.warning,
            Tok::EnvName | Tok::MathDelim => self.accent,
            Tok::Math => mix(self.accent, self.fg, 0.15),
            Tok::MathCommand => mix(self.success, self.accent, 0.35),
            Tok::Brace | Tok::Bracket => mix(self.fg, self.bg, 0.25),
            Tok::Special => self.error,
            Tok::Module => self.success,
            Tok::Str => mix(self.warning, self.fg, 0.3),
            Tok::Verbatim => mix(self.fg, self.warning, 0.25),
            _ => self.fg,
        };
        TextFormat {
            font_id: FontId::monospace(size),
            color: col(color),
            italics: matches!(tok, Tok::Comment | Tok::EnvName | Tok::Italic),
            underline: if tok == Tok::Underline {
                Stroke::new(1.0, col(color))
            } else {
                Stroke::NONE
            },
            ..Default::default()
        }
    }
}
