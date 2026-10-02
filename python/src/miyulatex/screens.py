"""Diálogos modales."""

from __future__ import annotations

from pathlib import Path

from rich.table import Table
from rich.text import Text
from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label, OptionList, Static
from textual.widgets.option_list import Option

from .snippets import SYMBOLS, TEMPLATES, Symbol, Template

TEXT_SUFFIXES = {".tex", ".bib", ".sty", ".cls", ".ltx", ".tikz", ".txt", ".md", ".bst"}
SKIP_DIRS = {".git", ".venv", "node_modules", "__pycache__", ".idea", ".vscode", "build", "dist"}


class Dialog(ModalScreen):
    """Base de los diálogos: Escape cierra sin resultado."""

    BINDINGS = [Binding("escape", "dismiss(None)", "Cerrar", show=False)]

    def _move(self, delta: int) -> None:
        options = self.query_one(OptionList)
        if options.option_count:
            options.highlighted = ((options.highlighted or 0) + delta) % options.option_count

    def on_key(self, event) -> None:
        # Las flechas mueven la lista aunque el foco esté en el campo de texto.
        if event.key in ("down", "up") and isinstance(self.focused, Input) and self.query(OptionList):
            self._move(1 if event.key == "down" else -1)
            event.stop()
            event.prevent_default()


class ConfirmScreen(Dialog):
    """Pregunta con botones; devuelve la clave del botón pulsado."""

    def __init__(self, title: str, message: str, buttons: list[tuple[str, str, str]]) -> None:
        super().__init__()
        self._title = title
        self._message = message
        self._buttons = buttons

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog dialog-small"):
            yield Label(self._title, classes="dialog-title")
            yield Static(self._message, classes="dialog-body")
            with Horizontal(classes="dialog-buttons"):
                for key, label, variant in self._buttons:
                    yield Button(label, id=key, variant=variant)

    @on(Button.Pressed)
    def _pressed(self, event: Button.Pressed) -> None:
        self.dismiss(event.button.id)


class InputScreen(Dialog):
    """Pide una línea de texto."""

    def __init__(self, title: str, placeholder: str = "", value: str = "", hint: str = "") -> None:
        super().__init__()
        self._title = title
        self._placeholder = placeholder
        self._value = value
        self._hint = hint

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog dialog-small"):
            yield Label(self._title, classes="dialog-title")
            yield Input(value=self._value, placeholder=self._placeholder)
            if self._hint:
                yield Static(self._hint, classes="dialog-hint")

    @on(Input.Submitted)
    def _submitted(self, event: Input.Submitted) -> None:
        self.dismiss(event.value.strip() or None)


def list_project_files(root: Path, limit: int = 2000) -> list[Path]:
    """Archivos de texto del proyecto, relativos a ``root``."""
    found: list[Path] = []
    stack = [root]
    while stack and len(found) < limit:
        directory = stack.pop()
        try:
            entries = sorted(directory.iterdir(), key=lambda p: p.name.lower())
        except OSError:
            continue
        for entry in entries:
            if entry.name.startswith("."):
                continue
            if entry.is_dir():
                if entry.name not in SKIP_DIRS:
                    stack.append(entry)
            elif entry.suffix.lower() in TEXT_SUFFIXES:
                found.append(entry.relative_to(root))
    found.sort(key=lambda p: (len(p.parts), str(p).lower()))
    return found


def fuzzy(query: str, text: str) -> bool:
    """Coincidencia por subsecuencia, sin distinguir mayúsculas."""
    position = 0
    text = text.lower()
    for char in query.lower():
        position = text.find(char, position)
        if position == -1:
            return False
        position += 1
    return True


