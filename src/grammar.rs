//! Vocabulario de cada lenguaje, sacado de las gramáticas con que ya se
//! resalta el código: las palabras que una gramática marca como clave o como
//! parte de la biblioteca son las que conviene sugerir.

use std::{
    collections::HashMap,
    iter::Peekable,
    str::Chars,
    sync::{Once, OnceLock},
};

use syntect::parsing::{Scope, syntax_definition::Pattern};

use crate::format;

/// Cadenas que se aceptan de una sola expresión, como mucho.
const CAP: usize = 6000;

#[derive(Default)]
pub struct Vocabulary {
    pub keywords: Vec<String>,
    /// Tipos, funciones y constantes de la biblioteca del lenguaje.
    pub builtins: Vec<String>,
}

/// Cadenas que acepta un trozo de expresión; `None` si no son enumerables.
type Set = Option<Vec<String>>;

fn nothing() -> Set {
    Some(vec![String::new()])
}

fn concat(a: Set, b: Set) -> Set {
    let (a, b) = (a?, b?);
    if a.len() * b.len() > CAP {
        return None;
    }
    let mut all = Vec::with_capacity(a.len() * b.len());
    for head in &a {
        for tail in &b {
            all.push(format!("{head}{tail}"));
        }
    }
    Some(all)
}

/// Enumera lo que acepta una expresión hecha de literales, grupos,
/// alternativas y opcionales, que es como las gramáticas listan sus palabras.
struct Expander<'a> {
    chars: Peekable<Chars<'a>>,
    /// Lo que acepta cada grupo de captura; el 0 es la expresión entera.
    groups: Vec<Set>,
}

impl Expander<'_> {
    fn alternation(&mut self) -> Set {
        let mut all = Vec::new();
        let mut any = false;
        loop {
            // Una alternativa que no se puede enumerar no estropea las demás.
            if let Some(sequence) = self.sequence() {
                any = true;
                all.extend(sequence);
            }
            if self.chars.peek() != Some(&'|') {
                break;
            }
            self.chars.next();
        }
        any.then_some(all)
    }
    fn sequence(&mut self) -> Set {
        let mut all = nothing();
        while let Some(&c) = self.chars.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let mut atom = self.atom();
            match self.chars.peek() {
                Some('?') => {
                    self.chars.next();
                    if let Some(atom) = &mut atom {
                        atom.push(String::new());
                    }
                }
                Some('*' | '+') => {
                    self.chars.next();
                    atom = None;
                }
                Some('{') => {
                    self.chars.by_ref().find(|c| *c == '}');
                    atom = None;
                }
                _ => {}
            }
            // Perezoso o posesivo: no cambia lo que se acepta.
            if matches!(self.chars.peek(), Some('?' | '+')) {
                self.chars.next();
            }
            all = concat(all, atom);
        }
        all
    }
    fn atom(&mut self) -> Set {
        match self.chars.next()? {
            '\\' => match self.chars.next()? {
                'b' | 'B' | 'A' | 'z' | 'Z' | 'G' => nothing(),
                c if c.is_alphanumeric() => None,
                c => Some(vec![c.to_string()]),
            },
            '^' | '$' => nothing(),
            '.' => None,
            '[' => self.class(),
            '(' => self.group(),
            c => Some(vec![c.to_string()]),
        }
    }
    /// Una clase solo se enumera si es una lista corta de letras sueltas.
    fn class(&mut self) -> Set {
        let mut items = Vec::new();
        let mut simple = true;
        let mut first = true;
        while let Some(c) = self.chars.next() {
            match c {
                ']' if !first => break,
                '\\' => {
                    self.chars.next();
                    simple = false;
                }
                '^' if first => simple = false,
                '-' | '[' => simple = false,
                c => items.push(c.to_string()),
            }
            first = false;
        }
        (simple && items.len() <= 6).then_some(items)
    }
    fn group(&mut self) -> Set {
        let mut capture = true;
        let mut look = false;
        if self.chars.peek() == Some(&'?') {
            self.chars.next();
            capture = false;
            match self.chars.next()? {
                ':' | '>' => {}
                '=' | '!' => look = true,
                '<' | 'P' => {
                    if self.chars.peek() == Some(&'<') {
                        self.chars.next();
                    }
                    if matches!(self.chars.peek(), Some('=' | '!')) {
                        self.chars.next();
                        look = true;
                    } else {
                        // Grupo con nombre.
                        capture = true;
                        self.chars.by_ref().find(|c| *c == '>');
                    }
                }
                // Banderas: `(?i)` o `(?i:…)`.
                _ => loop {
                    match self.chars.next()? {
                        ')' => return nothing(),
                        ':' => break,
                        _ => {}
                    }
                },
            }
        }
        let index = capture.then(|| {
            self.groups.push(None);
            self.groups.len() - 1
        });
        let inner = self.alternation();
        self.chars.next();
        if let Some(index) = index {
            self.groups[index] = inner.clone();
        }
        // Lo que se mira alrededor no forma parte de la palabra.
        if look { nothing() } else { inner }
    }
}

