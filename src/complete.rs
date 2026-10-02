//! Completado local del código, al estilo de los IDE clásicos: palabras del
//! documento, miembros tras un punto, vocabulario del lenguaje y plantillas.
//! El vocabulario sale de las gramáticas del resaltado (`grammar`); las
//! tablas de aquí solo añaden lo que a esas gramáticas les falta.

use std::collections::HashMap;

use super::{Completion, Editor, Pos, byte_col, code::is_word};
use crate::format::Format;

#[path = "grammar.rs"]
mod grammar;

/// Palabras del documento que se ofrecen como mucho.
const DOCUMENT_WORDS: usize = 24;
const LIMIT: usize = 40;
/// Por encima de estas filas no se calcula el esquema solo para etiquetar.
const OUTLINE_ROWS: usize = 5000;

/// Abreviatura, descripción y cuerpo; `$0` es el cursor y `\t` un nivel de sangría.
type Snippet = (&'static str, &'static str, &'static str);
/// Listas de palabras separadas por espacios.
type Words = &'static [&'static str];

/// Lo que se añade a mano al vocabulario de la gramática de un lenguaje.
struct Lexicon {
    keywords: Words,
    /// Tipos, funciones y módulos que vienen con el lenguaje.
    builtins: Words,
    /// Métodos y campos habituales, para después de un punto.
    members: Words,
    snippets: &'static [&'static [Snippet]],
    /// El lenguaje no distingue mayúsculas: se sigue la caja de lo escrito.
    caseless: bool,
}

const NONE: Lexicon = Lexicon {
    keywords: &[],
    builtins: &[],
    members: &[],
    snippets: &[],
    caseless: false,
};

const RUST_KEYWORDS: &str = "async await dyn";
const RUST_BUILTINS: &str = "Rc Arc RefCell Cell Mutex RwLock HashMap HashSet BTreeMap BTreeSet VecDeque PathBuf \
    Path Duration Instant Debug Display Hash TryFrom u128 i128 println! eprintln! print! \
    format! vec! panic! assert! assert_eq! assert_ne! debug_assert! todo! unimplemented! \
    unreachable! matches! write! writeln! dbg! include_str! derive cfg allow test";
const RUST_MEMBERS: &str = "unwrap unwrap_or unwrap_or_default unwrap_or_else expect clone cloned iter iter_mut \
    into_iter collect map filter filter_map flat_map fold enumerate zip rev take skip \
    find position any all count sum len is_empty push push_str pop insert remove extend \
    clear contains get get_mut first last sort sort_by_key join to_string to_owned \
    to_vec as_str as_ref as_mut as_deref into is_some is_none is_ok is_err ok ok_or \
    and_then map_err trim split lines chars bytes starts_with ends_with replace parse \
    entry or_insert keys values lock borrow borrow_mut";
const RUST_SNIPPETS: &[Snippet] = &[
    ("fn", "función", "fn $0() {\n\t\n}"),
    ("main", "función principal", "fn main() {\n\t$0\n}"),
    ("struct", "estructura", "struct $0 {\n\t\n}"),
    ("enum", "enumeración", "enum $0 {\n\t\n}"),
    ("impl", "bloque impl", "impl $0 {\n\t\n}"),
    ("trait", "rasgo", "trait $0 {\n\t\n}"),
    ("match", "match", "match $0 {\n\t_ => {}\n}"),
    ("if", "condicional", "if $0 {\n\t\n}"),
    ("iflet", "if let", "if let Some($0) =  {\n\t\n}"),
    ("for", "bucle for", "for $0 in  {\n\t\n}"),
    ("while", "bucle while", "while $0 {\n\t\n}"),
    ("loop", "bucle infinito", "loop {\n\t$0\n}"),
    ("test", "prueba", "#[test]\nfn $0() {\n\t\n}"),
    (
        "tests",
        "módulo de pruebas",
        "#[cfg(test)]\nmod tests {\n\tuse super::*;\n\n\t#[test]\n\tfn $0() {\n\t\t\n\t}\n}",
    ),
    ("derive", "atributo derive", "#[derive(Debug, Clone$0)]"),
    ("println", "imprimir una línea", "println!(\"$0\");"),
];

const PYTHON_KEYWORDS: &str = "match";
const PYTHON_BUILTINS: &str = "__main__ dataclass";
const PYTHON_MEMBERS: &str = "append extend insert remove pop sort reverse index count clear copy items keys \
    values get update setdefault add discard join split strip lstrip rstrip replace \
    startswith endswith format lower upper find encode decode splitlines read write \
    readline readlines close path exists";
