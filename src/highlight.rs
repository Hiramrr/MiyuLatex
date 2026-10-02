//! Resaltado de sintaxis LaTeX, línea a línea y con estado.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tok {
    Comment,
    Command,
    Section,
    Title,
    Item,
    Env,
    EnvName,
    Math,
    MathDelim,
    MathCommand,
    Brace,
    Bracket,
    Special,
    Ref,
    Module,
    Str,
    Verbatim,
    Bold,
    Italic,
    Underline,
}

/// Tramo `[start, end)` en columnas de caracteres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub tok: Tok,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Math {
    Dollar,
    DoubleDollar,
    Paren,
    Bracket,
    Env(String),
}

/// Estado que se arrastra de una línea a la siguiente.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub math: Option<Math>,
    pub verbatim: Option<String>,
}

const MATH_ENVS: &[&str] = &[
    "equation",
    "align",
    "gather",
    "multline",
    "eqnarray",
    "displaymath",
    "math",
    "flalign",
    "alignat",
    "dmath",
    "split",
];
const VERBATIM_ENVS: &[&str] = &[
    "verbatim",
    "verbatim*",
    "Verbatim",
    "lstlisting",
    "minted",
    "comment",
];
pub const SECTION_COMMANDS: &[&str] = &[
    "part",
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
];

/// Estilo que recibe el argumento entre llaves de un comando.
fn arg_style(name: &str) -> Option<Tok> {
    Some(match name {
        "part" | "chapter" | "section" | "subsection" | "subsubsection" | "paragraph"
        | "subparagraph" | "title" | "author" => Tok::Title,
        "caption" => Tok::Str,
        "textbf" => Tok::Bold,
        "textit" | "emph" | "textsl" => Tok::Italic,
        "underline" => Tok::Underline,
        "texttt" => Tok::Verbatim,
        "url" | "label" | "ref" | "eqref" | "pageref" | "autoref" | "cref" | "Cref" | "cite"
        | "citep" | "citet" | "parencite" | "textcite" | "nocite" => Tok::Ref,
        "input" | "include" | "includegraphics" | "bibliography" | "addbibresource"
        | "usepackage" | "RequirePackage" | "documentclass" | "bibliographystyle" => Tok::Module,
        _ => return None,
    })
}

