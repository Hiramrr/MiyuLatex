"""Vista previa del PDF dentro de la terminal."""

from __future__ import annotations

import logging
import threading
from functools import partial
from pathlib import Path

import pypdfium2 as pdfium
from PIL import Image as PILImage
from PIL import ImageOps
from textual import on
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.message import Message
from textual.reactive import reactive
from textual.widgets import Button, Static

# Debe importarse antes de que Textual tome la terminal: detecta el protocolo
# gráfico disponible (Kitty, Sixel o medios bloques).
logging.getLogger("textual_image").setLevel(logging.CRITICAL)  # sin trazas si la terminal no responde
try:
    from textual_image.widget import Image
except Exception:  # la detección falla en terminales que no informan su tamaño
    Image = None

# ¿Sabe la terminal dibujar imágenes con el protocolo de Kitty? (Kitty, Ghostty…)
KITTY_GRAPHICS = False
CELL_PIXELS = (0, 0)
if Image is not None:
    try:
        from textual_image._terminal import probe_terminal

        _capabilities = probe_terminal()
        KITTY_GRAPHICS = bool(_capabilities.tgp)
        CELL_PIXELS = (int(_capabilities.cell_size.width), int(_capabilities.cell_size.height))
    except Exception:
        pass

ZOOMS = (50, 75, 100, 125, 150, 200, 300)
RENDER_SCALE = 2.0

_EMPTY = """\
[b $primary]✿[/]

[b]Sin vista previa todavía[/b]

[dim]Compila el documento con [/][b $accent]F5[/][dim] o [/][b $accent]Ctrl+R[/]
[dim]y el PDF aparecerá aquí.[/]
"""

_UNSUPPORTED = """\
[b $primary]✿[/]

[b]PDF listo[/b]

[dim]Esta terminal no permite dibujarlo aquí.[/]
[dim]Ábrelo en el visor del sistema con [/][b $accent]F6[/][dim].[/]
"""

_render_lock = threading.Lock()


def render_page(data: bytes, index: int, invert: bool) -> tuple[PILImage.Image, int]:
    """Rasteriza una página. Devuelve la imagen y el total de páginas."""
    with _render_lock:
        document = pdfium.PdfDocument(data)
        try:
            count = len(document)
            index = max(0, min(index, count - 1))
            page = document[index]
            image = page.render(scale=RENDER_SCALE).to_pil().convert("RGB")
            page.close()
        finally:
            document.close()
    if invert:
        image = ImageOps.invert(image)
    return image, count