class OpenFileScreen(Dialog):
    """Buscador rápido de archivos del proyecto."""

    def __init__(self, root: Path) -> None:
        super().__init__()
        self._root = root
        self._files = list_project_files(root)
        self._shown: list[Path] = []

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label("Abrir archivo", classes="dialog-title")
            yield Input(placeholder="Escribe para filtrar, o una ruta para abrir o crear…")
            yield OptionList()
            yield Static(f"[dim]{self._root}[/]", classes="dialog-hint")

    def on_mount(self) -> None:
        self._filter("")

    def _filter(self, query: str) -> None:
        self._shown = [path for path in self._files if fuzzy(query, str(path))][:200]
        options = self.query_one(OptionList)
        options.clear_options()
        rows = []
        for path in self._shown:
            text = Text()
            if path.parent != Path("."):
                text.append(f"{path.parent}/", style="dim")
            text.append(path.name, style="bold")
            rows.append(Option(text))
        options.add_options(rows)
        if rows:
            options.highlighted = 0

    @on(Input.Changed)
    def _changed(self, event: Input.Changed) -> None:
        self._filter(event.value.strip())

    @on(Input.Submitted)
    def _submitted(self, event: Input.Submitted) -> None:
        options = self.query_one(OptionList)
        if self._shown and options.highlighted is not None:
            self.dismiss(self._root / self._shown[options.highlighted])
        elif event.value.strip():
            self.dismiss((self._root / Path(event.value.strip()).expanduser()).resolve())

    @on(OptionList.OptionSelected)
    def _selected(self, event: OptionList.OptionSelected) -> None:
        self.dismiss(self._root / self._shown[event.option_index])


class NewFileScreen(Dialog):
    """Elige una plantilla y un nombre; devuelve ``(plantilla, nombre)``."""

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label("Nuevo documento", classes="dialog-title")
            options = []
            for template in TEMPLATES:
                text = Text()
                text.append(f"{template.title:<14}", style="bold")
                text.append(template.description, style="dim")
                options.append(Option(text, id=template.key))
            yield OptionList(*options)
            yield Input(value=TEMPLATES[0].filename, placeholder="nombre.tex")
            yield Static("[dim]↑↓ plantilla · Enter crear · Esc cancelar[/]", classes="dialog-hint")

    def on_mount(self) -> None:
        self.query_one(OptionList).highlighted = 0
        self.query_one(Input).focus()

    def _template(self) -> Template:
        index = self.query_one(OptionList).highlighted or 0
        return TEMPLATES[index]

    @on(OptionList.OptionHighlighted)
    def _highlighted(self, event: OptionList.OptionHighlighted) -> None:
        field = self.query_one(Input)
        defaults = {template.filename for template in TEMPLATES}
        if field.value in defaults or not field.value:
            field.value = TEMPLATES[event.option_index].filename

    @on(OptionList.OptionSelected)
    def _selected(self) -> None:
        self.query_one(Input).focus()

    @on(Input.Submitted)
    def _submitted(self, event: Input.Submitted) -> None:
        name = event.value.strip()
        if not name:
            return
        if "." not in Path(name).name:
            name += ".tex"
        self.dismiss((self._template(), name))


class SymbolScreen(Dialog):
    """Buscador de símbolos matemáticos; devuelve el comando elegido."""

    def __init__(self) -> None:
        super().__init__()
        self._shown: list[Symbol] = []

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog"):
            yield Label("Símbolos", classes="dialog-title")
            yield Input(placeholder="Busca por nombre o comando: integral, flecha, alpha…")
            yield OptionList()
            yield Static("[dim]↑↓ elegir · Enter insertar · Esc cancelar[/]", classes="dialog-hint")

    def on_mount(self) -> None:
        self._filter("")

    def _filter(self, query: str) -> None:
        query = query.lower()
        self._shown = [
            symbol
            for symbol in SYMBOLS
            if not query
            or query in symbol.name.lower()
            or query in symbol.command.lower()
            or query in symbol.group.lower()
            or query == symbol.glyph
        ]
        options = self.query_one(OptionList)
        options.clear_options()
        rows = []
        for symbol in self._shown:
            text = Text(no_wrap=True)
            text.append(f" {symbol.glyph}  ", style="bold")
            text.append(f"{symbol.command:<18}")
            text.append(f"{symbol.name}", style="dim")
            text.append(f"  · {symbol.group}", style="dim italic")
            rows.append(Option(text))
        options.add_options(rows)
        if rows:
            options.highlighted = 0

    @on(Input.Changed)
    def _changed(self, event: Input.Changed) -> None:
        self._filter(event.value.strip())

    def _choose(self, index: int | None) -> None:
        if index is not None and 0 <= index < len(self._shown):
            self.dismiss(self._shown[index].command)

    @on(Input.Submitted)
    def _submitted(self) -> None:
        self._choose(self.query_one(OptionList).highlighted)

    @on(OptionList.OptionSelected)
    def _selected(self, event: OptionList.OptionSelected) -> None:
        self._choose(event.option_index)


