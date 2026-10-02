use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub engine: String,
    pub autocompile: bool,
    pub autosave: bool,
    pub show_sidebar: bool,
    pub show_preview: bool,
    /// El panel de archivos va a la derecha en vez de a la izquierda.
    pub sidebar_right: bool,
    /// La vista previa va a la izquierda en vez de a la derecha.
    pub preview_left: bool,
    pub soft_wrap: bool,
    pub invert_preview: bool,
    pub background: String,
    pub background_style: String,
    pub background_intensity: f64,
    pub background_palette: bool,
    pub background_pixels: bool,
    pub background_dot: f64,
    pub text_shadow: f64,
    pub font: String,
    pub font_index: usize,
    pub font_size: f64,
    pub ui_font_size: f64,
    pub line_height: f64,
    pub line_numbers: bool,
    pub highlight_line: bool,
    /// Líneas verticales que marcan los niveles de sangría del código.
    pub indent_guides: bool,
    pub tab_width: usize,
    pub auto_pairs: bool,
    pub completions: bool,
    pub corner_radius: f64,
    /// El gatito que vive sobre la barra de estado.
    pub mascot: bool,
    /// El cangrejito que acompaña al gatito.
    pub mascot_friend: bool,
    /// El schnauzer que acompaña al gatito.
    pub mascot_dog: bool,
    pub autocompile_delay: f64,
    pub color_primary: String,
    pub color_secondary: String,
    pub color_accent: String,
    pub color_bg: String,
    pub color_fg: String,
    /// Sin argumentos, abrir el proyecto y las pestañas de la última vez.
    pub restore_session: bool,
    pub session_project: String,
    pub session_files: Vec<String>,
    pub session_active: String,
    pub recent_projects: Vec<String>,
    pub spellcheck: bool,
    /// Idioma del diccionario, como lo nombra el sistema: `es`, `en`, `es_MX`.
    pub spell_language: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: crate::theme::DEFAULT_THEME.into(),
            engine: "auto".into(),
            autocompile: true,
            autosave: true,
            show_sidebar: true,
            show_preview: true,
            sidebar_right: false,
            preview_left: false,
            soft_wrap: true,
            invert_preview: false,
            background: String::new(),
            background_style: "dither".into(),
            background_intensity: 0.7,
            background_palette: true,
            background_pixels: false,
            background_dot: 2.0,
            text_shadow: 0.8,
            font: String::new(),
            font_index: 0,
            font_size: 16.0,
            ui_font_size: 15.0,
            line_height: 1.0,
            line_numbers: true,
            highlight_line: false,
            indent_guides: true,
            tab_width: 4,
            auto_pairs: true,
            completions: true,
            corner_radius: 0.0,
            mascot: true,
            mascot_friend: true,
            mascot_dog: true,
            autocompile_delay: 1.2,
            color_primary: String::new(),
            color_secondary: String::new(),
            color_accent: String::new(),
            color_bg: String::new(),
            color_fg: String::new(),
            restore_session: true,
            session_project: String::new(),
            session_files: Vec::new(),
            session_active: String::new(),
            recent_projects: Vec::new(),
            spellcheck: true,
            spell_language: "es".into(),
            extra: BTreeMap::new(),
        }
    }
}

pub fn directory() -> PathBuf {
    // Las pruebas no leen ni escriben las preferencias reales.
    #[cfg(test)]
    return std::env::temp_dir().join(format!("miyu-config-{}", std::process::id()));
    #[cfg(not(test))]
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        })
        .join("miyulatex")
}

pub fn clean_path(raw: &str) -> PathBuf {
    let mut text = raw.trim();
    if text.len() >= 2
        && ((text.starts_with('\'') && text.ends_with('\''))
            || (text.starts_with('"') && text.ends_with('"')))
    {
        text = &text[1..text.len() - 1];
    }
    let text = text
        .strip_prefix("file://")
        .unwrap_or(text)
        .replace("\\ ", " ");
    if text == "~" || text.starts_with("~/") {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(text.strip_prefix("~/").unwrap_or(""))
    } else {
        PathBuf::from(text)
    }
}

/// Escribe junto al destino y renombra solo después de completar la escritura.
pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        if let Ok(meta) = fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

impl Config {
    pub fn load() -> Self {
        let Ok(data) = fs::read(directory().join("config.json")) else {
            return Self::default();
        };
        let Ok(Value::Object(raw)) = serde_json::from_slice::<Value>(&data) else {
            return Self::default();
        };
        let mut value = serde_json::to_value(Self::default()).unwrap();
        let values = value.as_object_mut().unwrap();
        for (key, v) in raw {
            let valid = values.get(&key).is_none_or(|d| match d {
                Value::String(_) => v.is_string(),
                Value::Bool(_) => v.is_boolean(),
                Value::Number(_) => v.is_number(),
                Value::Array(_) => v
                    .as_array()
                    .is_some_and(|list| list.iter().all(Value::is_string)),
                _ => true,
            });
            if valid {
                values.insert(key, v);
            }
        }
        // Un entero mal escrito no debe descartar el resto de las preferencias.
        for (key, default) in [("tab_width", 4), ("font_index", 0)] {
            if values[key].as_u64().is_none() {
                values.insert(key.into(), Value::from(default));
            }
        }
        let mut config: Self = serde_json::from_value(value).unwrap_or_default();
        config.background_intensity = config.background_intensity.clamp(0.25, 1.0);
        config.clamp();
        if config.background_style != "plain" {
            config.background_style = "dither".into();
        }
        config
    }

    /// Mantiene las opciones de personalización dentro de los rangos de Preferencias.
    pub fn clamp(&mut self) {
        let range = |value: &mut f64, low: f64, high: f64, default: f64| {
            *value = if value.is_finite() {
                value.clamp(low, high)
            } else {
                default
            };
        };
        range(&mut self.background_dot, 1.0, 6.0, 2.0);
        range(&mut self.text_shadow, 0.0, 1.0, 0.8);
        range(&mut self.font_size, 10.0, 32.0, 16.0);
        range(&mut self.ui_font_size, 11.0, 22.0, 15.0);
        range(&mut self.line_height, 1.0, 2.0, 1.0);
        range(&mut self.corner_radius, 0.0, 12.0, 0.0);
        range(&mut self.autocompile_delay, 0.3, 10.0, 1.2);
        self.tab_width = self.tab_width.clamp(1, 8);
        self.font_index = self.font_index.min(255);
    }

    pub fn save(&self) -> io::Result<()> {
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        atomic_write(&directory().join("config.json"), &bytes)
    }
}