/// Lo que acepta la expresión entera (grupo 0) y cada grupo de captura.
fn expand(regex: &str) -> Vec<Set> {
    // En modo extendido los espacios y los comentarios no cuentan.
    let source = if regex.contains("(?x") {
        let mut clean = String::new();
        let mut chars = regex.chars();
        let mut class = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    clean.push(c);
                    clean.extend(chars.next());
                }
                '#' if !class => {
                    chars.by_ref().find(|c| *c == '\n');
                }
                c if c.is_whitespace() && !class => {}
                c => {
                    class = (class || c == '[') && c != ']';
                    clean.push(c);
                }
            }
        }
        clean
    } else {
        regex.to_string()
    };
    let mut expander = Expander {
        chars: source.chars().peekable(),
        groups: vec![None],
    };
    let whole = expander.alternation();
    // Un paréntesis de más deja la expresión a medias: no es de fiar.
    expander.groups[0] = whole.filter(|_| expander.chars.peek().is_none());
    expander.groups
}

fn word(text: &str) -> bool {
    text.len() >= 2
        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !text.starts_with(|c: char| c.is_numeric())
}

fn build() -> HashMap<String, Vocabulary> {
    let scope = |name: &str| Scope::new(name).expect("ámbito integrado");
    let keyword = [
        "keyword",
        "storage",
        "constant.language",
        "variable.language",
    ]
    .map(scope);
    let builtin = ["support", "entity.name.tag"].map(scope);
    // Los prefijos de las cadenas (`rb"…"`) no son palabras que escribir solas.
    let skipped = scope("storage.type.string");
    // `Some(true)` si los ámbitos son de palabra clave, `Some(false)` si de biblioteca.
    let class = |scopes: &[Scope]| {
        scopes.iter().find_map(|s| {
            if skipped.is_prefix_of(*s) {
                None
            } else if keyword.iter().any(|k| k.is_prefix_of(*s)) {
                Some(true)
            } else {
                builtin.iter().any(|b| b.is_prefix_of(*s)).then_some(false)
            }
        })
    };
    // Las gramáticas se cargan a medias; el constructor las deja a la vista.
    let builder = format::syntax_settings().ps.clone().into_builder();
    let mut all = HashMap::new();
    for syntax in builder.syntaxes() {
        let mut vocabulary = Vocabulary::default();
        for pattern in syntax.contexts.values().flat_map(|c| &c.patterns) {
            let Pattern::Match(found) = pattern else {
                continue;
            };
            let mut wanted: Vec<(usize, bool)> =
                class(&found.scope).map(|k| (0, k)).into_iter().collect();
            for (group, scopes) in found.captures.iter().flatten() {
                wanted.extend(class(scopes).map(|k| (*group, k)));
            }
            if wanted.is_empty() {
                continue;
            }
            let groups = expand(found.regex.regex_str());
            for (group, is_keyword) in wanted {
                let list = if is_keyword {
                    &mut vocabulary.keywords
                } else {
                    &mut vocabulary.builtins
                };
                for text in groups.get(group).into_iter().flatten().flatten() {
                    if word(text) && !list.contains(text) {
                        list.push(text.clone());
                    }
                }
            }
        }
        if !vocabulary.keywords.is_empty() || !vocabulary.builtins.is_empty() {
            all.insert(syntax.name.clone(), vocabulary);
        }
    }
    all
}

