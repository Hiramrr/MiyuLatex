//! Logos de archivos incluidos en una textura, con variantes para temas claros.

use std::{collections::HashMap, path::Path, sync::OnceLock};

use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, vec2};
use serde::Deserialize;

use crate::theme::{Theme, col};

#[derive(Deserialize)]
struct Associations {
    extensions: HashMap<String, usize>,
    names: HashMap<String, usize>,
}

#[derive(Deserialize)]
struct Catalog {
    columns: usize,
    rows: usize,
    file: usize,
    #[serde(flatten)]
    base: Associations,
    light: Associations,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/file-icons.json"))
            .expect("Catálogo de iconos incluido en la aplicación")
    })
}

fn icon(path: &Path, dark: bool) -> usize {
    let catalog = catalog();
    let name = path.to_string_lossy().replace('\\', "/").to_lowercase();
    // El nombre manda sobre la extensión, incluso .github/funding.yml.
    let mut filename = name.as_str();
    loop {
        let named = (!dark)
            .then(|| catalog.light.names.get(filename))
            .flatten()
            .or_else(|| catalog.base.names.get(filename));
        if let Some(index) = named {
            return *index;
        }
        let Some((_, rest)) = filename.split_once('/') else {
            break;
        };
        filename = rest;
    }
    // Empieza por el sufijo más largo: d.ts, test.js, tar.gz, etc.
    let mut suffix = filename;
    while let Some((_, extension)) = suffix.split_once('.') {
        let found = (!dark)
            .then(|| catalog.light.extensions.get(extension))
            .flatten()
            .or_else(|| catalog.base.extensions.get(extension));
        if let Some(index) = found {
            return *index;
        }
        suffix = extension;
    }
    catalog.file
}

fn uv(index: usize) -> Rect {
    let catalog = catalog();
    let x = (index % catalog.columns) as f32 / catalog.columns as f32;
    let y = (index / catalog.columns) as f32 / catalog.rows as f32;
    Rect::from_min_size(
        egui::pos2(x, y),
        vec2(1.0 / catalog.columns as f32, 1.0 / catalog.rows as f32),
    )
}

/// Dibuja el logo en 16 × 16 puntos. egui conserva la textura entre cuadros.
pub fn file(painter: &Painter, center: Pos2, path: &Path, theme: &Theme) {
    let rect = Rect::from_center_size(center, vec2(16.0, 16.0));
    if let Ok(egui::load::TexturePoll::Ready { texture }) =
        egui::include_image!("../assets/file-icons.png").load(
            painter.ctx(),
            egui::TextureOptions::LINEAR,
            egui::load::SizeHint::default(),
        )
    {
        painter.image(texture.id, rect, uv(icon(path, theme.dark)), Color32::WHITE);
    } else {
        painter.rect_stroke(
            rect.shrink2(vec2(3.0, 1.0)),
            1.0,
            Stroke::new(1.2, col(theme.muted())),
            StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logos_by_name_extension_and_theme() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../assets/file-icons.json")).unwrap();
        for (name, expected) in [
            ("main.pdf", "pdf"),
            ("INFORME.PDF", "pdf"),
            ("Main.java", "java"),
            ("main.py", "python"),
            ("SCRIPT.PYW", "python"),
            ("main.rs", "rust"),
            ("Cargo.toml", "rust"),
            ("Cargo.lock", "rust"),
            ("main.tex", "tex"),
            ("Tesis.TEX", "tex"),
            ("refs.bib", "bibliography"),
            ("foto.JPG", "image"),
            ("README.md", "readme"),
            ("notas.md", "markdown"),
            ("main.js", "javascript"),
            ("main.ts", "typescript"),
            ("types.d.ts", "typescript-def"),
            ("App.jsx", "react"),
            ("App.tsx", "react_ts"),
            ("App.vue", "vue"),
            ("main.go", "go"),
            ("main.c", "c"),
            ("main.cpp", "cpp"),
            ("main.rb", "ruby"),
            ("Gemfile", "gemfile"),
            ("tema.css", "css"),
            ("build.sh", "console"),
            ("datos.json", "json"),
            ("datos.yaml", "yaml"),
            ("settings.toml", "toml"),
            ("Dockerfile", "docker"),
            ("Makefile", "makefile"),
            ("CMakeLists.txt", "cmake"),
            ("datos.csv", "table"),
            ("notas.txt", "document"),
            ("LICENSE", "license"),
            ("desconocido.xyzabc", "file"),
            ("sin_extension", "file"),
        ] {
            assert_eq!(
                data["icons"][icon(Path::new(name), true)],
                expected,
                "{name}"
            );
        }
        assert_ne!(
            icon(Path::new("main.rs"), true),
            icon(Path::new("main.py"), true)
        );
        assert_ne!(
            icon(Path::new("settings.toml"), true),
            icon(Path::new("settings.toml"), false)
        );
        for (dark, associations) in [(true, &catalog().base), (false, &catalog().light)] {
            for (name, index) in &associations.names {
                assert_eq!(icon(Path::new(name), dark), *index, "{name}");
            }
            for (extension, index) in &associations.extensions {
                assert_eq!(
                    icon(Path::new(&format!("file.{extension}")), dark),
                    *index,
                    "{extension}"
                );
            }
        }
    }

    #[test]
    fn embedded_atlas_matches_catalog_and_loads() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../assets/file-icons.json")).unwrap();
        let cell = data["cell"].as_u64().unwrap() as u32;
        let image = image::load_from_memory(include_bytes!("../assets/file-icons.png")).unwrap();
        assert_eq!(image.width(), catalog().columns as u32 * cell);
        assert_eq!(image.height(), catalog().rows as u32 * cell);
        for index in 0..data["icons"].as_array().unwrap().len() {
            assert!(Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)).contains_rect(uv(index)));
            let x = (index % catalog().columns) as u32 * cell;
            let y = (index / catalog().columns) as u32 * cell;
            assert!(
                image
                    .crop_imm(x, y, cell, cell)
                    .to_rgba8()
                    .pixels()
                    .any(|pixel| pixel[3] > 0)
            );
        }
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let draw = || {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(100.0, 100.0))),
                    ..Default::default()
                },
                |ui| {
                    file(
                        ui.painter(),
                        egui::pos2(20.0, 20.0),
                        Path::new("main.py"),
                        &crate::theme::builtin()[0],
                    )
                },
            )
        };
        let mut painted = false;
        let mut uploaded = false;
        // El cargador de egui decodifica las imágenes en otro hilo.
        for _ in 0..200 {
            let mut output = draw();
            uploaded |= output.textures_delta.set.values().flatten().any(|delta| {
                delta.image.size() == [image.width() as usize, image.height() as usize]
            });
            output.textures_delta.clear();
            painted = output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Mesh(_)));
            if painted {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            painted && uploaded,
            "El árbol debe dibujar el logo desde la textura incluida"
        );
        let mut cached = draw();
        let uploads = cached.textures_delta.set.len();
        cached.textures_delta.clear();
        assert_eq!(uploads, 0, "La textura se carga una sola vez");
    }
}
