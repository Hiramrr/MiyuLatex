//! Iconos de la lista de archivos: uno por tipo, dibujados con trazos en los colores del tema.

use std::f32::consts::TAU;
use std::path::Path;

use eframe::egui::{self, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, vec2};

use crate::theme::{Theme, col};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Tex,
    Package,
    Drawing,
    Bibliography,
    Pdf,
    Image,
    Markdown,
    Code,
    Styles,
    Shell,
    Data,
    Settings,
    Table,
    Text,
}

fn kind(path: &Path) -> Kind {
    // El nombre manda sobre la extensión: `CMakeLists.txt` no es una nota.
    match path.file_name().and_then(|s| s.to_str()) {
        Some("Makefile" | "Dockerfile" | "CMakeLists.txt") => return Kind::Settings,
        Some("LICENSE") => return Kind::Text,
        _ => {}
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "tex" | "ltx" => Kind::Tex,
        "sty" | "cls" => Kind::Package,
        "tikz" | "svg" => Kind::Drawing,
        "bib" | "bst" => Kind::Bibliography,
        "pdf" => Kind::Pdf,
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => Kind::Image,
        "md" | "markdown" | "mdown" | "mkd" => Kind::Markdown,
        "css" | "scss" | "sass" | "less" => Kind::Styles,
        "sh" | "bash" | "zsh" | "fish" | "bat" | "ps1" => Kind::Shell,
        "json" | "yaml" | "yml" | "xml" => Kind::Data,
        "toml" | "ini" | "conf" | "cfg" => Kind::Settings,
        "csv" | "tsv" => Kind::Table,
        "txt" | "log" => Kind::Text,
        // Lo demás que llega a la lista es código que syntect reconoce.
        _ => Kind::Code,
    }
}

