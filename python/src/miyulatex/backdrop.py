"""Fondo con foto para toda la ventana.

Sigue el esquema de BetterThanEminus: la foto cubre la ventana entera, fundida
hacia el color de fondo del tema. Cada punto de la trama (Bayer 8×8) decide si
ese trozo de imagen se ve «encendido» o queda como una sombra tenue, así que la
foto se reconoce completa aunque esté tramada.

En una terminal eso se traduce así: la sombra es el color de fondo de cada
celda (también debajo del texto) y los puntos son caracteres braille (2×4 por
celda) en las celdas vacías. El estilo «liso» no trama: pinta la foto con
medios bloques.

El fondo se aplica en un único sitio, a la salida del dibujado de cada widget:
las celdas cuyo color de fondo es el del tema dejan ver la foto; las barras y
los diálogos, que usan otros colores, quedan opacos como tarjetas.
"""

from __future__ import annotations

import base64
import colorsys
import io
import math
import shutil
from pathlib import Path

from PIL import Image as PILImage
from PIL import ImageChops
from rich.cells import cell_len
from rich.segment import Segment
from rich.style import Style
from textual._styles_cache import StylesCache
from textual.geometry import Region
from textual.strip import Strip
from textual.theme import Theme
from textual.widget import Widget

from .config import config_path

STYLES = {"dither": "Tramado", "plain": "Liso"}
IMAGE_SUFFIXES = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".tiff"}
MIN_INTENSITY, MAX_INTENSITY = 0.25, 1.0
PHOTO_THEME = "miyu-foto"
MIN_RUN = 4
"""Huecos de menos celdas (los espacios entre palabras) no llevan puntos."""

RGB = tuple[int, int, int]
Cell = tuple[RGB, str, Style, RGB]
"""``(sombra bajo el texto, carácter de relleno, estilo de la celda vacía, su fondo)``."""

# Pantallas con protocolo gráfico de Kitty: la imagen va por debajo incluso de
# las celdas con color de fondo, así solo asoma donde la celda no pinta fondo.
KITTY_Z = -1073741825
KITTY_ID = 7401


def _bayer(size: int = 8) -> list[list[float]]:
    matrix = [[0]]
    n = 1
    while n < size:
        matrix = [
            [4 * matrix[y % n][x % n] + (0, 2, 3, 1)[(y >= n) * 2 + (x >= n)] for x in range(n * 2)]
            for y in range(n * 2)
        ]
        n *= 2
    return [[(value + 0.5) / (size * size) for value in row] for row in matrix]


BAYER = _bayer()

# Bit braille de cada punto de la celda, en orden de fila y columna.
_DOTS = ((0x01, 0x08), (0x02, 0x10), (0x04, 0x20), (0x40, 0x80))


