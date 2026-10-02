"""La aplicación: ventana principal y coordinación de paneles."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path
from typing import Iterable

from rich.text import Text
from textual import on, work
from textual._animator import Animator
from textual.app import App, ComposeResult, SystemCommand
from textual.binding import Binding
from textual.containers import Container, Horizontal, Vertical
from textual.screen import Screen
from textual.widgets import (
    Button,
    DirectoryTree,
    Footer,
    Input,
    OptionList,
    RichLog,
    Static,
    TabbedContent,
    TabPane,
    TextArea,
    Tree,
)
from textual.widgets.option_list import Option
from textual.widgets.text_area import Selection

from . import FPS
from .backdrop import (
    MAX_INTENSITY,
    MIN_INTENSITY,
    PHOTO_THEME,
    STYLES,
    Backdrop,
    clean_path,
    import_background,
    install,
    photo_theme,
    saved_backgrounds,
)
from .compiler import (
    AUX_SUFFIXES,
    CompileResult,
    Problem,
    clean_auxiliary,
    compile_document,
    detect_engines,
    find_root,
)
from .config import Config
from .editor import CompletionPopup, LatexEditor
from .outline import OutlineItem, count_words, parse_outline
from .preview import CELL_PIXELS, KITTY_GRAPHICS, PdfPreview
from .screens import (
    TEXT_SUFFIXES,
    ConfirmScreen,
    HelpScreen,
    InputScreen,
    NewFileScreen,
    OpenFileScreen,
    SymbolScreen,
)
from .theme import APP_THEMES, DEFAULT_THEME, build_editor_theme

SPINNER = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
HIDDEN_SUFFIXES = set(AUX_SUFFIXES) | {".gz", ".pyc"}
HIDDEN_NAMES = {"__pycache__", "node_modules", ".git", ".venv", ".DS_Store"}
MAX_FILE_SIZE = 5_000_000

_SHORTCUTS = (
    ("Ctrl+N", "nuevo documento desde plantilla"),
    ("Ctrl+O", "abrir un archivo del proyecto"),
    ("F5", "compilar a PDF"),
    ("Ctrl+P", "paleta de comandos y fondos"),
    ("F1", "todos los atajos"),
)

# Las líneas de atajos tienen el mismo ancho para que el centrado las deje alineadas.
WELCOME = (
    "[b $primary]✿  M i y u L a T e X[/]\n"
    "[dim]un editor de LaTeX para la terminal[/]\n\n"
    + "\n".join(f"[b $secondary]{keys:<8}[/]{text:<32}" for keys, text in _SHORTCUTS)
    + "\n\n[dim]Escribe[/] [b $accent]\\frac[/] [dim]o[/] [b $accent]\\begin{[/] [dim]y elige una sugerencia.[/]"
)

ICONS = {"error": "✖", "warning": "▲", "info": "●"}


class ProjectTree(DirectoryTree):
    """Árbol del proyecto sin archivos auxiliares ni ocultos."""

    ICON_NODE = "▸ "
    ICON_NODE_EXPANDED = "▾ "
    ICON_FILE = "· "

    def filter_paths(self, paths: Iterable[Path]) -> Iterable[Path]:
        return [
            path
            for path in paths
            if not path.name.startswith(".")
            and path.name not in HIDDEN_NAMES
            and not any(path.name.endswith(suffix) for suffix in HIDDEN_SUFFIXES)
        ]


class FindBar(Vertical):
    """Barra de buscar y reemplazar bajo el editor."""

    def compose(self) -> ComposeResult:
        with Horizontal():
            yield Static("Buscar", classes="find-label")
            yield Input(id="find-input")
            yield Static("", id="find-count")
            yield Button("↑", id="find-prev", compact=True, tooltip="Anterior")
            yield Button("↓", id="find-next", compact=True, tooltip="Siguiente")
            yield Button("✕", id="find-close", compact=True, tooltip="Cerrar (Esc)")
        with Horizontal():
            yield Static("Reemplazar", classes="find-label")
            yield Input(id="replace-input")
            yield Button("Uno", id="replace-one", compact=True, tooltip="Reemplazar esta coincidencia")
            yield Button("Todos", id="replace-all", compact=True, tooltip="Reemplazar todas")


class MiyuApp(App[None]):
    """Editor de LaTeX para la terminal."""

    TITLE = "MiyuLaTeX"
    CSS_PATH = "app.tcss"
    ENABLE_COMMAND_PALETTE = True

    BINDINGS = [
        Binding("ctrl+s", "save", "Guardar", priority=True),
        Binding("f5,ctrl+r", "compile", "Compilar", priority=True, key_display="F5"),
        Binding("ctrl+o", "open", "Abrir", priority=True),
        Binding("ctrl+n", "new", "Nuevo", priority=True),
        Binding("ctrl+f", "find", "Buscar", priority=True),
        Binding("ctrl+t", "symbols", "Símbolos", priority=True),
        Binding("ctrl+g", "goto", "Ir a línea", show=False, priority=True),
        Binding("ctrl+b", "bold", "Negrita", show=False, priority=True),
        Binding("ctrl+l", "italic", "Cursiva", show=False, priority=True),
        Binding("ctrl+slash,ctrl+underscore", "comment", "Comentar", show=False, priority=True),
        Binding("ctrl+w", "close_tab", "Cerrar pestaña", show=False, priority=True),
        Binding("f1", "help", "Ayuda", priority=True),
        Binding("f2", "toggle_sidebar", "Barra lateral", show=False, priority=True),
        Binding("f3", "toggle_preview", "Vista previa", show=False, priority=True),
        Binding("f4", "toggle_panel", "Problemas", show=False, priority=True),
        Binding("f6", "open_pdf", "Abrir PDF", show=False, priority=True),
        Binding("ctrl+q", "request_quit", "Salir", show=False, priority=True),
    ]

    # Acciones que solo tienen sentido en la ventana principal.
    _MAIN_ONLY = {
        "save", "compile", "open", "new", "find", "symbols", "goto", "bold", "italic",
        "comment", "close_tab", "help", "toggle_sidebar", "toggle_preview",
        "toggle_panel", "open_pdf", "request_quit",
    }

    def __init__(self, target: Path | None = None) -> None:
        super().__init__()
        # El animador de Textual va a 60 fps por defecto, sea cual sea el tope de refresco.
        self._animator = Animator(self, frames_per_second=FPS)
        self._animate = self._animator.bind(self)
        self.config = Config.load()
        install()
        self.backdrop = Backdrop()
        self.backdrop.configure(self.config.background_style, self.config.background_intensity)
        if self.config.background:
            self.backdrop.load(Path(self.config.background))
        self.backdrop.pixels = self.config.background_pixels and self._can_use_pixels()
        self._pixels_placed = False
        self._pixels_timer = None
        self.initial_file: Path | None = None
        target = (target or Path.cwd()).expanduser().resolve()
        if target.is_dir():
            self.project = target
        else:
            self.project = target.parent
            self.initial_file = target
        self.engines = detect_engines()
        self.last_result: CompileResult | None = None
        self.problems: list[Problem] = []
        self.compiling = False
        self._spin = 0
        self._buffer_count = 0
        self._outline_items: list[OutlineItem] | None = None
        self._refresh_timer = None
        self._matches: list[tuple[int, int, int]] = []
        self._words = 0
        self._last_message = ""

    # ------------------------------------------------------------ diseño

    def compose(self) -> ComposeResult:
        yield Static(id="header")
        with Horizontal(id="body"):
            with Vertical(id="sidebar"):
                with TabbedContent(id="side-tabs"):
                    with TabPane("Archivos", id="side-files"):
                        yield ProjectTree(self.project, id="files")
                    with TabPane("Esquema", id="side-outline"):
                        yield Tree("Esquema", id="outline")
            with Vertical(id="center"):
                with Container(id="welcome"):
                    yield Static(WELCOME, id="welcome-text")
                yield TabbedContent(id="editors")
                yield FindBar(id="findbar")
                with Vertical(id="panel"):
                    with TabbedContent(id="panel-tabs"):
                        with TabPane("Problemas", id="panel-problems"):
                            yield OptionList(id="problems")
                        with TabPane("Registro", id="panel-log"):
                            yield RichLog(id="log", wrap=True, markup=False, highlight=False)
            yield PdfPreview(id="preview")
        with Horizontal(id="status"):
            yield Static(id="status-left")
            yield Static(id="status-right")
        yield Footer()
        yield CompletionPopup()

    async def on_mount(self) -> None:
        for theme in APP_THEMES:
            self.register_theme(theme)
        if self.config.theme not in self.available_themes or self.config.theme == PHOTO_THEME:
            self.config.theme = DEFAULT_THEME
        self.theme = self.config.theme
        self.theme_changed_signal.subscribe(self, self._theme_changed)
        self._sync_backdrop_colors()
        self._apply_photo_theme()

        self.query_one("#sidebar").border_title = self.project.name or str(self.project)
        self.query_one("#preview").border_title = "PDF"
        self.query_one("#panel").border_title = "Salida"
        self.query_one("#files", ProjectTree).show_root = False
        outline = self.query_one("#outline", Tree)
        outline.show_root = False
        outline.guide_depth = 2
        self.query_one("#editors").display = False
        self.query_one("#findbar").display = False
        self.query_one("#panel").display = False
        self.query_one("#completion").display = False
        self.query_one("#sidebar").display = self.config.show_sidebar
        preview = self.query_one(PdfPreview)
        preview.display = self.config.show_preview and self.size.width >= 100
        preview.set_reactive(PdfPreview.invert, self.config.invert_preview)

        self.set_interval(0.1, self._tick)
        self._update_chrome()

        if self.initial_file is not None:
            await self.open_path(self.initial_file, create=True)
        else:
            documents = sorted(self.project.glob("*.tex"))
            main = next((p for p in documents if p.name == "main.tex"), None)
            if main or len(documents) == 1:
                await self.open_path(main or documents[0])

    def check_action(self, action: str, parameters: tuple[object, ...]) -> bool | None:
        if action in self._MAIN_ONLY and len(self.screen_stack) > 1:
            return False
        return True

    # ----------------------------------------------------------- acceso

    @property
    def editors(self) -> TabbedContent:
        return self.query_one("#editors", TabbedContent)

    @property
    def editor(self) -> LatexEditor | None:
        """El editor de la pestaña activa."""
        pane = self.editors.active_pane
        if pane is None:
            return None
        found = pane.query(LatexEditor)
        return found.first() if found else None

    def all_editors(self) -> list[LatexEditor]:
        return list(self.editors.query(LatexEditor))

    def _pane_of(self, editor: LatexEditor) -> TabPane:
        return next(node for node in editor.ancestors if isinstance(node, TabPane))

    # ---------------------------------------------------------- cabecera

    def _theme_changed(self, _theme=None) -> None:
        theme = self._editor_theme()
        for editor in self.all_editors():
            editor.apply_theme(theme)
        self._sync_backdrop_colors()
        if self.theme != PHOTO_THEME and self.theme != self.config.theme:
            # Elegir un tema a mano es dejar de usar los colores de la foto.
            self.config.theme = self.theme
            if self.backdrop.active:
                self.config.background_palette = False
            self.config.save()
        self._update_chrome()
        self._repaint()

    def _sync_backdrop_colors(self) -> None:
        """Le dice al fondo qué colores de celda dejan ver la foto."""
        theme = self._editor_theme()
        base = theme.base_style.bgcolor.triplet
        cursor_line = theme.cursor_line_style.bgcolor.triplet
        self.backdrop.set_colors(tuple(base), (tuple(cursor_line),))

    def _apply_photo_theme(self) -> None:
        """Usa los colores de la foto como tema, o vuelve al tema elegido."""
        backdrop = self.backdrop
        if backdrop.active and backdrop.tone and self.config.background_palette:
            dark = self.available_themes[self.config.theme].dark
            self.register_theme(photo_theme(backdrop.tone, dark))
            if self.theme == PHOTO_THEME:
                self.theme = self.config.theme  # obliga a recargar el tema con la paleta nueva
            self.theme = PHOTO_THEME
        elif self.theme == PHOTO_THEME:
            self.theme = self.config.theme

    # El modo de píxeles necesita el protocolo gráfico de Kitty y saber cuánto mide una celda.
    @staticmethod
    def _can_use_pixels() -> bool:
        return KITTY_GRAPHICS and min(CELL_PIXELS) > 0

    def _sync_pixels(self) -> None:
        """Coloca (o retira) la foto en píxeles reales debajo de la pantalla."""
        self._pixels_timer = None
        backdrop = self.backdrop
        if backdrop.active and backdrop.pixels:
            size = self.size
            self.run_worker(
                lambda: self._place_pixels(size.width, size.height),
                thread=True,
                exclusive=True,
                group="backdrop-pixels",
            )
        elif self._pixels_placed:
            self._pixels_placed = False
            self._write_raw(Backdrop.kitty_clear())

    def _place_pixels(self, columns: int, rows: int) -> None:
        commands = self.backdrop.kitty_place(columns, rows, *CELL_PIXELS)
        self.call_from_thread(self._write_raw, Backdrop.kitty_clear() + commands)
        self._pixels_placed = True

    def _write_raw(self, data: str) -> None:
        driver = self._driver
        if driver is None:
            return
        try:
            driver.write(data)
            driver.flush()
        except Exception:
            pass

    def on_resize(self) -> None:
        if self.backdrop.pixels:
            if self._pixels_timer is not None:
                self._pixels_timer.stop()
            self._pixels_timer = self.set_timer(0.25, self._sync_pixels)

    def on_unmount(self) -> None:
        if self._pixels_placed:
            self._write_raw(Backdrop.kitty_clear())

    def action_background_pixels(self) -> None:
        self.backdrop.pixels = not self.backdrop.pixels
        self.backdrop._reset()
        self.config.background_pixels = self.backdrop.pixels
        self._backdrop_changed("Fondo en píxeles reales" if self.backdrop.pixels else "Fondo por celdas")

    def _repaint(self) -> None:
        self._sync_pixels()
        for screen in self.screen_stack:
            for widget in screen.walk_children(with_self=True):
                widget.refresh()

    def _editor_theme(self):
        return build_editor_theme(self.get_css_variables(), self.current_theme.dark)

    def _engine(self) -> tuple[str, str] | None:
        """Motor elegido como ``(nombre, ejecutable)``."""
        if not self.engines:
            self.engines = detect_engines()
        if self.config.engine in self.engines:
            return self.config.engine, self.engines[self.config.engine]
        for name, executable in self.engines.items():
            return name, executable
        return None

    def _update_chrome(self) -> None:
        """Redibuja la cabecera y la barra de estado."""
        variables = self.get_css_variables()
        primary = variables.get("primary", "#ff8fb7")
        secondary = variables.get("secondary", primary)
        accent = variables.get("accent", primary)
        muted = variables.get("text-muted", "grey")
        warning = variables.get("warning", "yellow")
        error = variables.get("error", "red")
        success = variables.get("success", "green")

        editor = self.editor
        header = Text(no_wrap=True, overflow="ellipsis")
        header.append(" ✿ MiyuLaTeX ", style=f"bold {primary}")
        header.append("│ ", style="dim")
        if editor is None:
            header.append(str(self.project).replace(str(Path.home()), "~"), style="dim")
        else:
            if editor.path is not None:
                try:
                    shown = editor.path.relative_to(self.project)
                except ValueError:
                    shown = editor.path
                parent = str(shown.parent)
                if parent != ".":
                    header.append(parent + "/", style="dim")
            header.append(editor.title, style="bold")
            if editor.dirty:
                header.append(" ●", style=f"{warning}")
        self.query_one("#header", Static).update(header)

        left = Text(no_wrap=True, overflow="ellipsis")
        engine = self._engine()
        if self.compiling:
            left.append(f" {SPINNER[self._spin % len(SPINNER)]} Compilando…", style=f"bold {accent}")
        elif self.last_result is not None:
            result = self.last_result
            if result.ok and not result.errors:
                left.append(f" ✔ Compilado en {result.duration:.1f} s", style=f"bold {success}")
            else:
                left.append(f" ✖ La compilación falló", style=f"bold {error}")
            if result.errors:
                left.append(f"  ✖ {result.errors}", style=error)
            if result.warnings:
                left.append(f"  ▲ {result.warnings}", style=warning)
        elif engine is None:
            left.append(" ▲ Sin motor LaTeX · brew install tectonic", style=warning)
        else:
            left.append(" Listo", style="dim")
        self.query_one("#status-left", Static).update(left)

        right = Text(no_wrap=True, justify="right")
        if editor is not None:
            row, column = editor.cursor_location
            right.append(f"Ln {row + 1}, Col {column + 1}", style="bold")
            right.append("  │  ", style="dim")
            right.append(f"{self._words:,} palabras".replace(",", " "))
            right.append("  │  ", style="dim")
        right.append(engine[0] if engine else "sin motor", style=secondary if engine else warning)
        if self.config.autocompile:
            right.append(" ⟳", style=accent)
        right.append(" ")
        self.query_one("#status-right", Static).update(right)

    def _tick(self) -> None:
        if self.compiling:
            self._spin += 1
            self._update_chrome()

    # ----------------------------------------------------------- buffers

    async def open_path(self, path: Path, line: int | None = None, create: bool = False) -> LatexEditor | None:
        """Abre ``path`` en una pestaña (o activa la que ya lo tiene)."""
        path = path.expanduser().resolve()
        if path.suffix.lower() == ".pdf":
            self._show_preview(path)
            return None

        for editor in self.all_editors():
            if editor.path == path:
                self.editors.active = self._pane_of(editor).id
                if line is not None:
                    editor.goto_line(line)
                else:
                    editor.focus()
                return editor

        if path.exists():
            if path.is_dir():
                return None
            if path.suffix.lower() not in TEXT_SUFFIXES and path.stat().st_size > 0 and not _looks_textual(path):
                self.notify(f"{path.name} no parece un archivo de texto.", severity="warning")
                return None
            if path.stat().st_size > MAX_FILE_SIZE:
                self.notify(f"{path.name} es demasiado grande para editarlo aquí.", severity="warning")
                return None
            try:
                text = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                text = path.read_text(encoding="latin-1")
                self.notify(f"{path.name} no es UTF-8: se leyó como Latin-1 y se guardará como UTF-8.", severity="warning")
            except OSError as error:
                self.notify(f"No pude abrir {path.name}: {error}", severity="error")
                return None
        elif create:
            text = ""
        else:
            self.notify(f"No existe {path}", severity="error")
            return None

        editor = await self._add_editor(text.replace("\r\n", "\n"), path)
        if not path.exists():
            editor.saved_text = "\0"  # un archivo nuevo siempre está pendiente de guardar
            self._refresh_tab(editor)
        if line is not None:
            editor.goto_line(line)
        return editor

    async def _add_editor(self, text: str, path: Path | None) -> LatexEditor:
        self._buffer_count += 1
        editor = LatexEditor(text, path=path)
        editor.completion = self.query_one(CompletionPopup)
        editor.soft_wrap = self.config.soft_wrap
        pane = TabPane(editor.title, editor, id=f"buffer-{self._buffer_count}")
        tabs = self.editors
        self.query_one("#welcome").display = False
        tabs.display = True
        await tabs.add_pane(pane)
        editor.apply_theme(self._editor_theme())
        tabs.active = pane.id
        editor.focus()
        self._document_changed(editor, immediate=True)
        return editor

    def _refresh_tab(self, editor: LatexEditor) -> None:
        dirty = editor.dirty
        shown = (editor.title, dirty)
        if getattr(editor, "_shown_tab", None) == shown:
            return
        editor._shown_tab = shown
        label = Text(editor.title)
        if dirty:
            label.append(" ●")
        try:
            self.editors.get_tab(self._pane_of(editor)).label = label
        except Exception:
            pass
        self._update_chrome()

    def _write(self, editor: LatexEditor) -> bool:
        assert editor.path is not None
        text = editor.text
        if text and not text.endswith("\n"):
            text += "\n"
        try:
            editor.path.parent.mkdir(parents=True, exist_ok=True)
            editor.path.write_text(text, encoding="utf-8")
        except OSError as error:
            self.notify(f"No pude guardar {editor.title}: {error}", severity="error")
            return False
        editor.saved_text = editor.text
        self._refresh_tab(editor)
        return True

    async def _save(self, editor: LatexEditor) -> bool:
        """Guarda el editor, pidiendo nombre si aún no tiene."""
        if editor.path is None:
            name = await self.push_screen_wait(
                InputScreen("Guardar como", "nombre.tex", editor.title, f"Se guardará en {self.project}")
            )
            if not name:
                return False
            if "." not in Path(name).name:
                name += ".tex"
            editor.path = (self.project / Path(name).expanduser()).resolve()
        created = not editor.path.exists()
        if not self._write(editor):
            return False
        if created:
            self.query_one("#files", ProjectTree).reload()
        return True

    @work
    async def action_save(self) -> None:
        editor = self.editor
        if editor is None:
            return
        if await self._save(editor):
            self._last_message = f"Guardado {editor.title}"
            if self.config.autocompile and editor.path and editor.path.suffix.lower() == ".tex" and self._engine():
                self._start_compile(editor)
            else:
                self.notify(f"Guardado [b]{editor.title}[/]", timeout=2)

    @work
    async def action_close_tab(self) -> None:
        editor = self.editor
        if editor is None:
            return
        if editor.dirty:
            answer = await self.push_screen_wait(
                ConfirmScreen(
                    "Cambios sin guardar",
                    f"[b]{editor.title}[/] tiene cambios sin guardar.",
                    [("save", "Guardar", "primary"), ("discard", "Descartar", "error"), ("cancel", "Cancelar", "default")],
                )
            )
            if answer in (None, "cancel"):
                return
            if answer == "save" and not await self._save(editor):
                return
        editor.close_completion()
        tabs = self.editors
        await tabs.remove_pane(self._pane_of(editor).id)
        if not self.all_editors():
            tabs.display = False
            self.query_one("#welcome").display = True
            self.query_one("#findbar").display = False
            self._outline_items = None
            self.query_one("#outline", Tree).clear()
        self._update_chrome()

    @work
    async def action_request_quit(self) -> None:
        dirty = [editor for editor in self.all_editors() if editor.dirty]
        if dirty:
            names = ", ".join(editor.title for editor in dirty)
            answer = await self.push_screen_wait(
                ConfirmScreen(
                    "Salir de MiyuLaTeX",
                    f"Hay cambios sin guardar en: [b]{names}[/]",
                    [("save", "Guardar todo", "primary"), ("discard", "Salir sin guardar", "error"), ("cancel", "Cancelar", "default")],
                )
            )
            if answer in (None, "cancel"):
                return
            if answer == "save":
                for editor in dirty:
                    if not await self._save(editor):
                        return
        self.config.save()
        self.exit()

    @work
    async def action_open(self) -> None:
        path = await self.push_screen_wait(OpenFileScreen(self.project))
        if path is not None:
            await self.open_path(path, create=True)

    @work
    async def action_new(self) -> None:
        chosen = await self.push_screen_wait(NewFileScreen())
        if chosen is None:
            return
        template, name = chosen
        path = (self.project / Path(name).expanduser()).resolve()
        if path.exists():
            answer = await self.push_screen_wait(
                ConfirmScreen(
                    "El archivo ya existe",
                    f"[b]{path.name}[/] ya existe en el proyecto.",
                    [("open", "Abrir el existente", "primary"), ("cancel", "Cancelar", "default")],
                )
            )
            if answer == "open":
                await self.open_path(path)
            return
        editor = await self._add_editor(template.body, path)
        if self._write(editor):
            self.query_one("#files", ProjectTree).reload()
            self.notify(f"Creado [b]{path.name}[/]", timeout=2)

    # ----------------------------------------------------------- edición

    @on(TextArea.Changed)
    def _text_changed(self, event: TextArea.Changed) -> None:
        editor = event.text_area
        if isinstance(editor, LatexEditor):
            self._refresh_tab(editor)
            self._document_changed(editor)

    @on(LatexEditor.CursorMoved)
    def _cursor_moved(self, event: LatexEditor.CursorMoved) -> None:
        if event.editor is self.editor:
            self._update_chrome()

    @on(LatexEditor.Escaped)
    def _editor_escaped(self) -> None:
        if self.query_one("#findbar").display:
            self._close_find()

    @on(TabbedContent.TabActivated, "#editors")
    def _tab_activated(self, event: TabbedContent.TabActivated) -> None:
        self.query_one(CompletionPopup).close()
        editor = self.editor
        if editor is not None:
            self._document_changed(editor, immediate=True)
            if self.query_one("#findbar").display:
                self._search()
            editor.focus()

    def _document_changed(self, editor: LatexEditor, immediate: bool = False) -> None:
        """Programa la actualización del esquema y del recuento de palabras."""
        if self._refresh_timer is not None:
            self._refresh_timer.stop()
        if immediate:
            self._refresh_document()
        else:
            self._refresh_timer = self.set_timer(0.3, self._refresh_document)

    def _refresh_document(self) -> None:
        self._refresh_timer = None
        editor = self.editor
        if editor is None:
            return
        text = editor.text
        self._words = count_words(text)
        items = parse_outline(text)
        if items != self._outline_items:
            self._outline_items = items
            self._build_outline(items)
        if self.query_one("#findbar").display and editor.search_matches:
            self._search(move=False)
        self._update_chrome()

    def _build_outline(self, items: list[OutlineItem]) -> None:
        tree = self.query_one("#outline", Tree)
        tree.clear()
        if not items:
            tree.root.add_leaf(Text("Sin secciones todavía", style="dim italic"))
            return
        marks = {0: "◆", 1: "◆", 2: "§", 3: "›", 4: "·", 5: "·"}
        stack: list[tuple[int, object]] = [(-1, tree.root)]
        for index, item in enumerate(items):
            while stack[-1][0] >= item.level:
                stack.pop()
            label = Text()
            label.append(f"{marks.get(item.level, '·')} ", style="dim")
            label.append(item.title, style="bold" if item.level <= 2 else "")
            has_children = index + 1 < len(items) and items[index + 1].level > item.level
            parent = stack[-1][1]
            if has_children:
                node = parent.add(label, data=item.line, expand=True)
            else:
                node = parent.add_leaf(label, data=item.line)
            stack.append((item.level, node))

    @on(Tree.NodeSelected, "#outline")
    def _outline_selected(self, event: Tree.NodeSelected) -> None:
        editor = self.editor
        if editor is not None and isinstance(event.node.data, int):
            editor.goto_line(event.node.data)

    @on(DirectoryTree.FileSelected, "#files")
    async def _file_selected(self, event: DirectoryTree.FileSelected) -> None:
        await self.open_path(event.path)

    def action_bold(self) -> None:
        if self.editor is not None:
            self.editor.wrap_selection("\\textbf{", "}")

    def action_italic(self) -> None:
        if self.editor is not None:
            self.editor.wrap_selection("\\textit{", "}")

    def action_comment(self) -> None:
        if self.editor is not None:
            self.editor.toggle_comment()

    @work
    async def action_goto(self) -> None:
        editor = self.editor
        if editor is None:
            return
        total = editor.document.line_count
        value = await self.push_screen_wait(
            InputScreen("Ir a línea", f"1 – {total}", hint="También puedes escribir línea:columna")
        )
        if not value:
            return
        line, _, column = value.partition(":")
        try:
            editor.goto_line(int(line) - 1, int(column) - 1 if column.strip() else 0)
        except ValueError:
            self.notify("Escribe un número de línea.", severity="warning")

    @work
    async def action_symbols(self) -> None:
        editor = self.editor
        if editor is None:
            return
        command = await self.push_screen_wait(SymbolScreen())
        if command:
            editor.insert_snippet(command.replace("{}", "{$0}", 1))

    def action_help(self) -> None:
        self.push_screen(HelpScreen())

    # ------------------------------------------------------------ buscar

    def action_find(self) -> None:
        editor = self.editor
        if editor is None:
            return
        bar = self.query_one("#findbar")
        bar.display = True
        field = self.query_one("#find-input", Input)
        selected = editor.selected_text
        if selected and "\n" not in selected:
            field.value = selected
        field.focus()
        field.action_select_all()
        self._search(move=False)

    def _close_find(self) -> None:
        self.query_one("#findbar").display = False
        for editor in self.all_editors():
            if editor.search_matches:
                editor.set_search("")
        if self.editor is not None:
            self.editor.focus()

    def _search(self, move: bool = True) -> None:
        editor = self.editor
        if editor is None:
            return
        query = self.query_one("#find-input", Input).value
        self._matches = editor.set_search(query)
        if move and self._matches:
            self._jump(1, include_current=True)
        else:
            self._update_find_count()

    def _current_match(self) -> int | None:
        editor = self.editor
        if editor is None:
            return None
        start, end = sorted(editor.selection)
        for index, (row, first, last) in enumerate(self._matches):
            if start == (row, first) and end == (row, last):
                return index
        return None

    def _update_find_count(self) -> None:
        label = self.query_one("#find-count", Static)
        query = self.query_one("#find-input", Input).value
        if not query:
            label.update("")
        elif not self._matches:
            label.update("[$error]sin resultados[/]")
        else:
            current = self._current_match()
            position = f"{current + 1}" if current is not None else "–"
            label.update(f"{position} de {len(self._matches)}")

    def _jump(self, direction: int, include_current: bool = False) -> None:
        """Selecciona la coincidencia siguiente (1) o anterior (-1), dando la vuelta al final."""
        editor = self.editor
        if editor is None or not self._matches:
            self._update_find_count()
            return
        start = min(editor.selection)
        positions = [(row, first) for row, first, _ in self._matches]
        if direction > 0:
            index = next(
                (i for i, pos in enumerate(positions) if pos > start or (include_current and pos == start)),
                0,
            )
        else:
            index = next(
                (i for i in range(len(positions) - 1, -1, -1) if positions[i] < start),
                len(positions) - 1,
            )
        row, first, last = self._matches[index]
        editor.selection = Selection((row, first), (row, last))
        editor.scroll_cursor_visible(center=True)
        self._update_find_count()

    def _replace_current(self) -> None:
        editor = self.editor
        if editor is None:
            return
        if self._current_match() is None:
            self._jump(1, include_current=True)
            return
        start, end = sorted(editor.selection)
        editor.replace(self.query_one("#replace-input", Input).value, start, end)
        self._search(move=False)
        self._jump(1, include_current=True)

    def _replace_all(self) -> None:
        editor = self.editor
        if editor is None or not self._matches:
            return
        replacement = self.query_one("#replace-input", Input).value
        count = len(self._matches)
        lines = list(editor.document.lines)
        for row, first, last in reversed(self._matches):
            lines[row] = lines[row][:first] + replacement + lines[row][last:]
        cursor = editor.cursor_location
        last_row = editor.document.line_count - 1
        editor.replace("\n".join(lines), (0, 0), (last_row, len(editor.document.get_line(last_row))))
        editor.move_cursor(cursor)
        self._search(move=False)
        self.notify(f"{count} reemplazos", timeout=2)

    @on(Input.Changed, "#find-input")
    def _find_changed(self) -> None:
        self._search()

    @on(Input.Submitted, "#find-input")
    def _find_submitted(self) -> None:
        self._jump(1)

    @on(Input.Submitted, "#replace-input")
    def _replace_submitted(self) -> None:
        self._replace_current()

    @on(Button.Pressed, "#findbar Button")
    def _find_button(self, event: Button.Pressed) -> None:
        event.stop()
        actions = {
            "find-prev": lambda: self._jump(-1),
            "find-next": lambda: self._jump(1),
            "replace-one": self._replace_current,
            "replace-all": self._replace_all,
            "find-close": self._close_find,
        }
        actions[event.button.id]()

    def on_key(self, event) -> None:
        focused = self.focused
        bar = self.query_one("#findbar")
        if focused is None or not bar.display or bar not in focused.ancestors:
            return
        if event.key == "escape":
            self._close_find()
            event.stop()
        elif isinstance(focused, Input) and event.key in ("up", "down"):
            self._jump(-1 if event.key == "up" else 1)
            event.stop()

    # ---------------------------------------------------------- compilar

    @work
    async def action_compile(self) -> None:
        editor = self.editor
        if editor is None:
            self.notify("Abre o crea un documento primero.", severity="warning")
            return
        if editor.path is None and not await self._save(editor):
            return
        for other in self.all_editors():
            if other.dirty and other.path is not None:
                self._write(other)
        self._start_compile(editor)

    def _start_compile(self, editor: LatexEditor) -> None:
        engine = self._engine()
        if engine is None:
            self.notify(
                "No encontré ningún motor LaTeX.\n"
                "El más sencillo es Tectonic:  [b]brew install tectonic[/]\n"
                "También sirven latexmk, pdflatex, xelatex o lualatex.",
                title="Sin motor LaTeX",
                severity="error",
                timeout=12,
            )
            return
        if self.compiling:
            return
        assert editor.path is not None
        root = find_root(editor.path, editor.text)
        self.compiling = True
        self._update_chrome()
        self.run_worker(self._compile(root, *engine), exclusive=True, group="compile")

    async def _compile(self, root: Path, engine: str, executable: str) -> None:
        try:
            result = await compile_document(root, engine, executable)
        except Exception as error:
            self.compiling = False
            self.notify(f"No pude ejecutar {engine}: {error}", severity="error")
            self._update_chrome()
            return
        self.compiling = False
        self._show_result(result)

    def _show_result(self, result: CompileResult) -> None:
        self.last_result = result
        self.problems = result.problems

        variables = self.get_css_variables()
        colors = {
            "error": variables.get("error", "red"),
            "warning": variables.get("warning", "yellow"),
            "info": variables.get("text-muted", "grey"),
        }
        options = self.query_one("#problems", OptionList)
        options.clear_options()
        rows = []
        for problem in result.problems:
            text = Text(no_wrap=True, overflow="ellipsis")
            text.append(f"{ICONS.get(problem.severity, '·')} ", style=f"bold {colors.get(problem.severity, '')}")
            if problem.line is not None:
                text.append(f"{problem.file or root_name(result)}:{problem.line}  ", style="dim")
            text.append(problem.message)
            rows.append(Option(text))
        if not rows:
            rows.append(Option(Text("Sin problemas ✿", style="dim italic"), disabled=True))
        options.add_options(rows)

        log = self.query_one("#log", RichLog)
        log.clear()
        log.write(f"$ {result.engine} {result.root.name}\n")
        for line in result.output.splitlines()[-1500:]:
            log.write(line)

        panel = self.query_one("#panel")
        count = len(result.problems)
        panel.border_title = "Salida" if not count else f"Salida · {count}"
        if result.errors:
            panel.display = True
            self.query_one("#panel-tabs", TabbedContent).active = "panel-problems"

        if result.pdf is not None:
            self._show_preview(result.pdf, reveal=False)
        self.query_one("#files", ProjectTree).reload()

        if result.ok and not result.errors:
            self.notify(f"[b]{result.root.stem}.pdf[/] listo en {result.duration:.1f} s", title="✔ Compilado", timeout=3)
        else:
            self.notify(
                f"{result.errors or 1} error(es). Pulsa uno en el panel de problemas para ir a la línea.",
                title="✖ La compilación falló",
                severity="error",
                timeout=5,
            )
        self._update_chrome()

    def _show_preview(self, pdf: Path, reveal: bool = True) -> None:
        preview = self.query_one(PdfPreview)
        if reveal:
            preview.display = True
        preview.border_title = pdf.name
        preview.load(pdf)

    @on(OptionList.OptionSelected, "#problems")
    async def _problem_selected(self, event: OptionList.OptionSelected) -> None:
        if event.option_index >= len(self.problems) or self.last_result is None:
            return
        problem = self.problems[event.option_index]
        if problem.line is None:
            return
        base = self.last_result.root.parent
        path = (base / problem.file) if problem.file else self.last_result.root
        if not path.exists():
            path = self.last_result.root
        await self.open_path(path, line=problem.line - 1)

    @on(OptionList.OptionSelected, "#completion")
    def _completion_selected(self, event: OptionList.OptionSelected) -> None:
        editor = self.editor
        if editor is not None:
            editor.accept_completion()
            editor.focus()

    def action_open_pdf(self) -> None:
        pdf = self._current_pdf()
        if pdf is None:
            self.notify("Todavía no hay PDF: compila con F5.", severity="warning")
            return
        opener = "open" if sys.platform == "darwin" else "xdg-open"
        try:
            subprocess.Popen([opener, str(pdf)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as error:
            self.notify(f"No pude abrir el PDF: {error}", severity="error")

    def _current_pdf(self) -> Path | None:
        preview = self.query_one(PdfPreview)
        if preview.path is not None and preview.path.exists():
            return preview.path
        editor = self.editor
        if editor is not None and editor.path is not None:
            pdf = find_root(editor.path, editor.text).with_suffix(".pdf")
            if pdf.exists():
                return pdf
        return None

    # ------------------------------------------------------------ paneles

    def action_toggle_sidebar(self) -> None:
        sidebar = self.query_one("#sidebar")
        sidebar.display = not sidebar.display
        self.config.show_sidebar = bool(sidebar.display)
        self.config.save()

    def action_toggle_preview(self) -> None:
        preview = self.query_one(PdfPreview)
        preview.display = not preview.display
        self.config.show_preview = bool(preview.display)
        self.config.save()
        if preview.display and preview.path is None and (pdf := self._current_pdf()):
            self._show_preview(pdf)

    def action_toggle_panel(self) -> None:
        panel = self.query_one("#panel")
        panel.display = not panel.display

    def action_toggle_wrap(self) -> None:
        self.config.soft_wrap = not self.config.soft_wrap
        self.config.save()
        for editor in self.all_editors():
            editor.soft_wrap = self.config.soft_wrap
        self.notify("Ajuste de línea " + ("activado" if self.config.soft_wrap else "desactivado"), timeout=2)

    def action_toggle_autocompile(self) -> None:
        self.config.autocompile = not self.config.autocompile
        self.config.save()
        self._update_chrome()
        self.notify("Compilar al guardar: " + ("sí" if self.config.autocompile else "no"), timeout=2)

    def action_set_engine(self, name: str) -> None:
        self.config.engine = name
        self.config.save()
        self.engines = detect_engines()
        self._update_chrome()
        engine = self._engine()
        self.notify(f"Motor: [b]{engine[0] if engine else 'ninguno disponible'}[/]", timeout=2)

    def action_clean(self) -> None:
        editor = self.editor
        if editor is None or editor.path is None:
            return
        removed = clean_auxiliary(find_root(editor.path, editor.text))
        self.query_one("#files", ProjectTree).reload()
        self.notify(f"{len(removed)} archivos auxiliares borrados", timeout=2)

    # ------------------------------------------------------------- fondo

    def _backdrop_changed(self, message: str) -> None:
        self.config.background = str(self.backdrop.path) if self.backdrop.path else ""
        self.config.background_style = self.backdrop.style
        self.config.background_intensity = self.backdrop.intensity
        self.config.save()
        self._apply_photo_theme()
        self._sync_backdrop_colors()
        self._repaint()
        self.notify(message, timeout=2)

    def action_background_palette(self) -> None:
        self.config.background_palette = not self.config.background_palette
        self._backdrop_changed(
            "Colores tomados de la foto" if self.config.background_palette else "Colores del tema"
        )

    def _set_background(self, path: Path) -> None:
        if self.backdrop.load(path):
            self._backdrop_changed(f"Fondo: [b]{path.name}[/]")
        else:
            self.notify(f"No pude abrir {path.name} como imagen.", severity="error")

    @work
    async def action_background(self) -> None:
        raw = await self.push_screen_wait(
            InputScreen(
                "Imagen de fondo",
                "~/Pictures/fondo.png",
                hint="Escribe la ruta o arrastra la imagen a la terminal. PNG, JPG o WebP.",
            )
        )
        if not raw:
            return
        source = clean_path(raw)
        try:
            path = import_background(source)
        except Exception:
            self.notify(f"No pude abrir {source} como imagen.", severity="error")
            return
        self._set_background(path)

    def action_background_clear(self) -> None:
        self.backdrop.load(None)
        self._backdrop_changed("Fondo quitado")

    def action_background_style(self, style: str) -> None:
        self.backdrop.configure(style=style)
        self._backdrop_changed(f"Fondo {STYLES[style].lower()}")

    def action_background_intensity(self, delta: float) -> None:
        self.backdrop.configure(intensity=round(self.backdrop.intensity + delta, 2))
        self._backdrop_changed(f"Intensidad del fondo: {self.backdrop.intensity:.0%}")

    @on(PdfPreview.InvertChanged)
    def _invert_changed(self, event: PdfPreview.InvertChanged) -> None:
        self.config.invert_preview = event.invert
        self.config.save()

    def get_system_commands(self, screen: Screen) -> Iterable[SystemCommand]:
        yield SystemCommand("Compilar a PDF", "Compila el documento actual (F5)", self.action_compile)
        yield SystemCommand("Guardar", "Guarda el archivo actual (Ctrl+S)", self.action_save)
        yield SystemCommand("Nuevo documento", "Crea un archivo desde una plantilla (Ctrl+N)", self.action_new)
        yield SystemCommand("Abrir archivo", "Busca un archivo del proyecto (Ctrl+O)", self.action_open)
        yield SystemCommand("Cerrar pestaña", "Cierra el archivo actual (Ctrl+W)", self.action_close_tab)
        yield SystemCommand("Buscar y reemplazar", "Busca en el documento (Ctrl+F)", self.action_find)
        yield SystemCommand("Ir a línea", "Salta a un número de línea (Ctrl+G)", self.action_goto)
        yield SystemCommand("Insertar símbolo", "Buscador de símbolos matemáticos (Ctrl+T)", self.action_symbols)
        yield SystemCommand("Negrita", "Envuelve la selección en \\textbf (Ctrl+B)", self.action_bold)
        yield SystemCommand("Cursiva", "Envuelve la selección en \\textit (Ctrl+L)", self.action_italic)
        yield SystemCommand("Comentar líneas", "Comenta o descomenta la selección (Ctrl+/)", self.action_comment)
        yield SystemCommand("Abrir PDF en el visor del sistema", "Abre el PDF compilado (F6)", self.action_open_pdf)
        yield SystemCommand("Mostrar u ocultar barra lateral", "Archivos y esquema (F2)", self.action_toggle_sidebar)
        yield SystemCommand("Mostrar u ocultar vista previa", "Panel del PDF (F3)", self.action_toggle_preview)
        yield SystemCommand("Mostrar u ocultar problemas", "Errores, avisos y registro (F4)", self.action_toggle_panel)
        yield SystemCommand(
            "Compilar al guardar: " + ("desactivar" if self.config.autocompile else "activar"),
            "Compila automáticamente cada vez que guardas",
            self.action_toggle_autocompile,
        )
        yield SystemCommand(
            "Ajuste de línea: " + ("desactivar" if self.config.soft_wrap else "activar"),
            "Parte visualmente las líneas largas",
            self.action_toggle_wrap,
        )
        yield SystemCommand("Fondo: elegir imagen…", "Pone una foto detrás de toda la ventana", self.action_background)
        for saved in saved_backgrounds():
            if saved != self.backdrop.path:
                yield SystemCommand(f"Fondo: {saved.name}", "Usa esta imagen ya importada", lambda saved=saved: self._set_background(saved))
        if self.backdrop.active:
            for key, label in STYLES.items():
                if key != self.backdrop.style:
                    yield SystemCommand(f"Fondo: estilo {label.lower()}", "Cambia cómo se dibuja la imagen", lambda key=key: self.action_background_style(key))
            if self._can_use_pixels():
                yield SystemCommand(
                    "Fondo: " + ("dibujar por celdas" if self.backdrop.pixels else "píxeles reales (experimental)"),
                    "Dibuja la foto con el protocolo gráfico de la terminal, con puntos finos",
                    self.action_background_pixels,
                )
            if self.backdrop.tone:
                yield SystemCommand(
                    "Fondo: usar colores " + ("del tema" if self.config.background_palette else "de la foto"),
                    "Elige si la paleta de la app sale de la foto o del tema",
                    self.action_background_palette,
                )
            if self.backdrop.intensity < MAX_INTENSITY:
                yield SystemCommand("Fondo: más intenso", "Sube la presencia de la imagen", lambda: self.action_background_intensity(0.15))
            if self.backdrop.intensity > MIN_INTENSITY:
                yield SystemCommand("Fondo: más tenue", "Baja la presencia de la imagen", lambda: self.action_background_intensity(-0.15))
            yield SystemCommand("Fondo: quitar", "Vuelve al fondo liso del tema", self.action_background_clear)
        yield SystemCommand("Borrar archivos auxiliares", "Elimina .aux, .log, .toc… del documento", self.action_clean)
        yield SystemCommand("Motor: automático", "Usa el primer motor disponible", lambda: self.action_set_engine("auto"))
        for name in self.engines:
            yield SystemCommand(f"Motor: {name}", f"Compila con {name}", lambda name=name: self.action_set_engine(name))
        yield SystemCommand("Atajos de teclado", "Muestra la ayuda (F1)", self.action_help)
        yield SystemCommand("Tema", "Cambia los colores de la aplicación", self.action_change_theme)
        yield SystemCommand("Salir", "Cierra MiyuLaTeX (Ctrl+Q)", self.action_request_quit)


def root_name(result: CompileResult) -> str:
    return result.root.name


def _looks_textual(path: Path) -> bool:
    try:
        with path.open("rb") as handle:
            return b"\0" not in handle.read(2048)
    except OSError:
        return False
