"""Detección de motores LaTeX, compilación asíncrona y lectura de errores."""

from __future__ import annotations

import asyncio
import os
import re
import shutil
import time
from dataclasses import dataclass, field
from pathlib import Path

ENGINES = ("tectonic", "latexmk", "pdflatex", "xelatex", "lualatex")

# Rutas habituales que una terminal puede no tener en el PATH.
_EXTRA_PATHS = (
    "/Library/TeX/texbin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
    str(Path.home() / ".cargo" / "bin"),
    str(Path.home() / ".local" / "bin"),
)

AUX_SUFFIXES = (
    ".aux", ".log", ".out", ".toc", ".lof", ".lot", ".fls", ".fdb_latexmk",
    ".synctex.gz", ".bbl", ".blg", ".nav", ".snm", ".vrb", ".bcf", ".run.xml",
    ".xdv",
)

TIMEOUT = 240.0


@dataclass(frozen=True)
class Problem:
    severity: str
    """``error``, ``warning`` o ``info``."""
    message: str
    file: str | None = None
    line: int | None = None
    """Línea en base 1, como la informa LaTeX."""


@dataclass
class CompileResult:
    ok: bool
    engine: str
    root: Path
    pdf: Path | None
    duration: float
    problems: list[Problem] = field(default_factory=list)
    output: str = ""

    @property
    def errors(self) -> int:
        return sum(1 for p in self.problems if p.severity == "error")

    @property
    def warnings(self) -> int:
        return sum(1 for p in self.problems if p.severity == "warning")


def which(program: str) -> str | None:
    """Como ``shutil.which`` pero mirando también las rutas típicas de TeX."""
    found = shutil.which(program)
    if found:
        return found
    for directory in _EXTRA_PATHS:
        candidate = Path(directory) / program
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return str(candidate)
    return None


def detect_engines() -> dict[str, str]:
    """Motores disponibles, en orden de preferencia: ``nombre -> ejecutable``."""
    return {name: path for name in ENGINES if (path := which(name))}


_MAGIC_ROOT = re.compile(r"^\s*%+\s*!\s*TEX\s+root\s*=\s*(.+?)\s*$", re.IGNORECASE | re.MULTILINE)


def find_root(path: Path, text: str) -> Path:
    """Decide qué archivo hay que compilar al editar ``path``.

    Respeta ``% !TEX root = ...``; si el archivo no tiene ``\\documentclass``
    busca un documento principal en la misma carpeta o en la superior.
    """
    magic = _MAGIC_ROOT.search(text[:2000])
    if magic:
        candidate = (path.parent / magic.group(1)).resolve()
        if candidate.is_file():
            return candidate
    if "\\documentclass" in text:
        return path
    for directory in (path.parent, path.parent.parent):
        for name in ("main.tex", "principal.tex", "tesis.tex", "thesis.tex"):
            candidate = directory / name
            if candidate.is_file():
                return candidate
        for candidate in sorted(directory.glob("*.tex")):
            try:
                if "\\documentclass" in candidate.read_text(encoding="utf-8", errors="replace")[:4000]:
                    return candidate
            except OSError:
                continue
    return path


def build_command(engine: str, executable: str, root: Path) -> list[str]:
    name = root.name
    if engine == "tectonic":
        return [executable, "-X", "compile", name]
    if engine == "latexmk":
        return [
            executable, "-pdf", "-interaction=nonstopmode", "-file-line-error", name,
        ]
    return [executable, "-interaction=nonstopmode", "-file-line-error", name]


_TECTONIC = re.compile(r"^(error|warning): (?:([^\s:]+\.\w+):(?:(\d+):)?\s*)?(.*)$")
# Líneas de tectonic que acompañan a un problema pero no aportan nada por sí solas.
_TECTONIC_NOISE = (
    "halted on potentially-recoverable",
    "the Tectonic backend",
    "something bad happened inside",
    "engine had an unrecoverable error",
    "could not represent character",
    "you may need to load the `fontspec`",
    "choose a different font that covers",
)
_FILE_LINE = re.compile(r"^(\.{0,2}/?[^:\s][^:]*\.(?:tex|sty|cls|bib|ltx|tikz)):(\d+): (.*)$")
_BANG = re.compile(r"^! (.*)$")
_LINE_REF = re.compile(r"^l\.(\d+) ")
_WARNING = re.compile(r"^(?:LaTeX|Package|Class)(?: (\S+))? Warning: (.*)$")
_INPUT_LINE = re.compile(r"on input line (\d+)")
_BADBOX = re.compile(r"^((?:Over|Under)full \\[hv]box .*?) (?:in paragraph )?at lines? (\d+)")
_NOISE = ("Rerun to get", "There were undefined references", "Label(s) may have changed")