def _smoothstep(a: float, b: float, x: float) -> float:
    t = min(1.0, max(0.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def _hex(color: RGB) -> str:
    return "#%02x%02x%02x" % color


def _hsl(h: float, s: float, l: float) -> str:
    r, g, b = colorsys.hls_to_rgb((h % 360) / 360, l, s)
    return _hex((round(r * 255), round(g * 255), round(b * 255)))


def backgrounds_dir() -> Path:
    return config_path().parent / "backgrounds"


def saved_backgrounds() -> list[Path]:
    directory = backgrounds_dir()
    if not directory.is_dir():
        return []
    return sorted(p for p in directory.iterdir() if p.suffix.lower() in IMAGE_SUFFIXES)


def clean_path(raw: str) -> Path:
    """Interpreta una ruta escrita o arrastrada a la terminal (comillas, espacios escapados)."""
    text = raw.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "'\"":
        text = text[1:-1]
    text = text.replace("\\ ", " ").removeprefix("file://")
    return Path(text).expanduser()


def import_background(source: Path) -> Path:
    """Copia la imagen a la carpeta de fondos y devuelve la copia."""
    with PILImage.open(source) as image:
        image.verify()
    directory = backgrounds_dir()
    directory.mkdir(parents=True, exist_ok=True)
    target = directory / source.name
    if source.resolve() != target.resolve():
        shutil.copyfile(source, target)
    return target


def dominant_tone(image: PILImage.Image) -> tuple[float, float] | None:
    """Tono dominante ``(h, s)``: histograma de 36 tonos pesado por saturación.

    Los píxeles casi negros, blancos o grises no votan. ``None`` si la foto
    apenas tiene color.
    """
    pixels = image.convert("RGB").resize((64, 64)).tobytes()
    bins = [[0.0, 0.0, 0.0, 0.0] for _ in range(36)]  # peso, saturación, x, y
    total = len(pixels) // 3
    for i in range(0, len(pixels), 3):
        r, g, b = pixels[i] / 255, pixels[i + 1] / 255, pixels[i + 2] / 255
        high, low = max(r, g, b), min(r, g, b)
        lightness, delta = (high + low) / 2, high - low
        if delta < 0.03 or lightness < 0.06 or lightness > 0.96:
            continue
        saturation = delta / (1 - abs(2 * lightness - 1))
        if high == r:
            hue = ((g - b) / delta) % 6
        elif high == g:
            hue = (b - r) / delta + 2
        else:
            hue = (r - g) / delta + 4
        hue = (hue * 60) % 360
        weight = saturation**1.5  # los colores vivos pesan más que los apagados
        entry = bins[int(hue // 10) % 36]
        entry[0] += weight
        entry[1] += saturation * weight
        entry[2] += math.cos(math.radians(hue)) * weight
        entry[3] += math.sin(math.radians(hue)) * weight
    # Se suman los vecinos para que un tono repartido entre dos cubetas no pierda.
    best, best_weight = None, 0.0
    for i, entry in enumerate(bins):
        weight = bins[i - 1][0] * 0.5 + entry[0] + bins[(i + 1) % 36][0] * 0.5
        if weight > best_weight:
            best, best_weight = entry, weight
    if best is None or best[0] == 0 or best_weight / total < 0.002:
        return None
    return math.degrees(math.atan2(best[3], best[2])) % 360, best[1] / best[0]


def photo_theme(tone: tuple[float, float], dark: bool) -> Theme:
    """Tema completo a partir del tono de la foto: neutros teñidos y un acento saturado."""
    h, saturation = tone
    sat = min(0.75, max(0.42, saturation))
    if dark:
        return Theme(
            name=PHOTO_THEME,
            primary=_hsl(h, sat, 0.68),
            secondary=_hsl(h + 35, sat * 0.75, 0.76),
            accent=_hsl(h + 180, sat * 0.55, 0.72),
            warning="#ffd486",
            error="#ff6b8b",
            success="#9be8a8",
            foreground=_hsl(h, 0.18, 0.88),
            background=_hsl(h, 0.16, 0.085),
            surface=_hsl(h, 0.14, 0.13),
            panel=_hsl(h, 0.12, 0.19),
            dark=True,
            variables={
                "border": _hsl(h, 0.12, 0.30),
                "border-blurred": _hsl(h, 0.12, 0.22),
                "block-cursor-background": _hsl(h, sat, 0.68),
                "block-cursor-foreground": _hsl(h, 0.16, 0.085),
                "footer-key-foreground": _hsl(h, sat, 0.68),
                "footer-background": _hsl(h, 0.14, 0.13),
                "scrollbar": _hsl(h, 0.12, 0.30),
                "scrollbar-hover": _hsl(h, 0.14, 0.40),
                "scrollbar-active": _hsl(h, sat, 0.68),
                "scrollbar-background": _hsl(h, 0.16, 0.085),
            },
        )
    return Theme(
        name=PHOTO_THEME,
        primary=_hsl(h, sat, 0.40),
        secondary=_hsl(h + 35, sat * 0.85, 0.36),
        accent=_hsl(h + 180, sat * 0.7, 0.32),
        warning="#b7791f",
        error="#d63651",
        success="#2f9e58",
        foreground=_hsl(h, 0.22, 0.17),
        background=_hsl(h, 0.30, 0.965),
        surface=_hsl(h, 0.22, 0.925),
        panel=_hsl(h, 0.16, 0.86),
        dark=False,
        variables={
            "border": _hsl(h, 0.14, 0.76),
            "border-blurred": _hsl(h, 0.14, 0.84),
            "block-cursor-background": _hsl(h, sat, 0.40),
            "block-cursor-foreground": _hsl(h, 0.30, 0.965),
            "footer-key-foreground": _hsl(h, sat, 0.40),
            "footer-background": _hsl(h, 0.22, 0.925),
            "scrollbar": _hsl(h, 0.14, 0.76),
            "scrollbar-hover": _hsl(h, 0.14, 0.66),
            "scrollbar-active": _hsl(h, sat, 0.40),
            "scrollbar-background": _hsl(h, 0.30, 0.965),
        },
    )


class Backdrop:
    """La foto convertida a celdas de terminal y el pintado sobre lo ya dibujado."""

    def __init__(self) -> None:
        self.path: Path | None = None
        self.style = "dither"
        self.intensity = 0.7
        self.tone: tuple[float, float] | None = None
        self.pixels = False
        """Si la foto la dibuja la terminal en píxeles reales y las celdas solo la dejan asomar."""
        self._image: PILImage.Image | None = None
        self._base: RGB | None = None
        self._shifts: dict[RGB, RGB] = {}
        """Color de fondo que deja ver la foto -> diferencia respecto al fondo del tema."""
        self._grids: dict[tuple, list[list[Cell]]] = {}
        self._merged: dict[tuple, Style] = {}
        self._painted: dict[tuple, tuple[Strip, Strip]] = {}

    # ------------------------------------------------------------- estado

    @property
    def active(self) -> bool:
        return self._image is not None and self._base is not None

    def load(self, path: Path | None) -> bool:
        """Carga la imagen (o la quita con ``None``). Devuelve si quedó una foto cargada."""
        self._reset()
        self._image = None
        self.path = None
        self.tone = None
        if path is None:
            return False
        try:
            with PILImage.open(path) as image:
                image = image.convert("RGB")
                # No hace falta más resolución que la rejilla de puntos de una terminal grande.
                image.thumbnail((1600, 1600))
                self._image = image.copy()
        except (OSError, ValueError):
            return False
        self.path = path
        self.tone = dominant_tone(self._image)
        return True

    def configure(self, style: str | None = None, intensity: float | None = None) -> None:
        if style in STYLES:
            self.style = style
        if intensity is not None:
            self.intensity = max(MIN_INTENSITY, min(MAX_INTENSITY, intensity))
        self._reset()

    def set_colors(self, base: RGB, also: tuple[RGB, ...] = ()) -> None:
        """Fija el fondo del tema y otros fondos que también dejan ver la foto."""
        shifts = {base: (0, 0, 0)}
        for color in also:
            shifts[color] = (color[0] - base[0], color[1] - base[1], color[2] - base[2])
        if base != self._base or shifts != self._shifts:
            self._base = base
            self._shifts = shifts
            self._reset()

    def _reset(self) -> None:
        self._grids.clear()
        self._merged.clear()
        self._painted.clear()

    # ------------------------------------------------------------ rejilla

    def _cover(self, width: int, height: int) -> PILImage.Image:
        """La foto escalada para cubrir ``width × height`` y recortada al centro."""
        image = self._image
        assert image is not None
        scale = max(width / image.width, height / image.height)
        scaled = image.resize(
            (max(width, round(image.width * scale)), max(height, round(image.height * scale))),
            PILImage.LANCZOS,
        )
        left = (scaled.width - width) // 2
        top = (scaled.height - height) // 2
        return scaled.crop((left, top, left + width, top + height))

    def grid(self, width: int, height: int) -> list[list[Cell]]:
        """Celdas del fondo para una pantalla de ``width × height``."""
        key = (width, height)
        cached = self._grids.get(key)
        if cached is not None:
            return cached
        if not self.active or width <= 0 or height <= 0:
            return []
        if len(self._grids) > 4:
            self._grids.clear()
        builder = self._dither_grid if self.style == "dither" else self._plain_grid
        result = builder(width, height)
        self._grids[key] = result
        return result

    @staticmethod
    def _presence(row: int, rows: int) -> float:
        # Se ve entera arriba y se va apagando hacia abajo sin desaparecer.
        return 1 - 0.55 * _smoothstep(0.2, 1.0, row / max(1, rows))

    def _blend(self, color: tuple[int, int, int], k: float, mask: int = ~3) -> RGB:
        base = self._base
        assert base is not None
        return (
            max(0, min(255, int(base[0] + (color[0] - base[0]) * k))) & mask,
            max(0, min(255, int(base[1] + (color[1] - base[1]) * k))) & mask,
            max(0, min(255, int(base[2] + (color[2] - base[2]) * k))) & mask,
        )

    def _lit(self, photo: PILImage.Image) -> PILImage.Image:
        """Máscara de la trama: qué puntos de ``photo`` quedan «encendidos»."""
        base = self._base
        assert base is not None
        width, height = photo.size
        # Más puntos donde la imagen se aleja del fondo.
        base_lum = (0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2]) / 255
        contrast = photo.convert("L").point(
            [min(255, int((abs(v / 255 - base_lum) * 1.6 + 0.1) * 255)) for v in range(256)]
        )
        product = ImageChops.multiply(contrast, self._rows_mask(width, height, lambda v: v))
        tile = PILImage.new("L", (8, 8))
        tile.putdata([min(255, round(value / 1.1 * 255)) for row in BAYER for value in row])
        threshold = PILImage.new("L", (width, height))
        for ty in range(0, height, 8):
            for tx in range(0, width, 8):
                threshold.paste(tile, (tx, ty))
        return ImageChops.subtract(product, threshold)

    def _rows_mask(self, width: int, height: int, value) -> PILImage.Image:
        """Imagen en grises con ``value(presencia de la fila)`` en cada fila."""
        column = PILImage.new("L", (1, height))
        column.putdata([max(0, min(255, round(value(self._presence(y, height)) * 255))) for y in range(height)])
        return column.resize((width, height))

    def _dither_grid(self, width: int, height: int) -> list[list[Cell]]:
        dots_w, dots_h = width * 2, height * 4
        photo = self._cover(dots_w, dots_h)
        average = photo.resize((width, height), PILImage.BOX).tobytes()
        lit = self._lit(photo).tobytes()

        strength = self.intensity
        styles: dict[tuple, Style] = {}
        result: list[list[Cell]] = []
        for cy in range(height):
            v = self._presence(cy, height)
            dot_k = (0.35 + 0.65 * v) * strength
            shade_k = 0.25 * v * v * v * strength
            rows = [(cy * 4 + dy) * dots_w for dy in range(4)]
            line: list[Cell] = []
            for cx in range(width):
                i = (cy * width + cx) * 3
                color = (average[i], average[i + 1], average[i + 2])
                bits = 0
                x = cx * 2
                for dy in range(4):
                    start = rows[dy] + x
                    if lit[start]:
                        bits |= _DOTS[dy][0]
                    if lit[start + 1]:
                        bits |= _DOTS[dy][1]
                # Donde la foto se parece al fondo queda una sombra tenue; los puntos van encima.
                shade = self._blend(color, shade_k, ~1)
                dot = self._blend(color, dot_k, ~3) if bits else shade
                key = (dot, shade)
                style = styles.get(key)
                if style is None:
                    style = styles[key] = Style(color=_hex(dot), bgcolor=_hex(shade))
                line.append((shade, chr(0x2800 + bits) if bits else " ", style, shade))
            result.append(line)
        return result

    def _plain_grid(self, width: int, height: int) -> list[list[Cell]]:
        pixels = self._cover(width, height * 2).tobytes()
        styles: dict[tuple, Style] = {}
        result: list[list[Cell]] = []
        for cy in range(height):
            v = self._presence(cy, height) * self.intensity
            line: list[Cell] = []
            for cx in range(width):
                i = (cy * 2 * width + cx) * 3
                j = i + width * 3
                upper = (pixels[i], pixels[i + 1], pixels[i + 2])
                lower = (pixels[j], pixels[j + 1], pixels[j + 2])
                top = self._blend(upper, 0.55 * v)
                bottom = self._blend(lower, 0.55 * v)
                middle = ((upper[0] + lower[0]) // 2, (upper[1] + lower[1]) // 2, (upper[2] + lower[2]) // 2)
                key = (top, bottom)
                style = styles.get(key)
                if style is None:
                    style = styles[key] = Style(color=_hex(top), bgcolor=_hex(bottom))
                line.append((self._blend(middle, 0.2 * v), "▀", style, bottom))
            result.append(line)
        return result

    # ------------------------------------------------------- píxeles reales

    def render_pixels(self, width: int, height: int, dot: int) -> PILImage.Image:
        """La foto tramada a resolución de píxel, como el lienzo de BetterThanEminus.

        ``dot`` es el lado de cada punto de la trama en píxeles de pantalla.
        """
        base = self._base
        assert base is not None and self._image is not None
        if self.style != "dither":
            photo = self._cover(width, height)
            flat = PILImage.new("RGB", photo.size, base)
            return PILImage.composite(photo, flat, self._rows_mask(width, height, lambda v: v * self.intensity * 0.55))
        w, h = -(-width // dot), -(-height // dot)
        photo = self._cover(w, h)
        flat = PILImage.new("RGB", (w, h), base)
        strength = self.intensity
        dots = PILImage.composite(photo, flat, self._rows_mask(w, h, lambda v: (0.35 + 0.65 * v) * strength))
        shade = PILImage.composite(photo, flat, self._rows_mask(w, h, lambda v: 0.25 * v**3 * strength))
        lit = self._lit(photo).point([0] + [255] * 255)
        image = PILImage.composite(dots, shade, lit)
        return image.resize((w * dot, h * dot), PILImage.NEAREST).crop((0, 0, width, height))

    def kitty_place(self, columns: int, rows: int, cell_width: int, cell_height: int) -> str:
        """Secuencias que colocan la foto bajo toda la pantalla (protocolo gráfico de Kitty)."""
        dot = max(2, round(cell_width / 4.5))
        image = self.render_pixels(columns * cell_width, rows * cell_height, dot)
        buffer = io.BytesIO()
        image.save(buffer, format="PNG", compress_level=3)
        data = base64.standard_b64encode(buffer.getvalue()).decode("ascii")
        chunks = [data[i : i + 4096] for i in range(0, len(data), 4096)] or [""]
        parts = ["\x1b7\x1b[H"]
        for index, chunk in enumerate(chunks):
            more = 1 if index < len(chunks) - 1 else 0
            if index == 0:
                control = f"a=T,f=100,t=d,i={KITTY_ID},p=1,q=2,C=1,c={columns},r={rows},z={KITTY_Z},m={more}"
            else:
                control = f"m={more}"
            parts.append(f"\x1b_G{control};{chunk}\x1b\\")
        parts.append("\x1b8")
        return "".join(parts)

    @staticmethod
    def kitty_clear() -> str:
        return f"\x1b_Ga=d,d=I,i={KITTY_ID},q=2\x1b\\"

    # ------------------------------------------------------------ pintado

    def _under(self, style: Style, shade: RGB, shift: RGB) -> Style:
        """El estilo de una celda con texto, con la foto como color de fondo."""
        key = (style, shade, shift)
        merged = self._merged.get(key)
        if merged is None:
            if len(self._merged) > 20000:
                self._merged.clear()
            color = (
                max(0, min(255, shade[0] + shift[0])),
                max(0, min(255, shade[1] + shift[1])),
                max(0, min(255, shade[2] + shift[2])),
            )
            merged = self._merged[key] = style + Style(bgcolor=_hex(color))
        return merged

    def paint_widget(self, widget: Widget, crop: Region, strips: list[Strip]) -> list[Strip]:
        """Pone la foto detrás de las líneas ya dibujadas de un widget."""
        screen = widget.app.size
        grid = self.grid(screen.width, screen.height)
        if not grid:
            return strips
        region = widget.region
        left = region.x + crop.x
        top = region.y + crop.y
        painted = self._painted
        if len(painted) > 4000:
            painted.clear()
        result = []
        for index, strip in enumerate(strips):
            y = top + index
            if not 0 <= y < len(grid):
                result.append(strip)
                continue
            key = (id(strip), left, y)
            cached = painted.get(key)
            if cached is not None and cached[0] is strip:
                result.append(cached[1])
                continue
            new = self._paint(strip, grid[y], left)
            painted[key] = (strip, new)
            result.append(new)
        return result

    def _paint(self, strip: Strip, row: list[Cell], left: int) -> Strip:
        shifts = self._shifts
        segments = strip._segments

        # Primera pasada: qué celdas son huecos lo bastante anchos para llevar puntos.
        cells: list[bool] = []
        touched = False
        for segment in segments:
            if segment.control:
                continue
            style = segment.style
            bgcolor = style.bgcolor if style is not None else None
            if bgcolor is not None and bgcolor.triplet in shifts:
                touched = True
                text = segment.text
                if text.isascii():
                    cells.extend(char == " " for char in text)
                else:
                    for char in text:
                        cells.extend([False] * max(1, cell_len(char)))
            else:
                cells.extend([False] * segment.cell_length)
        if not touched:
            return strip
        total = len(cells)
        fill = [False] * total
        x = 0
        while x < total:
            if not cells[x]:
                x += 1
                continue
            end = x
            while end < total and cells[end]:
                end += 1
            if end - x >= MIN_RUN:
                fill[x:end] = [True] * (end - x)
            x = end

        if self.pixels:
            return self._paint_holes(strip, fill)

        # Segunda pasada: sombra debajo de todo, puntos en los huecos.
        output: list[Segment] = []
        width = len(row)
        x = 0
        for segment in segments:
            style = segment.style
            if segment.control:
                output.append(segment)
                continue
            bgcolor = style.bgcolor if style is not None else None
            shift = shifts.get(bgcolor.triplet) if bgcolor is not None else None
            if shift is None:
                output.append(segment)
                x += segment.cell_length
                continue
            plain = shift == (0, 0, 0)
            run: list[str] = []
            run_style: Style | None = None
            for char in segment.text:
                size = 1 if char.isascii() else max(1, cell_len(char))
                column = left + x
                if 0 <= column < width:
                    shade, glyph, empty, empty_bg = row[column]
                    if fill[x] and size == 1:
                        char = glyph
                        cell_style = empty if plain else self._under(empty, empty_bg, shift)
                    else:
                        cell_style = self._under(style, shade, shift)
                else:
                    cell_style = style
                if cell_style is not run_style:
                    if run:
                        output.append(Segment("".join(run), run_style))
                        run = []
                    run_style = cell_style
                run.append(char)
                x += size
            if run:
                output.append(Segment("".join(run), run_style))
        return Strip(output, strip.cell_length)


    def _paint_holes(self, strip: Strip, fill: list[bool]) -> Strip:
        """Quita el color de fondo de los huecos para que se vea la imagen que hay debajo."""
        base = self._base
        output: list[Segment] = []
        x = 0
        for segment in strip._segments:
            style = segment.style
            if segment.control:
                output.append(segment)
                continue
            bgcolor = style.bgcolor if style is not None else None
            if bgcolor is None or tuple(bgcolor.triplet or ()) != base:
                output.append(segment)
                x += segment.cell_length
                continue
            clear = self._merged.get((style, None))
            if clear is None:
                clear = self._merged[(style, None)] = Style(color=style.color)
            text = segment.text
            start = 0
            for index, char in enumerate(text):
                hole = char == " " and fill[x]
                x += 1 if char.isascii() else max(1, cell_len(char))
                end = index + 1
                if end < len(text) and (text[end] == " " and fill[x]) == hole:
                    continue
                output.append(Segment(text[start:end], clear if hole else style))
                start = end
        return Strip(output, strip.cell_length)


_installed = False


def install() -> None:
    """Engancha el fondo a la salida del dibujado de todos los widgets.

    ``StylesCache.render_widget`` es el único punto por el que pasa cada línea
    (contenido, relleno y bordes) antes de llegar a la pantalla.
    """
    global _installed
    if _installed:
        return
    _installed = True
    original = StylesCache.render_widget

    def render_widget(self: StylesCache, widget: Widget, crop: Region) -> list[Strip]:
        strips = original(self, widget, crop)
        backdrop = getattr(widget.app, "backdrop", None)
        if backdrop is None or not backdrop.active:
            return strips
        return backdrop.paint_widget(widget, crop, strips)

    StylesCache.render_widget = render_widget