const PYTHON_SNIPPETS: &[Snippet] = &[
    ("def", "función", "def $0():\n\tpass"),
    (
        "class",
        "clase",
        "class $0:\n\tdef __init__(self):\n\t\tpass",
    ),
    ("init", "constructor", "def __init__(self$0):\n\tpass"),
    ("if", "condicional", "if $0:\n\tpass"),
    (
        "ifelse",
        "condicional con else",
        "if $0:\n\tpass\nelse:\n\tpass",
    ),
    ("for", "bucle for", "for $0 in :\n\tpass"),
    (
        "forr",
        "bucle sobre un rango",
        "for i in range($0):\n\tpass",
    ),
    ("while", "bucle while", "while $0:\n\tpass"),
    (
        "try",
        "try / except",
        "try:\n\t$0\nexcept Exception as e:\n\traise",
    ),
    ("with", "with open", "with open($0) as f:\n\tpass"),
    (
        "main",
        "punto de entrada",
        "if __name__ == \"__main__\":\n\t$0",
    ),
    ("lambda", "función anónima", "lambda x: $0"),
];

const JS_BUILTINS: &str = "Symbol JSON setInterval clearInterval fetch require globalThis localStorage \
    structuredClone requestAnimationFrame";
const JS_MEMBERS: &str = "length push pop shift unshift slice splice map filter reduce forEach find findIndex \
    includes indexOf join split concat sort reverse keys values entries then catch \
    finally toString trim replace replaceAll startsWith endsWith toLowerCase toUpperCase \
    log error warn stringify parse addEventListener removeEventListener querySelector \
    querySelectorAll getElementById createElement appendChild classList textContent \
    innerHTML preventDefault target value floor random";
const JS_SNIPPETS: &[Snippet] = &[
    ("function", "función", "function $0() {\n\t\n}"),
    ("arrow", "función flecha", "const $0 = () => {\n\t\n};"),
    (
        "for",
        "bucle con índice",
        "for (let i = 0; i < $0; i++) {\n\t\n}",
    ),
    ("forof", "bucle for…of", "for (const $0 of ) {\n\t\n}"),
    ("forin", "bucle for…in", "for (const $0 in ) {\n\t\n}"),
    (
        "class",
        "clase",
        "class $0 {\n\tconstructor() {\n\t\t\n\t}\n}",
    ),
    ("log", "console.log", "console.log($0);"),
    ("import", "importación", "import { $0 } from \"\";"),
    (
        "try",
        "try / catch",
        "try {\n\t$0\n} catch (error) {\n\t\n}",
    ),
];
/// Estructuras comunes a los lenguajes de llaves.
const BRACE_SNIPPETS: &[Snippet] = &[
    ("if", "condicional", "if ($0) {\n\t\n}"),
    (
        "ifelse",
        "condicional con else",
        "if ($0) {\n\t\n} else {\n\t\n}",
    ),
    ("while", "bucle while", "while ($0) {\n\t\n}"),
    ("do", "bucle do…while", "do {\n\t\n} while ($0);"),
    (
        "switch",
        "selección",
        "switch ($0) {\n\tcase :\n\t\tbreak;\n\tdefault:\n\t\tbreak;\n}",
    ),
];

const GO_BUILTINS: &str = "any fmt errors strings strconv context os io time sync http";
const GO_MEMBERS: &str = "Println Printf Sprintf Errorf Fprintf Error String Close Read Write Lock Unlock Add \
    Done Wait New Contains Split Join TrimSpace Atoi Itoa Now Since Background";
const GO_SNIPPETS: &[Snippet] = &[
    ("func", "función", "func $0() {\n\t\n}"),
    ("main", "función principal", "func main() {\n\t$0\n}"),
    (
        "for",
        "bucle con índice",
        "for i := 0; i < $0; i++ {\n\t\n}",
    ),
    ("forr", "bucle con range", "for i, v := range $0 {\n\t\n}"),
    ("if", "condicional", "if $0 {\n\t\n}"),
    (
        "iferr",
        "comprobar el error",
        "if err != nil {\n\treturn $0err\n}",
    ),
    ("struct", "estructura", "type $0 struct {\n\t\n}"),
    ("interface", "interfaz", "type $0 interface {\n\t\n}"),
    (
        "switch",
        "selección",
        "switch $0 {\ncase :\n\t\ndefault:\n\t\n}",
    ),
];

