//! Personalización: fuentes, colores propios y sus controles en Preferencias.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

use crate::{
    config::Config,
    theme::{self, Rgb, Theme},
};

const DEFAULT_CODE: &str = "/System/Library/Fonts/SFNSMono.ttf";

pub fn parse_color(text: &str) -> Option<Rgb> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok().map(theme::hex)
}

fn luminance(c: Rgb) -> f32 {
    (0.2126 * c.0 as f32 + 0.7152 * c.1 as f32 + 0.0722 * c.2 as f32) / 255.0
}

/// Aplica los colores propios sobre el tema elegido o el de la foto.
pub fn apply_colors(theme: &mut Theme, config: &Config) {
    let background = parse_color(&config.color_bg);
    if let Some(bg) = background {
        let dark = luminance(bg) < 0.5;
        // Un fondo de la otra claridad necesita texto y avisos legibles.
        if dark != theme.dark
            && let Some(base) = theme::builtin().into_iter().find(|t| t.dark == dark)
        {
            theme.fg = base.fg;
            theme.warning = base.warning;
            theme.error = base.error;
            theme.success = base.success;
        }
        theme.dark = dark;
        theme.bg = bg;
    }
    if let Some(fg) = parse_color(&config.color_fg) {
        theme.fg = fg;
    }
    if background.is_some() {
        theme.surface = theme::mix(theme.bg, theme.fg, 0.04);
        theme.panel = theme::mix(theme.bg, theme.fg, 0.09);
        theme.border = theme::mix(theme.bg, theme.fg, 0.18);
    }
    for (slot, value) in [
        (&mut theme.primary, &config.color_primary),
        (&mut theme.secondary, &config.color_secondary),
        (&mut theme.accent, &config.color_accent),
    ] {
        if let Some(color) = parse_color(value) {
            *slot = color;
        }
    }
}

/// Una cara tipográfica de un archivo de fuentes, con los nombres de su tabla `name`.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    pub index: usize,
    pub mono: bool,
}

/// Preferencia por la cara normal de una familia: Regular antes que Bold o Italic.
fn regularity(style: &str) -> usize {
    let lower = style.to_lowercase();
    if let Some(i) = ["regular", "roman", "book", "normal", "plain", "medium"]
        .iter()
        .position(|name| lower == *name)
    {
        return 10 - i;
    }
    let styled = [
        "bold",
        "italic",
        "oblique",
        "light",
        "thin",
        "black",
        "heavy",
        "condensed",
    ];
    usize::from(!styled.iter().any(|word| lower.contains(word)))
}

