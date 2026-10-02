"""Resaltado de sintaxis LaTeX, línea a línea y con estado.

El tokenizador es una función pura de ``(línea, estado)``, así que se cachea:
tras cada edición solo se recalculan de verdad las líneas que cambiaron.
"""

from __future__ import annotations

import re
from functools import lru_cache
from typing import NamedTuple

Span = tuple[int, int, str]
"""``(inicio, fin, nombre_de_estilo)`` en columnas de caracteres."""

MATH_ENVS = frozenset(
    {
        "equation", "align", "gather", "multline", "eqnarray", "displaymath",
        "math", "flalign", "alignat", "dmath", "split",
    }
)
VERBATIM_ENVS = frozenset(
    {"verbatim", "verbatim*", "Verbatim", "lstlisting", "minted", "comment"}
)
SECTION_COMMANDS = frozenset(
    {
        "part", "chapter", "section", "subsection", "subsubsection",
        "paragraph", "subparagraph",
    }
)

# Comando -> estilo que recibe su argumento entre llaves.
ARG_STYLES: dict[str, str] = {
    **{name: "title" for name in SECTION_COMMANDS},
    "title": "title",
    "author": "title",
    "caption": "string",
    "textbf": "bold",
    "textit": "italic",
    "emph": "italic",
    "textsl": "italic",
    "underline": "underline",
    "texttt": "verbatim",
    "url": "ref",
    "label": "ref",
    "ref": "ref",
    "eqref": "ref",
    "pageref": "ref",
    "autoref": "ref",
    "cref": "ref",
    "Cref": "ref",
    "cite": "ref",
    "citep": "ref",
    "citet": "ref",
    "parencite": "ref",
    "textcite": "ref",
    "nocite": "ref",
    "input": "module",
    "include": "module",
    "includegraphics": "module",
    "bibliography": "module",
    "addbibresource": "module",
    "usepackage": "module",
    "RequirePackage": "module",
    "documentclass": "module",
    "bibliographystyle": "module",
}

_TOKEN = re.compile(
    r"""
      (?P<comment>%.*$)
    | (?P<begin>\\(?:begin|end)\s*\{(?P<envname>[^{}]*)\})
    | (?P<verb>\\verb\*?(?P<vd>[^a-zA-Z*\s]).*?(?P=vd))
    | (?P<mathdelim>\$\$|\\\[|\\\]|\\\(|\\\)|\$)
    | (?P<command>\\(?:[a-zA-Z@]+\*?|.))
    | (?P<brace>[{}])
    | (?P<bracket>[\[\]])
    | (?P<special>[&~^_\#])
    """,
    re.VERBOSE,
)

_OPENERS = {"$$": "$$", "$": "$", "\\(": "\\(", "\\[": "\\["}
_CLOSERS = {"$": ("$", "$$"), "$$": ("$$",), "\\(": ("\\)",), "\\[": ("\\]",)}


class State(NamedTuple):
    """Estado que se arrastra de una línea a la siguiente."""

    math: str | None = None
    verbatim: str | None = None


START = State()


def arg_span(line: str, pos: int) -> tuple[int, int] | None:
    """Localiza el contenido del primer ``{...}`` a partir de ``pos``.

    Salta espacios y argumentos opcionales ``[...]``. Si la llave no se cierra
    en la línea, el tramo llega hasta el final.
    """
    n = len(line)
    while pos < n:
        ch = line[pos]
        if ch in " \t*":
            pos += 1
        elif ch == "[":
            close = line.find("]", pos)
            if close == -1:
                return None
            pos = close + 1
        else:
            break
    if pos >= n or line[pos] != "{":
        return None
    depth = 0
    i = pos
    while i < n:
        ch = line[i]
        if ch == "\\":
            i += 2
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return pos + 1, i
        i += 1
    return pos + 1, n


@lru_cache(maxsize=32768)
def tokenize_line(line: str, state: State = START) -> tuple[tuple[Span, ...], State]:
    """Tokeniza una línea y devuelve sus tramos y el estado resultante."""
    math, verbatim = state
    n = len(line)
    background: list[Span] = []
    spans: list[Span] = []
    pos = 0

    if verbatim is not None:
        end_tag = f"\\end{{{verbatim}}}"
        idx = line.find(end_tag)
        if idx == -1:
            return (((0, n, "verbatim"),) if n else ()), state
        if idx:
            spans.append((0, idx, "verbatim"))
        spans.append((idx, idx + 4, "env"))
        spans.append((idx + 5, idx + len(end_tag) - 1, "env.name"))
        pos = idx + len(end_tag)
        verbatim = None

    # Las matemáticas en línea no sobreviven a un salto de párrafo.
    if math in ("$", "\\(") and not line.strip():
        math = None

    math_start = pos if math else -1
    math_end = n

    for match in _TOKEN.finditer(line, pos):
        kind = match.lastgroup
        start, end = match.span()
        token = match.group()

        if kind == "comment":
            spans.append((start, n, "comment"))
            math_end = start
            break

        if kind == "begin":
            is_begin = token.startswith("\\begin")
            name = match.group("envname")
            spans.append((start, start + (6 if is_begin else 4), "env"))
            spans.append((match.start("envname"), match.end("envname"), "env.name"))
            base = name.rstrip("*")
            if is_begin and name in VERBATIM_ENVS:
                if end < n:
                    spans.append((end, n, "verbatim"))
                if math:
                    background.append((math_start, start, "math"))
                return tuple(background + spans), State(None, name)
            if base in MATH_ENVS:
                if is_begin and math is None:
                    math, math_start = base, end
                elif not is_begin and math == base:
                    background.append((math_start, start, "math"))
                    math = None

        elif kind == "verb":
            spans.append((start, end, "verbatim"))

        elif kind == "mathdelim":
            spans.append((start, end, "math.delim"))
            if math is None:
                if token in _OPENERS:
                    math, math_start = _OPENERS[token], start
            elif token in _CLOSERS.get(math, ()):
                background.append((math_start, end, "math"))
                math = None

        elif kind == "command":
            name = token[1:].rstrip("*")
            if math:
                spans.append((start, end, "math.command"))
            elif name in SECTION_COMMANDS:
                spans.append((start, end, "section"))
            elif name == "item":
                spans.append((start, end, "item"))
            elif not name.isalpha() and "@" not in name:
                spans.append((start, end, "special"))
            else:
                spans.append((start, end, "command"))
            style = ARG_STYLES.get(name)
            if style and not math:
                arg = arg_span(line, end)
                if arg and arg[1] > arg[0]:
                    background.append((arg[0], arg[1], style))

        elif kind == "brace":
            spans.append((start, end, "brace"))

        elif kind == "bracket":
            if not math:
                spans.append((start, end, "bracket"))

        elif kind == "special":
            spans.append((start, end, "special"))

    if math and math_end > math_start:
        background.append((math_start, math_end, "math"))

    return tuple(background + spans), State(math, verbatim)


def tokenize(lines: list[str]) -> list[tuple[Span, ...]]:
    """Tokeniza un documento completo."""
    state = START
    result: list[tuple[Span, ...]] = []
    for line in lines:
        spans, state = tokenize_line(line, state)
        result.append(spans)
    return result