const C_KEYWORDS: &str = "define include ifdef ifndef endif pragma";
const C_BUILTINS: &str = "EOF FILE stdin stdout stderr EXIT_SUCCESS EXIT_FAILURE";
const C_SNIPPETS: &[Snippet] = &[
    (
        "main",
        "función principal",
        "int main(int argc, char *argv[]) {\n\t$0\n\treturn 0;\n}",
    ),
    (
        "for",
        "bucle con índice",
        "for (int i = 0; i < $0; i++) {\n\t\n}",
    ),
    ("struct", "estructura", "struct $0 {\n\t\n};"),
    ("inc", "#include del sistema", "#include <$0>"),
    ("printf", "imprimir con formato", "printf(\"$0\\n\");"),
];

const CPP_KEYWORDS: &str = "concept const_cast dynamic_cast reinterpret_cast requires static_cast";
const CPP_BUILTINS: &str = "std string string_view vector map unordered_map set unordered_set pair tuple array \
    deque list queue stack optional variant unique_ptr shared_ptr make_unique \
    make_shared move forward cout cin cerr endl swap function thread mutex iostream \
    algorithm memory";
const CPP_MEMBERS: &str = "size empty push_back emplace_back pop_back begin end front back clear insert erase \
    find at length substr c_str first second reserve resize count get reset";
const CPP_SNIPPETS: &[Snippet] = &[
    (
        "class",
        "clase",
        "class $0 {\npublic:\n\t\nprivate:\n\t\n};",
    ),
    (
        "forr",
        "bucle sobre un rango",
        "for (const auto& $0 : ) {\n\t\n}",
    ),
    (
        "cout",
        "imprimir una línea",
        "std::cout << $0 << std::endl;",
    ),
    ("namespace", "espacio de nombres", "namespace $0 {\n\n}"),
    (
        "try",
        "try / catch",
        "try {\n\t$0\n} catch (const std::exception& e) {\n\t\n}",
    ),
];

const JAVA_KEYWORDS: &str = "record var";
const JAVA_BUILTINS: &str = "String Integer Long Double Float Boolean Character Object System Math List ArrayList \
    LinkedList Map HashMap TreeMap Set HashSet Arrays Collections Optional Stream \
    StringBuilder Scanner Exception RuntimeException IllegalArgumentException \
    IllegalStateException NullPointerException IOException Thread Runnable Override \
    Deprecated FunctionalInterface";
const JAVA_MEMBERS: &str = "length size get set add remove contains isEmpty equals hashCode toString put \
    containsKey keySet values entrySet stream map filter collect forEach out println \
    printf format substring indexOf charAt trim split append getClass valueOf";
const JAVA_SNIPPETS: &[Snippet] = &[
    (
        "main",
        "método principal",
        "public static void main(String[] args) {\n\t$0\n}",
    ),
    ("sout", "imprimir una línea", "System.out.println($0);"),
    (
        "for",
        "bucle con índice",
        "for (int i = 0; i < $0; i++) {\n\t\n}",
    ),
    (
        "foreach",
        "bucle sobre una colección",
        "for (var $0 : ) {\n\t\n}",
    ),
    ("class", "clase", "public class $0 {\n\t\n}"),
    (
        "try",
        "try / catch",
        "try {\n\t$0\n} catch (Exception e) {\n\t\n}",
    ),
];

const CSHARP_KEYWORDS: &str = "record yield";
const CSHARP_BUILTINS: &str = "Console String List Dictionary HashSet IEnumerable Task Math DateTime TimeSpan \
    Exception ArgumentException Guid StringBuilder System Linq Func Action Nullable";
const CSHARP_MEMBERS: &str = "WriteLine ReadLine Length Count Add Remove Contains ToString Equals Select Where \
    ToList ToArray First FirstOrDefault Any OrderBy Split Trim Substring Join Format \
    Parse TryParse";
const CSHARP_SNIPPETS: &[Snippet] = &[
    (
        "main",
        "método principal",
        "static void Main(string[] args)\n{\n\t$0\n}",
    ),
    ("cw", "imprimir una línea", "Console.WriteLine($0);"),
    (
        "for",
        "bucle con índice",
        "for (int i = 0; i < $0; i++)\n{\n\t\n}",
    ),
    (
        "foreach",
        "bucle sobre una colección",
        "foreach (var $0 in )\n{\n\t\n}",
    ),
    ("class", "clase", "public class $0\n{\n\t\n}"),
    ("prop", "propiedad", "public $0 { get; set; }"),
    (
        "try",
        "try / catch",
        "try\n{\n\t$0\n}\ncatch (Exception e)\n{\n\t\n}",
    ),
];