/// Caras de un TTF, OTF o TTC. Lee solo las cabeceras y las tablas `name` y `post`,
/// porque las fuentes CJK del sistema pesan decenas de MiB.
pub fn faces(path: &Path) -> io::Result<Vec<Face>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let mut read = |offset: u64, length: usize| -> io::Result<Vec<u8>> {
        if length > 1 << 20 {
            return Err(io::Error::other("tabla demasiado grande"));
        }
        let mut buffer = vec![0; length];
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    };
    let short = |bytes: &[u8], at: usize| {
        bytes
            .get(at..at + 2)
            .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]) as usize)
    };
    let long = |bytes: &[u8], at: usize| {
        bytes
            .get(at..at + 4)
            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64)
    };
    let header = read(0, 12)?;
    let offsets: Vec<u64> = if &header[..4] == b"ttcf" {
        let count = (long(&header, 8) as usize).min(256);
        let table = read(12, count * 4)?;
        (0..count).map(|i| long(&table, i * 4)).collect()
    } else {
        vec![0]
    };
    let mut found = Vec::new();
    for (index, start) in offsets.into_iter().enumerate() {
        let count = short(&read(start, 12)?, 4).min(512);
        let directory = read(start + 12, count * 16)?;
        let table = |tag: &[u8; 4]| {
            directory
                .as_chunks::<16>()
                .0
                .iter()
                .find(|record| &record[..4] == tag)
                .map(|record| (long(record, 8), long(record, 12) as usize))
        };
        let Some((offset, length)) = table(b"name") else {
            continue;
        };
        let names = read(offset, length)?;
        let strings = short(&names, 4);
        // Familia, estilo y sus variantes tipográficas (16 y 17), con el mejor idioma visto.
        let mut best: [(u8, String); 4] = Default::default();
        let records = names.get(6..6 + short(&names, 2) * 12).unwrap_or_default();
        for record in records.as_chunks::<12>().0 {
            let slot = match short(record, 6) {
                1 => 0,
                2 => 1,
                16 => 2,
                17 => 3,
                _ => continue,
            };
            let from = strings + short(record, 10);
            let Some(bytes) = names.get(from..from + short(record, 8)) else {
                continue;
            };
            let (platform, language) = (short(record, 0), short(record, 4));
            let (rank, text) = match platform {
                0 | 3 => {
                    let units: Vec<_> = bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| u16::from_be_bytes([b[0], b[1]]))
                        .collect();
                    (
                        if platform == 0 || language == 0x409 {
                            3
                        } else {
                            1
                        },
                        String::from_utf16_lossy(&units),
                    )
                }
                1 if language == 0 => (2, bytes.iter().map(|&b| b as char).collect()),
                _ => continue,
            };
            if rank > best[slot].0 {
                best[slot] = (rank, text);
            }
        }
        let [family, style, wide_family, wide_style] = best.map(|(_, text)| text);
        let family = if wide_family.is_empty() {
            family
        } else {
            wide_family
        };
        if family.is_empty() {
            continue;
        }
        // Monaco y Courier no declaran el paso fijo en `post`.
        let lower = family.to_lowercase();
        let mono = ["mono", "courier"].iter().any(|word| lower.contains(word))
            || match table(b"post") {
                Some((offset, length)) if length >= 16 => long(&read(offset, 16)?, 12) != 0,
                _ => false,
            };
        found.push(Face {
            family,
            style: if wide_style.is_empty() {
                style
            } else {
                wide_style
            },
            path: path.into(),
            index,
            mono,
        });
    }
    Ok(found)
}

/// La cara normal de cada familia de `faces`.
fn families(faces: impl IntoIterator<Item = Face>) -> Vec<Face> {
    let mut found: BTreeMap<String, Face> = BTreeMap::new();
    for face in faces {
        // Las familias con punto inicial son internas del sistema.
        if face.family.starts_with('.') {
            continue;
        }
        match found.entry(face.family.to_lowercase()) {
            std::collections::btree_map::Entry::Occupied(mut current) => {
                if regularity(&face.style) > regularity(&current.get().style) {
                    current.insert(face);
                }
            }
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(face);
            }
        }
    }
    found.into_values().collect()
}

/// Familias instaladas en el equipo, por nombre.
pub fn fonts() -> &'static [Face] {
    static FOUND: OnceLock<Vec<Face>> = OnceLock::new();
    FOUND.get_or_init(|| {
        fn walk(folder: &Path, depth: usize, out: &mut Vec<Face>) {
            let Ok(entries) = fs::read_dir(folder) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if depth < 3 {
                        walk(&path, depth + 1, out);
                    }
                    continue;
                }
                let extension = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if ["ttf", "otf", "ttc", "otc"].contains(&extension.as_str())
                    && let Ok(faces) = faces(&path)
                {
                    out.extend(faces);
                }
            }
        }
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        let mut folders = vec![
            home.join("Library/Fonts"),
            "/Library/Fonts".into(),
            "/System/Library/Fonts".into(),
            home.join(".local/share/fonts"),
            home.join(".fonts"),
            "/usr/share/fonts".into(),
            "/usr/local/share/fonts".into(),
            "C:\\Windows\\Fonts".into(),
        ];
        // Fuentes que macOS descarga a petición, como las de Catálogo Tipográfico.
        if let Ok(entries) = fs::read_dir("/System/Library/AssetsV2") {
            folders.extend(
                entries
                    .flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .starts_with("com_apple_MobileAsset_Font")
                    })
                    .map(|e| e.path()),
            );
        }
        let mut found = Vec::new();
        for folder in folders {
            walk(&folder, 0, &mut found);
        }
        families(found)
    })
}