HELP_SECTIONS: tuple[tuple[str, tuple[tuple[str, str], ...]], ...] = (
    (
        "Archivo",
        (
            ("Ctrl+N", "Nuevo documento desde plantilla"),
            ("Ctrl+O", "Abrir archivo del proyecto"),
            ("Ctrl+S", "Guardar"),
            ("Ctrl+W", "Cerrar pestaña"),
            ("Ctrl+Q", "Salir"),
        ),
    ),
    (
        "Compilar y ver",
        (
            ("F5 · Ctrl+R", "Compilar a PDF"),
            ("F6", "Abrir el PDF en el visor del sistema"),
            ("F2", "Mostrar u ocultar la barra lateral"),
            ("F3", "Mostrar u ocultar la vista previa"),
            ("F4", "Mostrar u ocultar problemas y registro"),
        ),
    ),
    (
        "Edición",
        (
            ("Ctrl+F", "Buscar y reemplazar"),
            ("Ctrl+G", "Ir a línea"),
            ("Ctrl+T", "Insertar símbolo"),
            ("Ctrl+B", "Negrita  \\textbf{…}"),
            ("Ctrl+L", "Cursiva  \\textit{…}"),
            ("Ctrl+/", "Comentar o descomentar líneas"),
            ("Tab · Shift+Tab", "Sangrar o quitar sangría"),
            ("Ctrl+Z · Ctrl+Y", "Deshacer · rehacer"),
            ("Ctrl+P", "Paleta de comandos"),
        ),
    ),
    (
        "Asistencia",
        (
            ("\\", "Autocompleta comandos; Tab o Enter aceptan"),
            ("\\begin{", "Lista de entornos; inserta también el \\end"),
            ("\\ref{ · \\cite{", "Sugiere etiquetas y claves de los .bib"),
            ("Enter", "Tras \\begin{…} cierra el entorno; en listas añade \\item"),
            ("{ [ ( $", "Se cierran solos y envuelven la selección"),
        ),
    ),
    (
        "Fondo (desde la paleta, Ctrl+P)",
        (
            ("Fondo: elegir imagen…", "Foto detrás de toda la ventana"),
            ("Fondo: estilo…", "Tramado con puntos o liso"),
            ("Fondo: usar colores…", "Paleta sacada de la foto o del tema"),
            ("Fondo: más / menos", "Intensidad de la foto"),
        ),
    ),
    (
        "Vista previa (con el foco en ella)",
        (
            ("j · k", "Página siguiente · anterior"),
            ("+ · −", "Acercar · alejar"),
            ("i", "Invertir colores"),
        ),
    ),
)


class HelpScreen(Dialog):
    """Chuleta de atajos."""

    BINDINGS = [Binding("escape,f1,q", "dismiss(None)", "Cerrar", show=False)]

    def compose(self) -> ComposeResult:
        with Vertical(classes="dialog dialog-wide"):
            yield Label("✿ MiyuLaTeX · atajos", classes="dialog-title")
            with VerticalScroll():
                for title, rows in HELP_SECTIONS:
                    table = Table.grid(padding=(0, 2), expand=True)
                    table.add_column(width=22, no_wrap=True)
                    table.add_column(ratio=1)
                    for keys, description in rows:
                        table.add_row(Text(keys, style="bold"), description)
                    yield Static(title, classes="help-section")
                    yield Static(table, classes="help-table")
            yield Static("[dim]Esc para cerrar[/]", classes="dialog-hint")