/// Localiza el contenido del primer `{...}` a partir de `pos`, saltando
/// espacios y argumentos opcionales `[...]`. Si la llave no se cierra en la
/// línea, el tramo llega hasta el final.
pub fn arg_span(chars: &[char], mut pos: usize) -> Option<(usize, usize)> {
    let n = chars.len();
    while pos < n {
        match chars[pos] {
            ' ' | '\t' | '*' => pos += 1,
            '[' => {
                let close = chars[pos..].iter().position(|&c| c == ']')?;
                pos += close + 1;
            }
            _ => break,
        }
    }
    if pos >= n || chars[pos] != '{' {
        return None;
    }
    let mut depth = 0usize;
    let mut i = pos;
    while i < n {
        match chars[i] {
            '\\' => {
                i += 2;
                continue;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((pos + 1, i));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Some((pos + 1, n))
}

fn find_sub(chars: &[char], from: usize, needle: &[char]) -> Option<usize> {
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    (from..=chars.len() - needle.len()).find(|&i| chars[i..i + needle.len()] == *needle)
}

/// `\begin{nombre}` o `\end{nombre}` en `pos`: devuelve (es_begin, inicio y fin del nombre, fin total).
fn begin_end(chars: &[char], pos: usize) -> Option<(bool, usize, usize, usize)> {
    let rest = &chars[pos..];
    let is_begin = rest.starts_with(&['\\', 'b', 'e', 'g', 'i', 'n']);
    let is_end = rest.starts_with(&['\\', 'e', 'n', 'd']);
    if !is_begin && !is_end {
        return None;
    }
    let mut i = pos + if is_begin { 6 } else { 4 };
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() || chars[i] != '{' {
        return None;
    }
    let name_start = i + 1;
    let mut j = name_start;
    while j < chars.len() && chars[j] != '}' {
        if chars[j] == '{' {
            return None;
        }
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    Some((is_begin, name_start, j, j + 1))
}

/// Tokeniza una línea y devuelve sus tramos y el estado resultante. Los tramos
/// de fondo (matemáticas, argumentos) van primero para que los demás se pinten encima.
pub fn tokenize_line(line: &str, state: &State) -> (Vec<Span>, State) {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut math = state.math.clone();
    let mut verbatim = state.verbatim.clone();
    let mut background: Vec<Span> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut pos = 0;

    if let Some(name) = verbatim.clone() {
        let tag: Vec<char> = format!("\\end{{{name}}}").chars().collect();
        match find_sub(&chars, 0, &tag) {
            None => {
                if n > 0 {
                    spans.push(Span {
                        start: 0,
                        end: n,
                        tok: Tok::Verbatim,
                    });
                }
                return (spans, state.clone());
            }
            Some(idx) => {
                if idx > 0 {
                    spans.push(Span {
                        start: 0,
                        end: idx,
                        tok: Tok::Verbatim,
                    });
                }
                spans.push(Span {
                    start: idx,
                    end: idx + 4,
                    tok: Tok::Env,
                });
                spans.push(Span {
                    start: idx + 5,
                    end: idx + tag.len() - 1,
                    tok: Tok::EnvName,
                });
                pos = idx + tag.len();
                verbatim = None;
            }
        }
    }

    // Las matemáticas en línea no sobreviven a un salto de párrafo.
    if matches!(math, Some(Math::Dollar) | Some(Math::Paren)) && line.trim().is_empty() {
        math = None;
    }

    let mut math_start = pos;
    let mut math_end = n;
    let mut i = pos;
    while i < n {
        let c = chars[i];
        match c {
            '%' => {
                spans.push(Span {
                    start: i,
                    end: n,
                    tok: Tok::Comment,
                });
                math_end = i;
                break;
            }
            '\\' => {
                if let Some((is_begin, name_start, name_end, end)) = begin_end(&chars, i) {
                    let name: String = chars[name_start..name_end].iter().collect();
                    spans.push(Span {
                        start: i,
                        end: i + if is_begin { 6 } else { 4 },
                        tok: Tok::Env,
                    });
                    spans.push(Span {
                        start: name_start,
                        end: name_end,
                        tok: Tok::EnvName,
                    });
                    let base = name.trim_end_matches('*');
                    if is_begin && VERBATIM_ENVS.contains(&name.as_str()) {
                        if end < n {
                            spans.push(Span {
                                start: end,
                                end: n,
                                tok: Tok::Verbatim,
                            });
                        }
                        if math.is_some() {
                            background.push(Span {
                                start: math_start,
                                end: i,
                                tok: Tok::Math,
                            });
                        }
                        background.extend(spans);
                        return (
                            background,
                            State {
                                math: None,
                                verbatim: Some(name),
                            },
                        );
                    }
                    if MATH_ENVS.contains(&base) {
                        if is_begin && math.is_none() {
                            math = Some(Math::Env(base.to_string()));
                            math_start = end;
                        } else if !is_begin && math == Some(Math::Env(base.to_string())) {
                            background.push(Span {
                                start: math_start,
                                end: i,
                                tok: Tok::Math,
                            });
                            math = None;
                        }
                    }
                    i = end;
                    continue;
                }
                // \verb|...|
                if chars[i..].starts_with(&['\\', 'v', 'e', 'r', 'b']) {
                    let mut d = i + 5;
                    if d < n && chars[d] == '*' {
                        d += 1;
                    }
                    if d < n
                        && !chars[d].is_alphabetic()
                        && !chars[d].is_whitespace()
                        && let Some(close) = chars[d + 1..].iter().position(|&x| x == chars[d])
                    {
                        let end = d + 1 + close + 1;
                        spans.push(Span {
                            start: i,
                            end,
                            tok: Tok::Verbatim,
                        });
                        i = end;
                        continue;
                    }
                }
                let next = chars.get(i + 1).copied();
                // Delimitadores matemáticos \[ \] \( \)
                if let Some(d @ ('[' | ']' | '(' | ')')) = next {
                    spans.push(Span {
                        start: i,
                        end: i + 2,
                        tok: Tok::MathDelim,
                    });
                    match (&math, d) {
                        (None, '[') => {
                            math = Some(Math::Bracket);
                            math_start = i;
                        }
                        (None, '(') => {
                            math = Some(Math::Paren);
                            math_start = i;
                        }
                        (Some(Math::Bracket), ']') | (Some(Math::Paren), ')') => {
                            background.push(Span {
                                start: math_start,
                                end: i + 2,
                                tok: Tok::Math,
                            });
                            math = None;
                        }
                        _ => {}
                    }
                    i += 2;
                    continue;
                }
                // Comando: \nombre* o \ seguido de un carácter cualquiera.
                let mut end = i + 1;
                if next.is_some_and(|x| x.is_ascii_alphabetic() || x == '@') {
                    while end < n && (chars[end].is_ascii_alphabetic() || chars[end] == '@') {
                        end += 1;
                    }
                    if end < n && chars[end] == '*' {
                        end += 1;
                    }
                } else if next.is_some() {
                    end = i + 2;
                }
                let name: String = chars[i + 1..end].iter().collect();
                let name = name.trim_end_matches('*');
                let alpha = !name.is_empty() && name.chars().all(|x| x.is_alphabetic() || x == '@');
                let tok = if math.is_some() {
                    Tok::MathCommand
                } else if SECTION_COMMANDS.contains(&name) {
                    Tok::Section
                } else if name == "item" {
                    Tok::Item
                } else if !alpha {
                    Tok::Special
                } else {
                    Tok::Command
                };
                spans.push(Span { start: i, end, tok });
                if math.is_none()
                    && let Some(style) = arg_style(name)
                    && let Some((a, b)) = arg_span(&chars, end)
                    && b > a
                {
                    background.push(Span {
                        start: a,
                        end: b,
                        tok: style,
                    });
                }
                i = end;
                continue;
            }
            '$' => {
                let double = chars.get(i + 1) == Some(&'$');
                let end = if double { i + 2 } else { i + 1 };
                spans.push(Span {
                    start: i,
                    end,
                    tok: Tok::MathDelim,
                });
                match &math {
                    None => {
                        math = Some(if double {
                            Math::DoubleDollar
                        } else {
                            Math::Dollar
                        });
                        math_start = i;
                    }
                    Some(Math::Dollar) => {
                        background.push(Span {
                            start: math_start,
                            end,
                            tok: Tok::Math,
                        });
                        math = None;
                    }
                    Some(Math::DoubleDollar) if double => {
                        background.push(Span {
                            start: math_start,
                            end,
                            tok: Tok::Math,
                        });
                        math = None;
                    }
                    _ => {}
                }
                i = end;
                continue;
            }
            '{' | '}' => spans.push(Span {
                start: i,
                end: i + 1,
                tok: Tok::Brace,
            }),
            '[' | ']' if math.is_none() => {
                spans.push(Span {
                    start: i,
                    end: i + 1,
                    tok: Tok::Bracket,
                });
            }
            '&' | '~' | '^' | '_' | '#' => spans.push(Span {
                start: i,
                end: i + 1,
                tok: Tok::Special,
            }),
            _ => {}
        }
        i += 1;
    }

    if math.is_some() && math_end > math_start {
        background.push(Span {
            start: math_start,
            end: math_end,
            tok: Tok::Math,
        });
    }
    background.extend(spans);
    (background, State { math, verbatim })
}

/// Tokeniza un documento completo.
pub fn tokenize(lines: &[String]) -> Vec<Vec<Span>> {
    let mut state = State::default();
    lines
        .iter()
        .map(|line| {
            let (spans, next) = tokenize_line(line, &state);
            state = next;
            spans
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(line: &str) -> Vec<(String, Tok)> {
        let chars: Vec<char> = line.chars().collect();
        tokenize_line(line, &State::default())
            .0
            .iter()
            .map(|s| (chars[s.start..s.end].iter().collect(), s.tok))
            .collect()
    }

    fn has(line: &str, text: &str, tok: Tok) -> bool {
        names(line).iter().any(|(t, k)| t == text && *k == tok)
    }

    #[test]
    fn commands_and_arguments() {
        let line = r"\section{Intro} \textbf{hola} % nota";
        assert!(has(line, r"\section", Tok::Section));
        assert!(has(line, "Intro", Tok::Title));
        assert!(has(line, r"\textbf", Tok::Command));
        assert!(has(line, "hola", Tok::Bold));
        assert!(has(line, "% nota", Tok::Comment));
    }

    #[test]
    fn escaped_percent_is_not_a_comment() {
        let line = r"un 50\% de \emph{x}";
        assert!(!names(line).iter().any(|(_, k)| *k == Tok::Comment));
        assert!(has(line, r"\%", Tok::Special));
    }

    #[test]
    fn inline_math() {
        let line = r"sea $x^2 + \alpha$ fin";
        let (spans, state) = tokenize_line(line, &State::default());
        assert!(state.math.is_none());
        assert!(spans.contains(&Span {
            start: 4,
            end: 18,
            tok: Tok::Math
        }));
        assert!(has(line, r"\alpha", Tok::MathCommand));
    }

    #[test]
    fn math_environment_spans_lines() {
        let lines: Vec<String> = [
            r"\begin{align*}",
            r"a &= \frac{1}{2}",
            r"\end{align*}",
            "texto",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let result = tokenize(&lines);
        assert!(result[1].contains(&Span {
            start: 0,
            end: 16,
            tok: Tok::Math
        }));
        assert!(!result[3].iter().any(|s| s.tok == Tok::Math));
    }

    #[test]
    fn verbatim_is_not_tokenized() {
        let lines: Vec<String> = [r"\begin{verbatim}", r"\foo $x$ % y", r"\end{verbatim} \bar"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let result = tokenize(&lines);
        assert_eq!(
            result[1],
            vec![Span {
                start: 0,
                end: 12,
                tok: Tok::Verbatim
            }]
        );
        assert!(result[2].contains(&Span {
            start: 15,
            end: 19,
            tok: Tok::Command
        }));
    }

    #[test]
    fn inline_math_ends_at_paragraph_break() {
        let (_, state) = tokenize_line("precio: $5", &State::default());
        assert_eq!(state.math, Some(Math::Dollar));
        let (_, state) = tokenize_line("", &state);
        assert!(state.math.is_none());
    }
}