/// Gramáticas de las que hereda un lenguaje, además de la suya.
fn related(language: &str) -> &'static [&'static str] {
    match language {
        "C++" | "Objective-C" => &["C"],
        "Objective-C++" => &["Objective-C", "C++", "C"],
        "PHP" => &["PHP Source"],
        "Bourne Again Shell (bash)" => &["commands-builtin-shell-bash"],
        "Shell-Unix-Generic" => &["Bourne Again Shell (bash)", "commands-builtin-shell-bash"],
        _ => &[],
    }
}

/// Vocabularios que aplican a `language`. Se preparan aparte la primera vez
/// (unas decenas de milisegundos) y mientras tanto no hay ninguno.
pub fn vocabularies(language: &str) -> Vec<&'static Vocabulary> {
    static ALL: OnceLock<HashMap<String, Vocabulary>> = OnceLock::new();
    static STARTED: Once = Once::new();
    let all = if cfg!(test) {
        Some(ALL.get_or_init(build))
    } else {
        STARTED.call_once(|| {
            std::thread::spawn(|| ALL.get_or_init(build));
        });
        ALL.get()
    };
    let Some(all) = all else {
        return Vec::new();
    };
    std::iter::once(language)
        .chain(related(language).iter().copied())
        .filter_map(|name| all.get(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn whole(regex: &str) -> Vec<String> {
        let mut words = expand(regex).swap_remove(0).unwrap_or_default();
        words.sort();
        words
    }

    #[test]
    fn expands_factored_alternations() {
        assert_eq!(
            whole(r"\b(?:s(?:elect|witch)|for)\b"),
            ["for", "select", "switch"]
        );
        assert_eq!(
            whole(r"\bstr(n?cpy|len)(?=\s*\()"),
            ["strcpy", "strlen", "strncpy"]
        );
        assert_eq!(
            whole("(?x) \\b( if # condicional\n | else )\\b"),
            ["else", "if"]
        );
        assert_eq!(whole(r"ceil[fl]?"), ["ceil", "ceilf", "ceill"]);
        // Lo que no se puede enumerar se descarta, pero no sus alternativas.
        assert!(whole(r"\w+").is_empty());
        assert_eq!(whole(r"\d+|nil"), ["nil"]);
        // Cada captura guarda lo suyo aunque el resto no sea enumerable.
        let groups = expand(r"\b(def|class)\s+(\w+)");
        assert_eq!(groups[0], None);
        assert_eq!(groups[1], Some(vec!["def".into(), "class".into()]));
        assert_eq!(groups[2], None);
    }

    #[test]
    fn grammars_carry_the_language_vocabulary() {
        let has = |language: &str, keyword: bool, word: &str| {
            vocabularies(language).iter().any(|v| {
                let list = if keyword { &v.keywords } else { &v.builtins };
                list.iter().any(|w| w == word)
            })
        };
        assert!(has("Python", true, "lambda") && has("Python", false, "isinstance"));
        assert!(has("Python", false, "ValueError") && !has("Python", true, "rb"));
        assert!(has("Rust", true, "match") && has("Rust", false, "Option"));
        assert!(has("C", false, "printf") && has("C++", false, "printf"));
        assert!(has("C++", true, "namespace") && has("Go", true, "defer"));
        assert!(has("PHP", true, "foreach") && has("Haskell", true, "where"));
        assert!(has("HTML", false, "div") && has("Lua", false, "ipairs"));
    }
}