const PHP_SNIPPETS: &[Snippet] = &[
    ("function", "función", "function $0() {\n\t\n}"),
    (
        "foreach",
        "bucle sobre un arreglo",
        "foreach ($0 as $value) {\n\t\n}",
    ),
    (
        "for",
        "bucle con índice",
        "for ($i = 0; $i < $0; $i++) {\n\t\n}",
    ),
    (
        "class",
        "clase",
        "class $0 {\n\tpublic function __construct() {\n\t\t\n\t}\n}",
    ),
    (
        "try",
        "try / catch",
        "try {\n\t$0\n} catch (Exception $e) {\n\t\n}",
    ),
];

const RUBY_BUILTINS: &str = "Symbol Struct StandardError ArgumentError";
const RUBY_MEMBERS: &str = "each each_with_index map select reject reduce find include? empty? nil? length size \
    first last push pop join split strip to_s to_i to_a to_sym keys values new sort_by \
    upcase downcase";
const RUBY_SNIPPETS: &[Snippet] = &[
    ("def", "método", "def $0\n\t\nend"),
    (
        "class",
        "clase",
        "class $0\n\tdef initialize\n\t\t\n\tend\nend",
    ),
    ("if", "condicional", "if $0\n\t\nend"),
    ("each", "bucle each", "$0.each do |item|\n\t\nend"),
    ("module", "módulo", "module $0\n\t\nend"),
];

const LUA_KEYWORDS: &str = "goto";
const LUA_BUILTINS: &str = "string table math io os coroutine";
const LUA_MEMBERS: &str = "insert remove concat sort format gsub gmatch match find sub len lower upper rep \
    floor ceil random max min open read write close";
const LUA_SNIPPETS: &[Snippet] = &[
    ("function", "función", "function $0()\n\t\nend"),
    ("lfunction", "función local", "local function $0()\n\t\nend"),
    ("if", "condicional", "if $0 then\n\t\nend"),
    ("for", "bucle numérico", "for i = 1, $0 do\n\t\nend"),
    (
        "forp",
        "bucle con pairs",
        "for k, v in pairs($0) do\n\t\nend",
    ),
    ("while", "bucle while", "while $0 do\n\t\nend"),
];

const SHELL_KEYWORDS: &str = "select return unset shift exit echo printf read cd pwd source eval exec trap test \
    true false set getopts wait kill grep sed awk cat ls mkdir rm cp mv chmod chown find \
    xargs sort uniq head tail curl wget tar basename dirname";
const SHELL_SNIPPETS: &[Snippet] = &[
    ("if", "condicional", "if [ $0 ]; then\n\t\nfi"),
    ("for", "bucle for", "for $0 in ; do\n\t\ndone"),
    ("while", "bucle while", "while $0; do\n\t\ndone"),
    ("case", "selección", "case $0 in\n\t*)\n\t\t;;\nesac"),
    ("function", "función", "$0() {\n\t\n}"),
    (
        "shebang",
        "cabecera del guion",
        "#!/usr/bin/env bash\nset -euo pipefail\n$0",
    ),
];

const SQL_KEYWORDS: &str = "SELECT INSERT INTO CREATE INNER LEFT RIGHT OUTER FULL CROSS BY ORDER OFFSET DISTINCT \
    UNION ALL NOT NULL IS EXISTS PRIMARY KEY FOREIGN UNIQUE BEGIN COMMIT ROLLBACK \
    TRANSACTION RETURNING TRUNCATE";
const SQL_BUILTINS: &str = "COALESCE NULLIF CAST CONCAT LENGTH ROUND NOW VARCHAR";
const SQL_SNIPPETS: &[Snippet] = &[
    ("select", "consulta", "SELECT $0\nFROM \nWHERE ;"),
    ("insert", "inserción", "INSERT INTO $0 ()\nVALUES ();"),
    ("update", "actualización", "UPDATE $0\nSET \nWHERE ;"),
    ("delete", "borrado", "DELETE FROM $0\nWHERE ;"),
    (
        "create",
        "tabla nueva",
        "CREATE TABLE $0 (\n\tid INTEGER PRIMARY KEY\n);",
    ),
    ("join", "unión de tablas", "JOIN $0 ON "),
];

const HTML_WORDS: &str = "a main canvas svg video audio source class id href src alt type name value \
    placeholder rel charset content width height onclick disabled checked required \
    target lang";