pub fn load_font(path: &Path, index: usize) -> Result<FontData, String> {
    let mut data = FontData::from_owned(fs::read(path).map_err(|e| e.to_string())?);
    data.index = index as u32;
    let mut probe = FontDefinitions::empty();
    probe
        .font_data
        .insert("probe".into(), Arc::new(data.clone()));
    // epaint entra en pánico con una fuente ilegible: se prueba antes de instalarla.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        egui::epaint::text::FontsImpl::new(Default::default(), probe);
    }))
    .map_err(|_| "el archivo no es una fuente TTF u OTF válida".to_string())?;
    Ok(data)
}

/// Instala las fuentes del sistema y la elegida en Preferencias. Solo actúa si cambió.
pub fn install_fonts(ctx: &egui::Context, config: &Config) -> Result<(), String> {
    let id = egui::Id::new("miyu-font");
    let key = (config.font.clone(), config.font_index);
    if ctx.data(|d| d.get_temp::<(String, usize)>(id)).as_ref() == Some(&key) {
        return Ok(());
    }
    ctx.data_mut(|d| d.insert_temp(id, key));
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, data: FontData, family: FontFamily| {
        fonts.font_data.insert(name.into(), Arc::new(data));
        fonts
            .families
            .get_mut(&family)
            .unwrap()
            .insert(0, name.into());
    };
    for (name, path, family) in [
        (
            "system",
            "/System/Library/Fonts/SFNS.ttf",
            FontFamily::Proportional,
        ),
        ("code", DEFAULT_CODE, FontFamily::Monospace),
    ] {
        if let Ok(data) = fs::read(path) {
            add(name, FontData::from_owned(data), family);
        }
    }
    let mut result = Ok(());
    if !config.font.is_empty() {
        match load_font(Path::new(&config.font), config.font_index) {
            Ok(data) => add("custom", data, FontFamily::Monospace),
            Err(e) => result = Err(e),
        }
    }
    ctx.set_fonts(fonts);
    result
}