/// Dibuja el icono de `path` en una caja de 14 × 14 puntos centrada en `center`.
pub fn file(painter: &Painter, center: Pos2, path: &Path, theme: &Theme) {
    let kind = kind(path);
    let color = col(match kind {
        Kind::Tex | Kind::Package => theme.secondary,
        Kind::Bibliography | Kind::Data => theme.warning,
        Kind::Pdf => theme.error,
        Kind::Image | Kind::Shell | Kind::Table => theme.success,
        Kind::Markdown | Kind::Drawing | Kind::Styles => theme.accent,
        Kind::Code => theme.primary,
        Kind::Settings | Kind::Text => theme.muted(),
    });
    let stroke = Stroke::new(1.2, color);
    let soft = Stroke::new(1.0, color.gamma_multiply(0.7));
    let at = |(x, y): (f32, f32)| center + vec2(x, y);
    let points = |list: &[(f32, f32)]| list.iter().copied().map(at).collect::<Vec<_>>();
    let line = |list: &[(f32, f32)], stroke: Stroke| {
        painter.add(Shape::line(points(list), stroke));
    };
    let closed = |list: &[(f32, f32)]| {
        painter.add(Shape::closed_line(points(list), stroke));
    };
    let frame = |half: (f32, f32)| {
        painter.rect_stroke(
            Rect::from_center_size(center, vec2(half.0, half.1) * 2.0),
            1.5,
            stroke,
            StrokeKind::Inside,
        );
    };
    // Una hoja con la esquina doblada, desplazada `dx` puntos.
    let sheet = |dx: f32| {
        closed(&[
            (dx - 5.0, -6.5),
            (dx + 1.5, -6.5),
            (dx + 5.0, -3.0),
            (dx + 5.0, 6.5),
            (dx - 5.0, 6.5),
        ]);
    };
    match kind {
        // El logotipo de TeX: la E baja de la línea.
        Kind::Tex => {
            line(&[(-7.0, -4.0), (-2.0, -4.0)], stroke);
            line(&[(-4.5, -4.0), (-4.5, 3.0)], stroke);
            line(
                &[(1.0, -0.5), (-2.0, -0.5), (-2.0, 5.5), (1.0, 5.5)],
                stroke,
            );
            line(&[(-2.0, 2.5), (0.5, 2.5)], stroke);
            line(&[(2.8, -4.0), (6.8, 3.0)], stroke);
            line(&[(6.8, -4.0), (2.8, 3.0)], stroke);
        }
        // Un paquete: la caja vista desde una esquina.
        Kind::Package => {
            closed(&[
                (0.0, -6.5),
                (5.6, -3.2),
                (5.6, 3.2),
                (0.0, 6.5),
                (-5.6, 3.2),
                (-5.6, -3.2),
            ]);
            line(&[(-5.6, -3.2), (0.0, 0.0), (5.6, -3.2)], soft);
            line(&[(0.0, 0.0), (0.0, 6.5)], soft);
        }
        // Una curva con sus dos anclas, como en un programa de dibujo.
        Kind::Drawing => {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [(-4.5, 4.5), (-4.5, -5.0), (4.5, 5.0), (4.5, -4.5)].map(at),
                false,
                egui::Color32::TRANSPARENT,
                stroke,
            ));
            for anchor in [(-4.5, 4.5), (4.5, -4.5)] {
                painter.rect_filled(
                    Rect::from_center_size(at(anchor), vec2(3.6, 3.6)),
                    0.8,
                    color,
                );
            }
        }
        // Un libro abierto.
        Kind::Bibliography => {
            closed(&[
                (0.0, -4.0),
                (-6.5, -5.5),
                (-6.5, 4.0),
                (0.0, 5.5),
                (6.5, 4.0),
                (6.5, -5.5),
            ]);
            line(&[(0.0, -4.0), (0.0, 5.5)], stroke);
            for y in [-2.0, 0.8] {
                line(&[(-4.3, y - 0.6), (-2.2, y)], soft);
                line(&[(2.2, y), (4.3, y - 0.6)], soft);
            }
        }
        // La hoja con su etiqueta.
        Kind::Pdf => {
            sheet(1.5);
            painter.rect_filled(
                Rect::from_min_max(at((-7.0, 0.0)), at((3.5, 4.5))),
                1.0,
                color,
            );
        }
        // Un paisaje enmarcado.
        Kind::Image => {
            frame((6.5, 5.5));
            painter.circle_filled(at((-3.0, -2.2)), 1.3, color);
            line(
                &[(-5.3, 3.6), (-1.8, 0.2), (0.6, 2.4), (2.6, 0.2), (5.3, 3.2)],
                stroke,
            );
        }
        // La marca de Markdown: la M y la flecha.
        Kind::Markdown => {
            frame((7.0, 5.0));
            line(
                &[
                    (-4.6, 2.4),
                    (-4.6, -2.4),
                    (-2.5, 0.2),
                    (-0.4, -2.4),
                    (-0.4, 2.4),
                ],
                stroke,
            );
            line(&[(3.4, -2.4), (3.4, 2.2)], stroke);
            line(&[(1.6, 0.4), (3.4, 2.4), (5.2, 0.4)], stroke);
        }
        Kind::Code => {
            line(&[(-3.2, -3.5), (-6.5, 0.0), (-3.2, 3.5)], stroke);
            line(&[(3.2, -3.5), (6.5, 0.0), (3.2, 3.5)], stroke);
            line(&[(1.3, -5.0), (-1.3, 5.0)], soft);
        }
        Kind::Styles => {
            line(&[(-1.6, -5.5), (-3.0, 5.5)], stroke);
            line(&[(3.0, -5.5), (1.6, 5.5)], stroke);
            line(&[(-5.5, -2.0), (5.8, -2.0)], stroke);
            line(&[(-5.8, 2.0), (5.5, 2.0)], stroke);
        }
        // Una terminal con su indicador.
        Kind::Shell => {
            frame((7.0, 5.5));
            line(&[(-4.2, -2.2), (-1.8, 0.0), (-4.2, 2.2)], stroke);
            line(&[(0.4, 2.4), (4.0, 2.4)], stroke);
        }
        Kind::Data => {
            for side in [-1.0, 1.0] {
                line(
                    &[
                        (side * 2.2, -5.5),
                        (side * 3.8, -4.5),
                        (side * 3.8, -1.2),
                        (side * 5.8, 0.0),
                        (side * 3.8, 1.2),
                        (side * 3.8, 4.5),
                        (side * 2.2, 5.5),
                    ],
                    stroke,
                );
            }
            painter.circle_filled(center, 1.0, color.gamma_multiply(0.7));
        }
        // Un engranaje.
        Kind::Settings => {
            for tooth in 0..8 {
                let (sin, cos) = (tooth as f32 * TAU / 8.0).sin_cos();
                line(
                    &[(cos * 3.8, sin * 3.8), (cos * 6.0, sin * 6.0)],
                    Stroke::new(2.0, color),
                );
            }
            painter.circle_stroke(center, 3.6, Stroke::new(1.5, color));
        }
        Kind::Table => {
            frame((6.5, 5.5));
            line(&[(-6.0, -1.8), (6.0, -1.8)], stroke);
            line(&[(-6.0, 1.8), (6.0, 1.8)], soft);
            line(&[(-1.8, -1.8), (-1.8, 5.0)], soft);
        }
        Kind::Text => {
            sheet(0.0);
            for y in [-1.0, 2.0] {
                line(&[(-2.5, y), (2.5, y)], soft);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_name_and_extension() {
        for (name, expected) in [
            ("main.tex", Kind::Tex),
            ("Tesis.TEX", Kind::Tex),
            ("miyu.sty", Kind::Package),
            ("figura.tikz", Kind::Drawing),
            ("refs.bib", Kind::Bibliography),
            ("main.pdf", Kind::Pdf),
            ("foto.JPG", Kind::Image),
            ("README.md", Kind::Markdown),
            ("main.rs", Kind::Code),
            ("Gemfile", Kind::Code),
            ("tema.css", Kind::Styles),
            ("build.sh", Kind::Shell),
            ("datos.json", Kind::Data),
            ("Cargo.toml", Kind::Settings),
            ("Makefile", Kind::Settings),
            ("CMakeLists.txt", Kind::Settings),
            ("datos.csv", Kind::Table),
            ("notas.txt", Kind::Text),
            ("LICENSE", Kind::Text),
        ] {
            assert_eq!(kind(Path::new(name)), expected, "{name}");
        }
    }
}
