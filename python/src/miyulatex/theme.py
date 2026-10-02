"""Temas de la aplicación y del editor.

El tema del editor se genera a partir de las variables de color del tema
activo, así que cualquier tema de Textual produce un resaltado coherente.
"""

from __future__ import annotations

from rich.style import Style
from textual.color import Color
from textual.theme import Theme
from textual.widgets.text_area import TextAreaTheme

MIYU_NIGHT = Theme(
    name="miyu-noche",
    primary="#ff8fb7",
    secondary="#b69cff",
    accent="#7ee0d2",
    warning="#ffd486",
    error="#ff6b8b",
    success="#9be8a8",
    foreground="#ece6ff",
    background="#16131f",
    surface="#1e1a2b",
    panel="#29233b",
    dark=True,
    variables={
        "border": "#3a3254",
        "border-blurred": "#2c2640",
        "block-cursor-background": "#ff8fb7",
        "block-cursor-foreground": "#16131f",
        "block-cursor-text-style": "bold",
        "footer-key-foreground": "#ff8fb7",
        "footer-background": "#1e1a2b",
        "input-selection-background": "#b69cff 35%",
        "scrollbar": "#3a3254",
        "scrollbar-hover": "#4d4370",
        "scrollbar-active": "#ff8fb7",
        "scrollbar-background": "#16131f",
    },
)

MIYU_DAY = Theme(
    name="miyu-dia",
    primary="#d6457f",
    secondary="#7a55e0",
    accent="#0e8f86",
    warning="#b7791f",
    error="#d63651",
    success="#2f9e58",
    foreground="#3a2c45",
    background="#fdf7fa",
    surface="#f7ebf2",
    panel="#efdde8",
    dark=False,
    variables={
        "border": "#e2c8d8",
        "border-blurred": "#ecdbe5",
        "block-cursor-background": "#d6457f",
        "block-cursor-foreground": "#fdf7fa",
        "footer-key-foreground": "#d6457f",
        "footer-background": "#f7ebf2",
        "input-selection-background": "#7a55e0 25%",
        "scrollbar": "#e2c8d8",
        "scrollbar-hover": "#d4b0c6",
        "scrollbar-active": "#d6457f",
        "scrollbar-background": "#fdf7fa",
    },
)

APP_THEMES = (MIYU_NIGHT, MIYU_DAY)
DEFAULT_THEME = MIYU_NIGHT.name
EDITOR_THEME = "miyu"


def _color(variables: dict[str, str], name: str, fallback: str) -> Color:
    try:
        return Color.parse(variables.get(name) or fallback)
    except Exception:
        return Color.parse(fallback)


def build_editor_theme(variables: dict[str, str], dark: bool) -> TextAreaTheme:
    """Crea el tema del ``TextArea`` a partir de las variables CSS del tema activo."""
    background = _color(variables, "background", "#16131f" if dark else "#ffffff")
    foreground = _color(variables, "foreground", "#ece6ff" if dark else "#222222")
    surface = _color(variables, "surface", background.hex)
    panel = _color(variables, "panel", surface.hex)
    primary = _color(variables, "primary", "#ff8fb7")
    secondary = _color(variables, "secondary", primary.hex)
    accent = _color(variables, "accent", secondary.hex)
    warning = _color(variables, "warning", "#ffd486")
    error = _color(variables, "error", "#ff6b8b")
    success = _color(variables, "success", "#9be8a8")

    def mix(a: Color, b: Color, amount: float) -> str:
        return a.blend(b, amount).hex

    bg = background.hex
    fg = foreground.hex
    muted = mix(foreground, background, 0.55)
    faint = mix(foreground, background, 0.72)
    math_fg = mix(accent, foreground, 0.15)
    cursor_line = mix(background, panel, 0.75)

    return TextAreaTheme(
        name=EDITOR_THEME,
        base_style=Style(color=fg, bgcolor=bg),
        gutter_style=Style(color=faint, bgcolor=bg),
        cursor_style=Style(color=bg, bgcolor=primary.hex),
        cursor_line_style=Style(bgcolor=cursor_line),
        cursor_line_gutter_style=Style(color=primary.hex, bgcolor=cursor_line, bold=True),
        bracket_matching_style=Style(bgcolor=mix(background, secondary, 0.4), bold=True),
        selection_style=Style(bgcolor=mix(background, secondary, 0.32)),
        syntax_styles={
            "comment": Style(color=muted, italic=True),
            "command": Style(color=primary.hex),
            "section": Style(color=secondary.hex, bold=True),
            "title": Style(color=mix(secondary, foreground, 0.45), bold=True),
            "item": Style(color=warning.hex, bold=True),
            "env": Style(color=secondary.hex),
            "env.name": Style(color=accent.hex, italic=True),
            "math": Style(color=math_fg),
            "math.delim": Style(color=accent.hex, bold=True),
            "math.command": Style(color=mix(success, accent, 0.35)),
            "brace": Style(color=mix(foreground, background, 0.4)),
            "bracket": Style(color=mix(foreground, background, 0.4)),
            "special": Style(color=error.hex),
            "ref": Style(color=warning.hex),
            "module": Style(color=success.hex),
            "string": Style(color=mix(warning, foreground, 0.3)),
            "verbatim": Style(color=mix(foreground, warning, 0.25), bgcolor=mix(background, surface, 0.8)),
            "bold": Style(bold=True),
            "italic": Style(italic=True),
            "underline": Style(underline=True),
            "search": Style(color=bg, bgcolor=mix(warning, background, 0.25)),
        },
    )