const HTML_SNIPPETS: &[Snippet] = &[
    (
        "html",
        "esqueleto del documento",
        "<!DOCTYPE html>\n<html lang=\"es\">\n<head>\n\t<meta charset=\"utf-8\">\n\t<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\t<title>$0</title>\n</head>\n<body>\n\t\n</body>\n</html>",
    ),
    ("div", "bloque", "<div class=\"$0\">\n\t\n</div>"),
    ("a", "enlace", "<a href=\"$0\"></a>"),
    ("img", "imagen", "<img src=\"$0\" alt=\"\">"),
    (
        "link",
        "hoja de estilos",
        "<link rel=\"stylesheet\" href=\"$0\">",
    ),
    ("script", "guion", "<script src=\"$0\"></script>"),
    ("ul", "lista", "<ul>\n\t<li>$0</li>\n</ul>"),
    (
        "table",
        "tabla",
        "<table>\n\t<tr>\n\t\t<td>$0</td>\n\t</tr>\n</table>",
    ),
];

const CSS_WORDS: &str = "background-color border-radius min-width max-width min-height max-height font-family \
    font-size font-weight line-height text-align text-decoration flex-direction justify- \
    content align-items gap grid-template-columns box-shadow z-index inline-block \
    important";
const CSS_SNIPPETS: &[Snippet] = &[
    (
        "flex",
        "caja flexible centrada",
        "display: flex;\nalign-items: center;\njustify-content: center;$0",
    ),
    (
        "media",
        "consulta de medios",
        "@media (max-width: $0) {\n\t\n}",
    ),
];