/// Un deslizador que solo pide guardar al soltarlo; el valor cambia mientras se arrastra.
fn slider(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> bool {
    let response = ui.add(slider);
    response.drag_stopped() || (response.changed() && !response.dragged())
}

/// Secciones de personalización de Preferencias. Devuelve si hay que aplicar y guardar.
pub fn preferences(
    ui: &mut egui::Ui,
    config: &mut Config,
    theme: &Theme,
    message: &mut String,
) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new("Editor").show(ui, |ui| {
        let mut pick = None;
        ui.horizontal_wrapped(|ui| {
            ui.label("Fuente");
            let current = if config.font.is_empty() {
                "Predeterminada".to_string()
            } else {
                fonts()
                    .iter()
                    .find(|f| f.path == Path::new(&config.font) && f.index == config.font_index)
                    .map(|f| f.family.clone())
                    .unwrap_or_else(|| {
                        Path::new(&config.font)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
            };
            let id = egui::Id::new("font-filter");
            let (mut search, mut mono): (String, bool) =
                ui.data(|d| d.get_temp(id)).unwrap_or_default();
            egui::ComboBox::from_id_salt("font")
                .selected_text(current)
                .width(230.0)
                .height(320.0)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show_ui(ui, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut search)
                            .hint_text("Buscar fuente")
                            .desired_width(f32::INFINITY),
                    );
                    ui.checkbox(&mut mono, "Solo monoespaciadas");
                    ui.separator();
                    if ui
                        .selectable_label(config.font.is_empty(), "Predeterminada")
                        .clicked()
                    {
                        config.font.clear();
                        config.font_index = 0;
                        changed = true;
                        ui.close();
                    }
                    let query = search.to_lowercase();
                    for face in fonts() {
                        if (mono && !face.mono) || !face.family.to_lowercase().contains(&query) {
                            continue;
                        }
                        let selected =
                            face.path == Path::new(&config.font) && face.index == config.font_index;
                        if ui.selectable_label(selected, &face.family).clicked() {
                            pick = Some((face.path.clone(), face.index));
                            ui.close();
                        }
                    }
                });
            ui.data_mut(|d| d.insert_temp(id, (search, mono)));
            if ui
                .button("Cargar fuente…")
                .on_hover_text(
                    "Elige una fuente TTF, OTF o una colección de fuentes para el editor.",
                )
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .set_title("Elegir fuente")
                    .add_filter("Fuentes", &["ttf", "otf", "ttc", "otc"])
                    .pick_file()
            {
                let index = families(faces(&path).unwrap_or_default())
                    .first()
                    .map_or(0, |face| face.index);
                pick = Some((path, index));
            }
        });
        if let Some((path, index)) = pick {
            match load_font(&path, index) {
                Ok(_) => {
                    config.font = path.to_string_lossy().into_owned();
                    config.font_index = index;
                    changed = true;
                }
                Err(e) => *message = format!("No pude cargar la fuente: {e}"),
            }
        }
        changed |= slider(
            ui,
            egui::Slider::new(&mut config.font_size, 10.0..=32.0)
                .step_by(1.0)
                .text("Tamaño del código"),
        );
        changed |= slider(
            ui,
            egui::Slider::new(&mut config.line_height, 1.0..=2.0)
                .step_by(0.05)
                .text("Interlineado"),
        );
        changed |= slider(
            ui,
            egui::Slider::new(&mut config.tab_width, 1..=8).text("Espacios por sangría"),
        );
        changed |= ui
            .checkbox(&mut config.line_numbers, "Números de línea")
            .changed();
        changed |= ui
            .checkbox(&mut config.highlight_line, "Resaltar la línea actual")
            .changed();
        changed |= ui
            .checkbox(&mut config.indent_guides, "Guías de sangría en el código")
            .changed();
        changed |= ui
            .checkbox(&mut config.auto_pairs, "Cerrar llaves, corchetes y $")
            .changed();
        changed |= ui
            .checkbox(
                &mut config.completions,
                "Sugerir comandos y palabras al escribir",
            )
            .changed();
        ui.add_enabled_ui(config.autocompile, |ui| {
            changed |= slider(
                ui,
                egui::Slider::new(&mut config.autocompile_delay, 0.3..=10.0)
                    .step_by(0.1)
                    .suffix(" s")
                    .text("Espera para compilar"),
            );
        });
    });
    egui::CollapsingHeader::new("Apariencia").show(ui, |ui| {
        changed |= slider(
            ui,
            egui::Slider::new(&mut config.ui_font_size, 11.0..=22.0)
                .step_by(1.0)
                .text("Tamaño de la interfaz"),
        );
        changed |= slider(
            ui,
            egui::Slider::new(&mut config.corner_radius, 0.0..=12.0)
                .step_by(1.0)
                .text("Esquinas redondeadas"),
        );
        changed |= ui
            .checkbox(&mut config.mascot, "Gatito en la barra de estado")
            .changed();
        if !config.background.is_empty() {
            changed |= slider(
                ui,
                egui::Slider::new(&mut config.background_dot, 1.0..=6.0)
                    .step_by(1.0)
                    .text("Punto del tramado"),
            );
            changed |= slider(
                ui,
                egui::Slider::new(&mut config.text_shadow, 0.0..=1.0)
                    .step_by(0.05)
                    .text("Sombra bajo el texto"),
            );
        }
        ui.label("Colores propios");
        let mut custom = false;
        for (label, slot, current) in [
            ("Primario", &mut config.color_primary, theme.primary),
            ("Secundario", &mut config.color_secondary, theme.secondary),
            ("Acento", &mut config.color_accent, theme.accent),
            ("Fondo", &mut config.color_bg, theme.bg),
            ("Texto", &mut config.color_fg, theme.fg),
        ] {
            ui.push_id(label, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let mut rgb = [current.0, current.1, current.2];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        *slot = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
                        changed = true;
                    }
                    ui.label(label);
                    if !slot.is_empty() {
                        custom = true;
                        if ui
                            .button("Usar color del tema")
                            .on_hover_text(format!(
                                "Restaura el color {label} del tema seleccionado."
                            ))
                            .clicked()
                        {
                            slot.clear();
                            changed = true;
                        }
                    }
                });
            });
        }
        if custom
            && ui
                .button("Restaurar todos los colores del tema")
                .on_hover_text(
                    "Quita todos los colores propios y vuelve a los colores del tema seleccionado.",
                )
                .clicked()
        {
            for slot in [
                &mut config.color_primary,
                &mut config.color_secondary,
                &mut config.color_accent,
                &mut config.color_bg,
                &mut config.color_fg,
            ] {
                slot.clear();
            }
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_fonts_and_ranges() {
        assert_eq!(parse_color("#FF8fb7"), Some((255, 143, 183)));
        assert_eq!(parse_color("16131f"), Some((22, 19, 31)));
        for bad in ["", "#fff", "+12345", "#gggggg", "#1234567"] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
        let base = theme::builtin().remove(0);
        let mut config = Config::default();
        let mut theme = base.clone();
        apply_colors(&mut theme, &config);
        assert_eq!(theme, base);
        config.color_primary = "#102030".into();
        config.color_bg = "#fafafa".into();
        apply_colors(&mut theme, &config);
        assert_eq!(theme.primary, (16, 32, 48));
        assert!(!theme.dark);
        // El texto del tema oscuro no se leería sobre un fondo claro.
        assert!(luminance(theme.fg) < 0.4);
        assert_ne!(theme.surface, base.surface);
        config.color_fg = "#000000".into();
        apply_colors(&mut theme, &config);
        assert_eq!(theme.fg, (0, 0, 0));

        let folder = std::env::temp_dir().join(format!("miyu-font-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let broken = folder.join("rota.ttf");
        fs::write(&broken, b"esto no es una fuente").unwrap();
        assert!(load_font(&broken, 0).is_err());
        assert!(load_font(&folder.join("no-existe.ttf"), 0).is_err());
        assert!(faces(&broken).unwrap_or_default().is_empty());
        let ctx = egui::Context::default();
        config.font = broken.to_string_lossy().into_owned();
        assert!(install_fonts(&ctx, &config).is_err());
        config.font.clear();
        assert!(install_fonts(&ctx, &config).is_ok());

        // La cara normal gana dentro de una familia y las internas no se listan.
        let face = |family: &str, style: &str, index| Face {
            family: family.into(),
            style: style.into(),
            path: "x.ttc".into(),
            index,
            mono: false,
        };
        let chosen = families([
            face("Menlo", "Bold", 0),
            face("Menlo", "Regular", 1),
            face("Menlo", "Italic", 2),
            face(".SF NS", "Regular", 0),
            face("Avenir", "Heavy", 0),
            face("Avenir", "Book", 3),
        ]);
        assert_eq!(
            chosen
                .iter()
                .map(|f| (f.family.as_str(), f.index))
                .collect::<Vec<_>>(),
            [("Avenir", 3), ("Menlo", 1)]
        );
        if cfg!(target_os = "macos") {
            let started = std::time::Instant::now();
            let installed = fonts();
            assert!(started.elapsed().as_secs() < 5);
            for name in ["Menlo", "Helvetica Neue", "Georgia", "Times New Roman"] {
                let face = installed
                    .iter()
                    .find(|f| f.family == name)
                    .unwrap_or_else(|| panic!("falta {name}"));
                assert_eq!(regularity(&face.style), 10, "{name}: {}", face.style);
                assert_eq!(face.mono, name == "Menlo");
                assert!(load_font(&face.path, face.index).is_ok(), "{name}");
            }
        }
        fs::remove_dir_all(folder).unwrap();

        config.font_size = 400.0;
        config.line_height = f64::NAN;
        config.tab_width = 0;
        config.clamp();
        assert_eq!(
            (config.font_size, config.line_height, config.tab_width),
            (32.0, 1.0, 1)
        );
    }
}