def parse_output(output: str, root: Path | None = None) -> list[Problem]:
    """Convierte la salida de cualquier motor en una lista de problemas."""
    problems: list[Problem] = []
    seen: dict[tuple, int] = {}
    default_file = root.name if root else None
    lines = output.splitlines()

    def add(severity: str, message: str, file: str | None, line: int | None) -> None:
        message = message.strip().lstrip("!").strip()
        if not message:
            return
        if "nullfont" in message:
            severity = "info"
        # El mismo error suele llegar dos veces: resumido y dentro del registro de TeX.
        key = (severity, message.rstrip("."))
        if key in seen:
            index = seen[key]
            if problems[index].line is None and line is not None:
                problems[index] = Problem(severity, message, file or default_file, line)
            return
        seen[key] = len(problems)
        problems.append(Problem(severity, message, file or default_file, line))

    index = 0
    while index < len(lines):
        line = lines[index].rstrip()
        index += 1

        if match := _TECTONIC.match(line):
            severity, file, number, message = match.groups()
            if any(noise in message for noise in _TECTONIC_NOISE):
                continue
            if file is None and (warn := _WARNING.match(message)):
                message = warn.group(2)
            line_no = int(number) if number else None
            if line_no is None and (ref := _INPUT_LINE.search(message)):
                line_no = int(ref.group(1))
            add(severity, message, file, line_no)
            continue

        if match := _FILE_LINE.match(line):
            file, number, message = match.groups()
            add("error", message, file, int(number))
            continue

        if match := _BANG.match(line):
            message = match.group(1)
            line_no = None
            for ahead in lines[index : index + 12]:
                if ref := _LINE_REF.match(ahead):
                    line_no = int(ref.group(1))
                    break
            add("error", message, None, line_no)
            continue

        if match := _WARNING.match(line):
            message = match.group(2)
            # Los avisos largos continúan en las líneas siguientes.
            while index < len(lines) and lines[index].strip() and not message.rstrip().endswith("."):
                message += " " + lines[index].strip().removeprefix(f"({match.group(1)})").strip()
                index += 1
                if len(message) > 300:
                    break
            if any(noise in message for noise in _NOISE):
                continue
            ref = _INPUT_LINE.search(message)
            add("warning", message, None, int(ref.group(1)) if ref else None)
            continue

        if match := _BADBOX.match(line):
            add("info", match.group(1), None, int(match.group(2)))

    order = {"error": 0, "warning": 1, "info": 2}
    problems.sort(key=lambda p: order.get(p.severity, 3))
    return problems


async def _run(command: list[str], cwd: Path) -> tuple[int, str]:
    process = await asyncio.create_subprocess_exec(
        *command,
        cwd=cwd,
        stdin=asyncio.subprocess.DEVNULL,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.STDOUT,
    )
    try:
        raw, _ = await asyncio.wait_for(process.communicate(), timeout=TIMEOUT)
    except asyncio.TimeoutError:
        process.kill()
        await process.wait()
        return 124, f"error: la compilación superó los {TIMEOUT:.0f} s y se canceló"
    except asyncio.CancelledError:
        process.kill()
        await process.wait()
        raise
    return process.returncode or 0, raw.decode("utf-8", errors="replace")


def _needs_bibliography(root: Path) -> str | None:
    aux = root.with_suffix(".aux")
    try:
        content = aux.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None
    if root.with_suffix(".bcf").exists():
        return "biber"
    if "\\bibdata" in content and "\\citation" in content:
        return "bibtex"
    return None


async def compile_document(root: Path, engine: str, executable: str) -> CompileResult:
    """Compila ``root`` y devuelve el resultado con los problemas ya analizados."""
    started = time.monotonic()
    cwd = root.parent
    command = build_command(engine, executable, root)
    pdf = root.with_suffix(".pdf")
    before = pdf.stat().st_mtime_ns if pdf.exists() else None

    code, output = await _run(command, cwd)

    # Los motores clásicos necesitan pasadas extra; tectonic y latexmk no.
    if engine in ("pdflatex", "xelatex", "lualatex") and code == 0:
        tool = _needs_bibliography(root)
        if tool and (tool_path := which(tool)):
            _, bib_output = await _run([tool_path, root.stem], cwd)
            output += "\n" + bib_output
            code, output_2 = await _run(command, cwd)
            output += "\n" + output_2
        for _ in range(2):
            if "Rerun to get" not in output:
                break
            code, output = await _run(command, cwd)

    after = pdf.stat().st_mtime_ns if pdf.exists() else None
    produced = after is not None and after != before
    problems = parse_output(output, root)
    has_errors = any(p.severity == "error" for p in problems)
    ok = code == 0 and pdf.exists()
    if code != 0 and not has_errors:
        tail = next((ln for ln in reversed(output.splitlines()) if ln.strip()), "sin salida")
        problems.insert(0, Problem("error", f"{engine} terminó con código {code}: {tail.strip()}", root.name))

    return CompileResult(
        ok=ok,
        engine=engine,
        root=root,
        pdf=pdf if (ok or produced) else None,
        duration=time.monotonic() - started,
        problems=problems,
        output=output,
    )


def clean_auxiliary(root: Path) -> list[Path]:
    """Borra los archivos auxiliares generados junto a ``root``."""
    removed: list[Path] = []
    for suffix in AUX_SUFFIXES:
        candidate = root.parent / (root.stem + suffix)
        if candidate.is_file():
            try:
                candidate.unlink()
                removed.append(candidate)
            except OSError:
                pass
    return removed