fn lexicon(language: &str) -> Lexicon {
    match language {
        "Rust" => Lexicon {
            keywords: &[RUST_KEYWORDS],
            builtins: &[RUST_BUILTINS],
            members: &[RUST_MEMBERS],
            snippets: &[RUST_SNIPPETS],
            ..NONE
        },
        "Python" => Lexicon {
            keywords: &[PYTHON_KEYWORDS],
            builtins: &[PYTHON_BUILTINS],
            members: &[PYTHON_MEMBERS],
            snippets: &[PYTHON_SNIPPETS],
            ..NONE
        },
        "JavaScript" => Lexicon {
            builtins: &[JS_BUILTINS],
            members: &[JS_MEMBERS],
            snippets: &[JS_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "Go" => Lexicon {
            builtins: &[GO_BUILTINS],
            members: &[GO_MEMBERS],
            snippets: &[GO_SNIPPETS],
            ..NONE
        },
        "C" | "Objective-C" => Lexicon {
            keywords: &[C_KEYWORDS],
            builtins: &[C_BUILTINS],
            snippets: &[C_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "C++" | "Objective-C++" => Lexicon {
            keywords: &[C_KEYWORDS, CPP_KEYWORDS],
            builtins: &[CPP_BUILTINS, C_BUILTINS],
            members: &[CPP_MEMBERS],
            snippets: &[CPP_SNIPPETS, C_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "Java" => Lexicon {
            keywords: &[JAVA_KEYWORDS],
            builtins: &[JAVA_BUILTINS],
            members: &[JAVA_MEMBERS],
            snippets: &[JAVA_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "C#" => Lexicon {
            keywords: &[CSHARP_KEYWORDS],
            builtins: &[CSHARP_BUILTINS],
            members: &[CSHARP_MEMBERS],
            snippets: &[CSHARP_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "PHP" => Lexicon {
            snippets: &[PHP_SNIPPETS, BRACE_SNIPPETS],
            ..NONE
        },
        "Ruby" => Lexicon {
            builtins: &[RUBY_BUILTINS],
            members: &[RUBY_MEMBERS],
            snippets: &[RUBY_SNIPPETS],
            ..NONE
        },
        "Lua" => Lexicon {
            keywords: &[LUA_KEYWORDS],
            builtins: &[LUA_BUILTINS],
            members: &[LUA_MEMBERS],
            snippets: &[LUA_SNIPPETS],
            ..NONE
        },
        "Bourne Again Shell (bash)" | "Shell-Unix-Generic" => Lexicon {
            keywords: &[SHELL_KEYWORDS],
            snippets: &[SHELL_SNIPPETS],
            ..NONE
        },
        "SQL" => Lexicon {
            keywords: &[SQL_KEYWORDS],
            builtins: &[SQL_BUILTINS],
            snippets: &[SQL_SNIPPETS],
            caseless: true,
            ..NONE
        },
        "HTML" => Lexicon {
            builtins: &[HTML_WORDS],
            snippets: &[HTML_SNIPPETS],
            ..NONE
        },
        "CSS" => Lexicon {
            builtins: &[CSS_WORDS],
            snippets: &[CSS_SNIPPETS],
            ..NONE
        },
        _ => NONE,
    }
}

/// Si lo que precede accede a un miembro: `.`, `::` o `->`, pero no un rango `..`.
fn after_access(before: &[u8]) -> bool {
    (before.ends_with(b".") && !before.ends_with(b".."))
        || before.ends_with(b"::")
        || before.ends_with(b"->")
}

fn has_prefix(word: &str, prefix: &str) -> bool {
    word.len() >= prefix.len()
        && word.is_char_boundary(prefix.len())
        && word[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// Sugerencia a medio ordenar: grupo, desempate y los datos que se muestran.
struct Entry {
    rank: (u8, bool, usize),
    label: String,
    insert: String,
    detail: String,
    kind: &'static str,
}

impl Editor {
    /// Pide sugerencias aunque aún no se haya escrito nada (Ctrl+Espacio).
    pub fn request_completion(&mut self) {
        self.forced = Some((self.revision, self.cursor));
        self.quiet = None;
    }
    /// Columna donde empieza y texto de la palabra a medio escribir. Hacen
    /// falta dos letras, una tras un punto y ninguna si se pidió a mano.
    pub(super) fn word_prefix(&self) -> Option<(usize, String)> {
        let chars: Vec<char> = self.lines[self.cursor.row].chars().collect();
        let end = self.cursor.col.min(chars.len());
        if chars.get(end).is_some_and(|c| is_word(*c)) {
            return None;
        }
        let mut start = end;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }
        if chars.get(start).is_some_and(|c| c.is_numeric()) {
            return None;
        }
        let needed = if self.forced == Some((self.revision, self.cursor)) {
            0
        } else if after_access(chars[..start].iter().collect::<String>().as_bytes()) {
            1
        } else {
            2
        };
        (end - start >= needed).then(|| (start, chars[start..end].iter().collect()))
    }
    /// Si `before`, el comienzo de la línea, acaba dentro de un comentario o
    /// de una cadena: ahí solo tienen sentido las palabras del documento.
    fn in_prose(&self, before: &str) -> bool {
        let Format::Code(language) = &self.format else {
            return false;
        };
        let comment = self.format.comment().map(|(open, _)| open);
        // El apóstrofo abre tiempos de vida en Rust y es texto en el marcado.
        let quotes: &[char] = match language.as_str() {
            "Rust" | "HTML" | "XML" => &['"'],
            _ => &['"', '\'', '`'],
        };
        let mut quote = None;
        let mut chars = before.char_indices();
        while let Some((at, c)) = chars.next() {
            match quote {
                Some(_) if c == '\\' => {
                    chars.next();
                }
                Some(open) if c == open => quote = None,
                Some(_) => {}
                None if comment.is_some_and(|open| before[at..].starts_with(open)) => return true,
                None if quotes.contains(&c) => quote = Some(c),
                None => {}
            }
        }
        quote.is_some()
    }
    /// Clase y fila de las definiciones del archivo, por nombre.
    fn defined(&self) -> HashMap<String, String> {
        if self.lines.len() > OUTLINE_ROWS {
            return HashMap::new();
        }
        self.outline()
            .iter()
            .filter_map(|(row, _, title)| {
                let (kind, name) = match title.rsplit_once(' ') {
                    Some((kind, name)) => (kind, name),
                    None => (
                        if title.ends_with("()") {
                            "función"
                        } else {
                            "definición"
                        },
                        title.as_str(),
                    ),
                };
                let name = name.trim_end_matches("()");
                (!kind.contains(' ') && !name.is_empty() && name.chars().all(is_word))
                    .then(|| (name.to_string(), format!("{kind} · línea {}", row + 1)))
            })
            .collect()
    }
    /// Sugiere lo que empieza como la palabra a medio escribir: primero lo del
    /// documento, de más cerca a más lejos, y luego lo propio del lenguaje.
    pub(super) fn complete_words(&mut self) {
        let Some((start, prefix)) = self.word_prefix() else {
            return;
        };
        if self.quiet == Some((self.revision, self.cursor)) {
            return;
        }
        let Format::Code(language) = &self.format else {
            return;
        };
        let lexicon = lexicon(language);
        let line = &self.lines[self.cursor.row];
        let before = &line[..byte_col(line, start)];
        let member = after_access(before.as_bytes());
        let prose = self.in_prose(before);
        let here = self.offset(Pos::new(self.cursor.row, start));
        let text = &self.text;
        let base = text.as_ptr() as usize;

        // Cada palabra del documento: si alguna vez sigue a un punto y a qué distancia queda.
        let mut found: HashMap<&str, (bool, usize)> = HashMap::new();
        for word in text.split(|c: char| !is_word(c)) {
            if word.len() <= prefix.len()
                || !has_prefix(word, &prefix)
                || word.starts_with(|c: char| c.is_numeric())
            {
                continue;
            }
            let at = word.as_ptr() as usize - base;
            let entry = found.entry(word).or_insert((false, usize::MAX));
            entry.0 |= after_access(&text.as_bytes()[..at]);
            entry.1 = entry.1.min(at.abs_diff(here));
        }
        // Vocabulario del lenguaje: el de su gramática y lo añadido a mano.
        let grammars = grammar::vocabularies(language);
        let keywords = || {
            let own = lexicon
                .keywords
                .iter()
                .flat_map(|list| list.split_whitespace());
            own.chain(
                grammars
                    .iter()
                    .flat_map(|g| g.keywords.iter().map(String::as_str)),
            )
        };
        let builtins = || {
            let own = lexicon
                .builtins
                .iter()
                .flat_map(|list| list.split_whitespace());
            own.chain(
                grammars
                    .iter()
                    .flat_map(|g| g.builtins.iter().map(String::as_str)),
            )
        };
        let defined = if found.is_empty() {
            HashMap::new()
        } else {
            self.defined()
        };
        let mut entries: Vec<Entry> = found
            .into_iter()
            .map(|(word, (dotted, distance))| Entry {
                rank: (
                    if member && !dotted { 4 } else { 1 },
                    !word.starts_with(&prefix),
                    distance,
                ),
                label: word.to_string(),
                insert: word.to_string(),
                detail: if let Some(detail) = defined.get(word) {
                    detail.clone()
                } else if keywords().any(|k| k == word) {
                    "Palabra clave".into()
                } else if builtins().any(|b| b == word) {
                    "Del lenguaje".into()
                } else if member && dotted {
                    "Miembro usado en el documento".into()
                } else {
                    "Palabra del documento".into()
                },
                kind: "word",
            })
            .collect();
        entries.sort_by_key(|entry| entry.rank);
        entries.truncate(DOCUMENT_WORDS);

        if !prose {
            // En SQL se sigue la caja de lo que se lleva escrito.
            let lower = !prefix.is_empty() && !prefix.chars().any(char::is_uppercase);
            let cased = |word: &str| match (lexicon.caseless, lower) {
                (false, _) => word.to_string(),
                (true, true) => word.to_lowercase(),
                (true, false) => word.to_uppercase(),
            };
            let sources: Vec<(u8, &str, &str)> = if member {
                let usual = lexicon
                    .members
                    .iter()
                    .flat_map(|list| list.split_whitespace());
                usual.map(|w| (2, w, "Miembro habitual")).collect()
            } else {
                let keywords = keywords().map(|w| (2, w, "Palabra clave"));
                keywords
                    .chain(builtins().map(|w| (3, w, "Del lenguaje")))
                    .collect()
            };
            // Por grupos y, dentro de cada uno, por orden alfabético.
            let mut words: Vec<(u8, bool, String, String, &str)> = sources
                .into_iter()
                .map(|(group, word, detail)| (group, cased(word), detail))
                .filter(|(_, word, _)| word.len() > prefix.len() && has_prefix(word, &prefix))
                .map(|(group, word, detail)| {
                    (
                        group,
                        !word.starts_with(&prefix),
                        word.to_lowercase(),
                        word,
                        detail,
                    )
                })
                .collect();
            words.sort();
            for (group, mismatch, _, word, detail) in words {
                if !entries.iter().any(|e| e.label == word) {
                    entries.push(Entry {
                        rank: (group, mismatch, entries.len()),
                        label: word.clone(),
                        insert: word,
                        detail: detail.into(),
                        kind: "word",
                    });
                }
            }
            if !member {
                let unit = self.unit();
                for (trigger, description, body) in lexicon.snippets.iter().copied().flatten() {
                    if !has_prefix(trigger, &prefix)
                        || entries
                            .iter()
                            .any(|e| e.kind == "snippet" && e.label == *trigger)
                    {
                        continue;
                    }
                    // Escrita entera, la abreviatura va la primera: Tab la expande.
                    let exact = trigger.len() == prefix.len();
                    entries.push(Entry {
                        rank: (if exact { 0 } else { 5 }, false, entries.len()),
                        label: trigger.to_string(),
                        insert: body.replace('\t', &unit),
                        detail: format!("Plantilla: {description}"),
                        kind: "snippet",
                    });
                }
            }
        }
        entries.sort_by_key(|entry| entry.rank);
        self.completions = entries
            .into_iter()
            .take(LIMIT)
            .map(|entry| Completion {
                label: entry.label,
                insert: entry.insert,
                detail: entry.detail,
                start,
                kind: entry.kind.into(),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(e: &Editor) -> Vec<&str> {
        e.completions.iter().map(|c| c.label.as_str()).collect()
    }
    fn typed(text: &str, name: &str) -> Editor {
        let mut e = Editor::untitled(text.into(), name);
        let end = e.end();
        e.goto(end.row, end.col);
        e.update_completion();
        e
    }

    #[test]
    fn word_completion_prefers_nearby_words() {
        let mut e = typed("let contador = 1;\nlet contexto = 2;\nco", "main.rs");
        assert_eq!(
            labels(&e),
            ["contexto", "contador", "const", "continue", "Copy"]
        );
        e.accept_completion();
        assert_eq!(e.lines[2], "contexto");
        // Tras aceptar no vuelve a sugerir hasta que se siga escribiendo.
        e.update_completion();
        assert!(e.completions.is_empty());
        e.insert("\ncont");
        e.update_completion();
        assert_eq!(e.completions[0].label, "contexto");
        e.insert("exto");
        e.update_completion();
        assert!(e.completions.is_empty());
        e.goto(0, 6);
        e.update_completion();
        assert!(e.completions.is_empty());
    }

    #[test]
    fn vocabulary_depends_on_the_language() {
        assert!(labels(&typed("pri", "a.py")).contains(&"print"));
        assert!(labels(&typed("pri", "a.rs")).contains(&"println!"));
        assert!(labels(&typed("pri", "a.c")).contains(&"printf"));
        assert!(labels(&typed("pri", "a.json")).is_empty());
        // Ignora mayúsculas, y en SQL respeta la caja de lo escrito.
        assert!(labels(&typed("hashm", "a.rs")).contains(&"HashMap"));
        assert_eq!(labels(&typed("sel", "a.sql")), ["select", "select"]);
        assert_eq!(typed("SEL", "a.sql").completions[0].label, "SELECT");
        // Las definiciones del archivo dicen qué son y dónde están.
        let e = typed("def calcular(x):\n    return x\n\ncalc", "a.py");
        assert_eq!(e.completions[0].label, "calcular");
        assert_eq!(e.completions[0].detail, "def · línea 1");
    }

    #[test]
    fn members_follow_a_dot() {
        let e = typed("datos.append(1)\nappuntes = 2\ndatos.ap", "a.py");
        assert_eq!(labels(&e), ["append", "appuntes"]);
        assert_eq!(e.completions[0].detail, "Miembro usado en el documento");
        // Tras el punto basta una letra y no se ofrecen palabras clave.
        let e = typed("x.i", "a.py");
        assert_eq!(labels(&e), ["index", "insert", "items"]);
        assert!(typed("i", "a.py").completions.is_empty());
        let e = typed("v.unw", "a.rs");
        assert_eq!(e.completions[0].label, "unwrap");
        assert!(typed("for i in 0..n", "a.rs").completions.is_empty());
    }

    #[test]
    fn snippets_expand_with_the_file_indentation() {
        let mut e = typed("def f():\n  x = 1\n  for", "a.py");
        assert_eq!(e.completions[0].kind, "snippet");
        assert_eq!(e.completions[0].label, "for");
        e.accept_completion();
        assert_eq!(e.text(), "def f():\n  x = 1\n  for  in :\n    pass");
        assert_eq!(e.cursor, Pos::new(2, 6));
        e.update_completion();
        assert!(e.completions.is_empty());
        let mut e = typed("mai", "a.rs");
        assert_eq!(labels(&e), ["main"]);
        e.accept_completion();
        assert_eq!(e.text(), "fn main() {\n    \n}");
        assert_eq!(e.cursor, Pos::new(1, 4));
        e.undo(false);
        assert_eq!(e.text(), "mai");
    }

    #[test]
    fn comments_and_strings_only_offer_document_words() {
        assert!(typed("// pri", "a.rs").completions.is_empty());
        assert!(typed("x = \"pri", "a.py").completions.is_empty());
        assert_eq!(labels(&typed("precio = 1\n# pre", "a.py")), ["precio"]);
        // Un tiempo de vida no abre una cadena.
        assert!(labels(&typed("fn f<'a>(x: &'a str) -> Str", "a.rs")).contains(&"String"));
    }

    #[test]
    fn requested_completion_needs_no_prefix() {
        let mut e = typed("total = 1\ntotal.", "a.py");
        assert!(e.completions.is_empty());
        e.request_completion();
        e.update_completion();
        assert_eq!(e.completions[0].label, "add");
        assert_eq!(e.completions[0].detail, "Miembro habitual");
        e.insert("x");
        e.update_completion();
        assert!(e.completions.is_empty());
    }
}
