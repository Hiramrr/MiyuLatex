//! Ortografía con diccionarios Hunspell, para los sistemas sin corrector
//! propio. En macOS se usa el del sistema y esto solo corre en las pruebas.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use spellbook::Dictionary;

/// Carpetas donde las distribuciones instalan los diccionarios, tras la de Miyu.
const SYSTEM: &[&str] = &[
    "/usr/share/hunspell",
    "/usr/share/myspell",
    "/usr/share/myspell/dicts",
    "/usr/local/share/hunspell",
];

pub struct Checker {
    folders: Vec<PathBuf>,
    /// Diccionarios ya leídos; `None` si el idioma no se pudo cargar.
    dictionaries: HashMap<String, Option<Dictionary>>,
    learned: HashSet<String>,
    /// Archivo donde se guardan las palabras aprendidas.
    words: Option<PathBuf>,
}

/// El archivo puede estar en UTF-8 o, en diccionarios antiguos, en Latin-1.
fn read(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => e.into_bytes().iter().map(|b| *b as char).collect(),
    })
}

impl Checker {
    pub fn new(folders: Vec<PathBuf>, words: Option<PathBuf>) -> Self {
        let learned = words
            .as_deref()
            .and_then(|path| fs::read_to_string(path).ok())
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        Self {
            folders,
            dictionaries: HashMap::new(),
            learned,
            words,
        }
    }

    /// Idiomas con sus dos archivos, `.aff` y `.dic`, en alguna carpeta.
    pub fn languages(&self) -> Vec<String> {
        let mut found: Vec<String> = self
            .folders
            .iter()
            .filter_map(|folder| fs::read_dir(folder).ok())
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "dic"))
            .filter(|path| path.with_extension("aff").is_file())
            .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// Diccionario instalado que corresponde a `language`: «es» vale por
    /// `es_ES` y, si falta, por cualquier otro español.
    fn resolve(&self, language: &str) -> Option<String> {
        let wanted = language.replace('-', "_").to_lowercase();
        let languages = self.languages();
        let short = wanted.split('_').next().unwrap_or(&wanted).to_owned();
        let preferred = format!("{short}_{short}");
        [wanted.clone(), preferred]
            .iter()
            .find_map(|name| languages.iter().find(|l| l.to_lowercase() == *name))
            .or_else(|| {
                languages
                    .iter()
                    .find(|l| l.to_lowercase().split('_').next() == Some(short.as_str()))
            })
            .cloned()
    }

    fn dictionary(&mut self, language: &str) -> Option<&Dictionary> {
        if !self.dictionaries.contains_key(language) {
            let loaded = self.resolve(language).and_then(|name| {
                let dic = self
                    .folders
                    .iter()
                    .map(|folder| folder.join(format!("{name}.dic")))
                    .find(|path| path.is_file() && path.with_extension("aff").is_file())?;
                Dictionary::new(&read(&dic.with_extension("aff"))?, &read(&dic)?).ok()
            });
            self.dictionaries.insert(language.to_owned(), loaded);
        }
        self.dictionaries.get(language)?.as_ref()
    }

    /// Tramos, en unidades UTF-16, de las palabras que no están en el diccionario.
    pub fn check(&mut self, text: &str, language: &str) -> Vec<(usize, usize)> {
        let learned = std::mem::take(&mut self.learned);
        let mut found = Vec::new();
        if let Some(dictionary) = self.dictionary(language) {
            let mut word = String::new();
            let (mut start, mut at) = (0, 0);
            // Un espacio final cierra la última palabra.
            for c in text.chars().chain([' ']) {
                let inside = c.is_alphabetic() || (matches!(c, '\'' | '’') && !word.is_empty());
                if inside {
                    if word.is_empty() {
                        start = at;
                    }
                    word.push(c);
                } else if !word.is_empty() {
                    let trimmed = word.trim_end_matches(['\'', '’']);
                    if trimmed.chars().count() > 1
                        && !learned.contains(trimmed)
                        && !dictionary.check(trimmed)
                    {
                        let length: usize = trimmed.chars().map(char::len_utf16).sum();
                        found.push((start, start + length));
                    }
                    word.clear();
                }
                at += c.len_utf16();
            }
        }
        self.learned = learned;
        found
    }

    pub fn guesses(&mut self, word: &str, language: &str) -> Vec<String> {
        let mut guesses = Vec::new();
        if let Some(dictionary) = self.dictionary(language) {
            dictionary.suggest(word, &mut guesses);
        }
        guesses.truncate(8);
        guesses
    }

    /// Da por buena una palabra en todos los idiomas y la recuerda.
    pub fn learn(&mut self, word: &str) {
        if self.learned.insert(word.to_owned())
            && let Some(path) = &self.words
        {
            if let Some(folder) = path.parent() {
                let _ = fs::create_dir_all(folder);
            }
            if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{word}");
            }
        }
    }
}

/// Corrector compartido: los diccionarios de Miyu y los del sistema.
pub fn shared() -> &'static Mutex<Checker> {
    static CHECKER: OnceLock<Mutex<Checker>> = OnceLock::new();
    CHECKER.get_or_init(|| {
        let config = crate::config::directory();
        let mut folders = vec![config.join("dictionaries")];
        folders.extend(SYSTEM.iter().map(PathBuf::from));
        Mutex::new(Checker::new(folders, Some(config.join("palabras.txt"))))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_suggests_and_learns_with_hunspell_dictionaries() {
        let folder = std::env::temp_dir().join(format!("miyu-hunspell-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("es_MX.aff"),
            "SET UTF-8\nTRY aeoáéíóú\nSFX S Y 1\nSFX S 0 s [aeo]\n",
        )
        .unwrap();
        fs::write(folder.join("es_MX.dic"), "4\nhola\nmundo/S\ncanción\nel\n").unwrap();
        fs::write(folder.join("suelto.dic"), "1\nx\n").unwrap();
        let words = folder.join("palabras.txt");
        let mut checker = Checker::new(
            vec![folder.join("no-existe"), folder.clone()],
            Some(words.clone()),
        );
        assert_eq!(checker.languages(), ["es_MX"]);
        // «es» usa el español que haya; los tramos van en UTF-16.
        let text = "Hola mundos, canción 𝄞 mundx y Tectonic.";
        assert_eq!(checker.check(text, "es"), [(24, 29), (32, 40)]);
        assert_eq!(checker.check(text, "es-MX"), checker.check(text, "es"));
        assert!(
            checker
                .guesses("mundx", "es")
                .contains(&"mundo".to_string())
        );
        checker.learn("Tectonic");
        assert_eq!(checker.check(text, "es"), [(24, 29)]);
        // Lo aprendido se conserva para la próxima vez.
        let mut again = Checker::new(vec![folder.clone()], Some(words));
        assert_eq!(again.check(text, "es"), [(24, 29)]);
        // Sin diccionario para el idioma no se marca nada.
        assert!(checker.check(text, "fr").is_empty());
        assert!(checker.guesses("mundx", "fr").is_empty());
        fs::remove_dir_all(folder).unwrap();
    }
}