class PdfPreview(Vertical):
    """Panel con el PDF compilado, página a página."""

    BINDINGS = [
        Binding("k,p", "previous_page", "Página anterior", show=False),
        Binding("j,n,space", "next_page", "Página siguiente", show=False),
        Binding("plus,equals_sign", "zoom(1)", "Acercar", show=False),
        Binding("minus", "zoom(-1)", "Alejar", show=False),
        Binding("i", "toggle_invert", "Invertir", show=False),
    ]

    class InvertChanged(Message):
        """El usuario cambió la inversión de colores."""

        def __init__(self, invert: bool) -> None:
            super().__init__()
            self.invert = invert

    page: reactive[int] = reactive(0, init=False)
    zoom: reactive[int] = reactive(100, init=False)
    invert: reactive[bool] = reactive(False, init=False)

    def __init__(self, **kwargs) -> None:
        super().__init__(**kwargs)
        self.path: Path | None = None
        self.page_count = 0
        self._data: bytes | None = None

    def compose(self) -> ComposeResult:
        with Horizontal(id="preview-bar"):
            yield Static("Vista previa", id="preview-title")
            yield Button("◀", id="pdf-prev", compact=True, tooltip="Página anterior")
            yield Static("–", id="pdf-page")
            yield Button("▶", id="pdf-next", compact=True, tooltip="Página siguiente")
            yield Button("−", id="pdf-zoom-out", compact=True, tooltip="Alejar")
            yield Static("100%", id="pdf-zoom")
            yield Button("+", id="pdf-zoom-in", compact=True, tooltip="Acercar")
            yield Button("◐", id="pdf-invert", compact=True, tooltip="Invertir colores")
        with VerticalScroll(id="preview-scroll"):
            yield Static(_EMPTY, id="preview-empty")
            if Image is not None:
                yield Image(id="preview-image")

    def on_mount(self) -> None:
        if Image is not None:
            self.query_one("#preview-image").display = False

    # --------------------------------------------------------------- API

    def load(self, path: Path) -> None:
        """Carga (o recarga) un PDF conservando la página actual."""
        try:
            data = path.read_bytes()
        except OSError as error:
            self.app.notify(f"No pude leer el PDF: {error}", severity="error")
            return
        same = self.path == path
        self.path = path
        self._data = data
        if not same:
            self.set_reactive(PdfPreview.page, 0)
        self._redraw()

    def clear(self) -> None:
        self.path = None
        self._data = None
        self.page_count = 0
        if Image is not None:
            self.query_one("#preview-image").display = False
        self.query_one("#preview-empty").display = True
        self.query_one("#pdf-page", Static).update("–")

    # ---------------------------------------------------------- dibujado

    def _redraw(self) -> None:
        if self._data is None:
            return
        if Image is None:
            self.query_one("#preview-empty", Static).update(_UNSUPPORTED)
            return
        self.run_worker(
            partial(self._render_worker, self._data, self.page, self.invert),
            thread=True,
            exclusive=True,
            group="pdf-render",
        )

    def _render_worker(self, data: bytes, index: int, invert: bool) -> None:
        try:
            image, count = render_page(data, index, invert)
        except Exception as error:  # PDF a medio escribir o dañado
            self.app.call_from_thread(self._show_error, str(error))
            return
        self.app.call_from_thread(self._show, image, count, index)

    def _show_error(self, message: str) -> None:
        self.app.notify(f"No pude dibujar el PDF: {message}", severity="warning")

    def _show(self, image: PILImage.Image, count: int, index: int) -> None:
        self.page_count = count
        self.set_reactive(PdfPreview.page, max(0, min(index, count - 1)))
        widget = self.query_one("#preview-image", Image)
        widget.image = image
        widget.display = True
        self.query_one("#preview-empty").display = False
        self._apply_zoom()
        self.query_one("#pdf-page", Static).update(f"{self.page + 1}/{count}")

    def _apply_zoom(self) -> None:
        if Image is None:
            return
        widget = self.query_one("#preview-image", Image)
        widget.styles.width = f"{self.zoom}%"
        widget.styles.height = "auto"
        self.query_one("#pdf-zoom", Static).update(f"{self.zoom}%")

    def watch_page(self) -> None:
        self._redraw()
        self.query_one("#preview-scroll").scroll_home(animate=False)

    def watch_zoom(self) -> None:
        self._apply_zoom()

    def watch_invert(self, invert: bool) -> None:
        self._redraw()
        self.post_message(self.InvertChanged(invert))

    # ---------------------------------------------------------- acciones

    def action_previous_page(self) -> None:
        if self.page > 0:
            self.page -= 1

    def action_next_page(self) -> None:
        if self.page < self.page_count - 1:
            self.page += 1

    def action_zoom(self, direction: int) -> None:
        index = ZOOMS.index(self.zoom) if self.zoom in ZOOMS else ZOOMS.index(100)
        self.zoom = ZOOMS[max(0, min(len(ZOOMS) - 1, index + direction))]

    def action_toggle_invert(self) -> None:
        self.invert = not self.invert

    @on(Button.Pressed)
    def _button(self, event: Button.Pressed) -> None:
        event.stop()
        actions = {
            "pdf-prev": self.action_previous_page,
            "pdf-next": self.action_next_page,
            "pdf-zoom-out": partial(self.action_zoom, -1),
            "pdf-zoom-in": partial(self.action_zoom, 1),
            "pdf-invert": self.action_toggle_invert,
        }
        action = actions.get(event.button.id or "")
        if action:
            action()
