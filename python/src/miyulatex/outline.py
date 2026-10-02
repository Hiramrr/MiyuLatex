"""Análisis ligero del documento: esquema, etiquetas, claves y estadísticas."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

from .highlight import arg_span

LEVELS = {
    "part": 0,
    "chapter": 1,
    "section": 2,
    "subsection": 3,
    "subsubsection": 4,
    "paragraph": 5,
}

_SECTION = re.compile(r"\\(part|chapter|section|subsection|subsubsection|paragraph)\*?(?![a-zA-Z])")
_LABEL = re.compile(r"\\label\{([^}]+)\}")
_BIB_KEY = re.compile(r"@\w+\s*\{\s*([^,\s]+)\s*,")
_BIBITEM = re.compile(r"\\bibitem(?:\[[^\]]*\])?\{([^}]+)\}")
_COMMENT = re.compile(r"(?<!\\)%.*")
_COMMAND = re.compile(r"\\[a-zA-Z@]+\*?(\[[^\]]*\])?")
_WORD = re.compile(r"[^\W\d_]+(?:['’-][^\W\d_]+)*")


@dataclass(frozen=True)
class OutlineItem:
    level: int
    kind: str
    title: str
    line: int
    """Línea (base 0) donde aparece el comando."""


def strip_comment(line: str) -> str:
    return _COMMENT.sub("", line)


def parse_outline(text: str) -> list[OutlineItem]:
    """Extrae la jerarquía de secciones del documento."""
    items: list[OutlineItem] = []
    for number, raw in enumerate(text.split("\n")):
        if "\\" not in raw:
            continue
        line = strip_comment(raw)
        for match in _SECTION.finditer(line):
            arg = arg_span(line, match.end())
            if arg is None:
                continue
            title = _clean_title(line[arg[0] : arg[1]])
            kind = match.group(1)
            items.append(OutlineItem(LEVELS[kind], kind, title or "(sin título)", number))
    return items


def _clean_title(title: str) -> str:
    title = _COMMAND.sub("", title)
    return re.sub(r"\s+", " ", title.replace("{", "").replace("}", "")).strip()


def find_labels(text: str) -> list[str]:
    """Todas las etiquetas ``\\label{...}`` del documento, sin repetir."""
    seen: dict[str, None] = {}
    for line in text.split("\n"):
        for match in _LABEL.finditer(strip_comment(line)):
            seen.setdefault(match.group(1))
    return list(seen)


def find_cite_keys(text: str, directory: Path | None) -> list[str]:
    """Claves bibliográficas: ``\\bibitem`` del documento y ``.bib`` de la carpeta."""
    seen: dict[str, None] = {}
    for match in _BIBITEM.finditer(text):
        seen.setdefault(match.group(1))
    if directory is not None and directory.is_dir():
        for bib in sorted(directory.glob("*.bib"))[:20]:
            try:
                content = bib.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue
            for match in _BIB_KEY.finditer(content):
                seen.setdefault(match.group(1))
    return list(seen)


def count_words(text: str) -> int:
    """Cuenta aproximada de palabras, ignorando comentarios, comandos y preámbulo."""
    start = text.find("\\begin{document}")
    if start != -1:
        text = text[start + len("\\begin{document}") :]
    end = text.find("\\end{document}")
    if end != -1:
        text = text[:end]
    text = "\n".join(strip_comment(line) for line in text.split("\n"))
    text = re.sub(r"\\(begin|end|label|ref|eqref|cite\w*|input|include\w*)\*?(\[[^\]]*\])?\{[^}]*\}", " ", text)
    text = _COMMAND.sub(" ", text)
    return len(_WORD.findall(text))
