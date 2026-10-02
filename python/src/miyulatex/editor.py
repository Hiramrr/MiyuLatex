"""El editor: un ``TextArea`` que entiende LaTeX."""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path

from rich.text import Text
from textual import events
from textual.message import Message
from textual.widgets import OptionList, TextArea
from textual.widgets.option_list import Option
from textual.widgets.text_area import Selection, TextAreaTheme

from .highlight import tokenize
from .outline import find_cite_keys, find_labels
from .snippets import COMMANDS, ENV_ARGS, ENVIRONMENTS, LIST_ENVS
from .theme import EDITOR_THEME

PAIRS = {"{": "}", "[": "]", "(": ")", "$": "$"}
CLOSERS = frozenset(PAIRS.values())
INDENT = "    "

_CTX_ENV = re.compile(r"\\(begin|end)\{([a-zA-Z*]*)$")
_CTX_REF = re.compile(r"\\(?:ref|eqref|pageref|autoref|cref|Cref)\{([^{}\s]*)$")
_CTX_CITE = re.compile(r"\\(?:cite[a-z]*|parencite|textcite|nocite)(?:\[[^\]]*\])*\{(?:[^{}]*,\s*)?([^{},\s]*)$")
_CTX_COMMAND = re.compile(r"\\([a-zA-Z]+)$")
_BEGIN_AT_END = re.compile(r"\\begin\{([^{}]+)\}(?:\[[^\]]*\]|\{[^{}]*\})*\s*$")
_ITEM_ONLY = re.compile(r"^\s*\\item(?:\[\])?\s*$")
_ITEM_START = re.compile(r"^\s*\\item\b")
_LEADING = re.compile(r"[ \t]*")

MAX_COMPLETIONS = 60


@dataclass(frozen=True)
class Completion:
    label: str
    insert: str
    detail: str = ""
    kind: str = "command"


class CompletionPopup(OptionList, can_focus=False):
    """Lista flotante de sugerencias bajo el cursor."""

    def __init__(self) -> None:
        super().__init__(id="completion")
        self.items: list[Completion] = []

    @property
    def is_open(self) -> bool:
        return bool(self.display) and bool(self.items)

    def open(self, items: list[Completion], x: int, y: int) -> None:
        self.items = items
        self.clear_options()
        self.add_options([Option(self._render_item(item)) for item in items])
        self.highlighted = 0
        self.place(x, y)
        self.display = True

    def place(self, x: int, y: int) -> None:
        """Coloca la lista junto a la celda ``(x, y)`` de la pantalla."""
        height = min(len(self.items), 8) + 2
        width = 44
        screen = self.screen.size
        # Si no cabe debajo del cursor (sobre la barra de estado), se abre encima.
        top = y + 1 if y + 1 + height <= screen.height - 2 else max(0, y - height)
        left = max(0, min(x, screen.width - width))
        self.styles.offset = (left, top)

    def close(self) -> None:
        if self.display:
            self.display = False
        self.items = []

    def move(self, delta: int) -> None:
        if not self.items:
            return
        current = self.highlighted or 0
        self.highlighted = (current + delta) % len(self.items)

    @property
    def current(self) -> Completion | None:
        if not self.items or self.highlighted is None:
            return None
        return self.items[self.highlighted]

    @staticmethod
    def _render_item(item: Completion) -> Text:
        icons = {"command": "λ", "env": "▣", "label": "#", "cite": "❝"}
        text = Text(no_wrap=True, overflow="ellipsis")
        text.append(f"{icons.get(item.kind, '·')} ", style="dim")
        text.append(item.label, style="bold")
        if item.detail:
            text.append(f"  {item.detail}", style="dim italic")
        return text


class LatexEditor(TextArea):
    """Editor con resaltado, autocompletado y edición asistida para LaTeX."""

    class CursorMoved(Message):
        """El cursor cambió de posición."""

        def __init__(self, editor: "LatexEditor") -> None:
            super().__init__()
            self.editor = editor

    class Escaped(Message):
        """Se pulsó Escape sin nada que cerrar en el editor."""

    def __init__(self, text: str = "", *, path: Path | None = None, **kwargs) -> None:
        # TextArea ya usa estos atributos dentro de su constructor.
        self.path = path
        self.saved_text = text
        self.search_matches: list[tuple[int, int, int]] = []
        self.completion: CompletionPopup | None = None
        self._completion_range: tuple[int, int, int] | None = None
        """``(fila, columna inicial, columna final)`` del texto que se reemplaza."""
        self._highlight = self._is_latex(path)
        super().__init__(
            text,
            soft_wrap=True,
            tab_behavior="indent",
            show_line_numbers=True,
            **kwargs,
        )
        self.indent_width = len(INDENT)

    # ------------------------------------------------------------- estado

    @staticmethod
    def _is_latex(path: Path | None) -> bool:
        return path is None or path.suffix.lower() in {".tex", ".sty", ".cls", ".ltx", ".tikz", ".bib"}

    @property
    def dirty(self) -> bool:
        return self.text != self.saved_text

    @property
    def title(self) -> str:
        return self.path.name if self.path else "sin-titulo.tex"

    def mark_saved(self) -> None:
        self.saved_text = self.text

    def apply_theme(self, theme: TextAreaTheme) -> None:
        self.register_theme(theme)
        if self.theme == EDITOR_THEME:
            self._set_theme(EDITOR_THEME)
            self._line_cache.clear()
            self.refresh()
        else:
            self.theme = EDITOR_THEME

    # --------------------------------------------------------- resaltado

    def _build_highlight_map(self) -> None:
        self._line_cache.clear()
        highlights = self._highlights
        highlights.clear()
        lines = self.document.lines
        rows: dict[int, list] = {}
        if self._highlight:
            for row, spans in enumerate(tokenize(lines)):
                if spans:
                    rows[row] = list(spans)
        # Las coincidencias de búsqueda van al final para pintarse encima.
        for row, start, end in self.search_matches:
            if row < len(lines):
                rows.setdefault(row, []).append((start, end, "search"))

        for row, spans in rows.items():
            line = lines[row]
            if line.isascii():
                highlights[row] = spans
                continue
            # TextArea espera columnas en bytes UTF-8.
            offsets = [0]
            total = 0
            for char in line:
                total += len(char.encode("utf-8"))
                offsets.append(total)
            last = len(offsets) - 1
            highlights[row] = [
                (offsets[min(start, last)], offsets[min(end, last)], name)
                for start, end, name in spans
            ]

    def set_search(self, query: str) -> list[tuple[int, int, int]]:
        """Resalta todas las apariciones de ``query`` y las devuelve."""
        self.search_matches = self.find_all(query)
        self._build_highlight_map()
        self.refresh()
        return self.search_matches

    def find_all(self, query: str) -> list[tuple[int, int, int]]:
        """Apariciones como ``(fila, inicio, fin)``. Distingue mayúsculas solo si la consulta las tiene."""
        if not query:
            return []
        sensitive = query != query.lower()
        needle = query if sensitive else query.lower()
        matches = []
        for row, line in enumerate(self.document.lines):
            haystack = line if sensitive else line.lower()
            if len(haystack) != len(line):
                haystack = line
            position = haystack.find(needle)
            while position != -1:
                matches.append((row, position, position + len(needle)))
                position = haystack.find(needle, position + len(needle))
        return matches

    # ----------------------------------------------------------- teclado

    async def _on_key(self, event: events.Key) -> None:
        popup = self.completion
        key = event.key

        if popup is not None and popup.is_open:
            if key in ("down", "up"):
                popup.move(1 if key == "down" else -1)
                self._consume(event)
                return
            if key == "tab" or (key == "enter" and not self._completion_is_typed()):
                self.accept_completion()
                self._consume(event)
                return
            if key == "enter":
                popup.close()
            if key == "escape":
                popup.close()
                self._consume(event)
                return

        if key == "escape":
            # TextArea usaría Escape para soltar el foco; aquí no queremos eso.
            self._consume(event)
            self.post_message(self.Escaped())
            return

        if self.read_only:
            return

        if key == "enter":
            self._consume(event)
            self._smart_newline()
            return

        if key == "tab" and self._selection_spans_lines():
            self._consume(event)
            self.indent_selection()
            return

        if key == "shift+tab":
            self._consume(event)
            self.dedent_selection()
            return

        char = event.character
        if event.is_printable and char:
            if self._handle_pairs(char):
                self._consume(event)
                self._update_completion()
                return
            # La inserción la hace TextArea después de este manejador.
            self.call_later(self._update_completion)

    @staticmethod
    def _consume(event: events.Key) -> None:
        event.stop()
        event.prevent_default()

    def _selection_spans_lines(self) -> bool:
        start, end = self.selection
        return start[0] != end[0]

    def _char_at(self, row: int, column: int) -> str:
        line = self.document.get_line(row)
        return line[column] if 0 <= column < len(line) else ""

    def _handle_pairs(self, char: str) -> bool:
        """Cierra pares automáticamente. Devuelve ``True`` si ya insertó el texto."""
        start, end = sorted(self.selection)
        row, column = end
        following = self._char_at(row, column)
        previous = self._char_at(row, column - 1)

        if start != end:
            if char in PAIRS:
                selected = self.get_text_range(start, end)
                self.replace(char + selected + PAIRS[char], start, end)
                return True
            return False

        # Escribir el cierre cuando ya está ahí solo avanza el cursor.
        if char in CLOSERS and following == char and (char != "$" or previous != "\\"):
            self.move_cursor((row, column + 1))
            return True

        if char in PAIRS:
            if previous == "\\":
                return False
            if following and (following.isalnum() or following == "\\"):
                return False
            if char == "$" and len(re.findall(r"(?<!\\)\$", self.document.get_line(row)[:column])) % 2:
                # Hay un $ abierto en la línea: este lo cierra.
                return False
            self.replace(char + PAIRS[char], start, end)
            self.move_cursor((row, column + 1))
            return True
        return False

    def action_delete_left(self) -> None:
        start, end = self.selection
        if start == end and not self.read_only:
            row, column = end
            previous = self._char_at(row, column - 1)
            if previous in PAIRS and self._char_at(row, column) == PAIRS[previous]:
                self.delete((row, column - 1), (row, column + 1))
                self._update_completion()
                return
        super().action_delete_left()
        self._update_completion()

    def _smart_newline(self) -> None:
        start, end = sorted(self.selection)
        row, column = end
        line = self.document.get_line(row)
        indent = _LEADING.match(line).group()
        before, after = line[:column], line[column:]

        if start == end:
            begin = _BEGIN_AT_END.search(before)
            if begin:
                env = begin.group(1)
                inner = indent + INDENT
                body = "\\item " if env in LIST_ENVS else ""
                first = "\n" + inner + body
                tail = ""
                if not after.strip() and self._needs_end(env):
                    tail = "\n" + indent + f"\\end{{{env}}}"
                self.replace(first + tail, start, end)
                self.move_cursor((row + 1, len(inner + body)))
                return

            if _ITEM_ONLY.match(line) and not after.strip():
                # Un \item vacío termina la lista en vez de añadir otro.
                self.replace("", (row, 0), (row, len(line)))
                return

            if _ITEM_START.match(before) and not after.strip():
                self.replace("\n" + indent + "\\item ", start, end)
                return

        self.replace("\n" + indent, start, end)

    def _needs_end(self, env: str) -> bool:
        text = self.text
        return text.count(f"\\begin{{{env}}}") > text.count(f"\\end{{{env}}}")

    # ------------------------------------------------- edición por líneas

    def _selected_rows(self) -> range:
        start, end = sorted(self.selection)
        last = end[0]
        if end[1] == 0 and end[0] > start[0]:
            last -= 1
        return range(start[0], last + 1)

    def _rewrite_rows(self, rows: range, new_lines: list[str]) -> None:
        last = rows[-1]
        end_column = len(self.document.get_line(last))
        self.replace("\n".join(new_lines), (rows[0], 0), (last, end_column))
        self.selection = Selection((rows[0], 0), (last, len(new_lines[-1])))

    def indent_selection(self) -> None:
        rows = self._selected_rows()
        lines = [self.document.get_line(row) for row in rows]
        self._rewrite_rows(rows, [INDENT + line if line.strip() else line for line in lines])

    def dedent_selection(self) -> None:
        rows = self._selected_rows()
        new_lines = []
        for row in rows:
            line = self.document.get_line(row)
            if line.startswith("\t"):
                line = line[1:]
            else:
                strip = min(len(INDENT), len(line) - len(line.lstrip(" ")))
                line = line[strip:]
            new_lines.append(line)
        cursor = self.cursor_location
        single = len(rows) == 1 and self.selection.is_empty
        self._rewrite_rows(rows, new_lines)
        if single:
            self.move_cursor((cursor[0], min(cursor[1], len(new_lines[0]))))

    def toggle_comment(self) -> None:
        rows = self._selected_rows()
        lines = [self.document.get_line(row) for row in rows]
        content = [line for line in lines if line.strip()]
        commented = bool(content) and all(line.lstrip().startswith("%") for line in content)
        new_lines = []
        for line in lines:
            if not line.strip():
                new_lines.append(line)
                continue
            indent = _LEADING.match(line).group()
            rest = line[len(indent) :]
            if commented:
                rest = rest[1:]
                if rest.startswith(" "):
                    rest = rest[1:]
                new_lines.append(indent + rest)
            else:
                new_lines.append(indent + "% " + rest)
        cursor = self.cursor_location
        single = len(rows) == 1 and self.selection.is_empty
        self._rewrite_rows(rows, new_lines)
        if single:
            delta = len(new_lines[0]) - len(lines[0])
            self.move_cursor((cursor[0], max(0, cursor[1] + delta)))

    # --------------------------------------------------------- inserción

    def insert_snippet(
        self,
        snippet: str,
        start: tuple[int, int] | None = None,
        end: tuple[int, int] | None = None,
    ) -> None:
        """Inserta un fragmento y deja el cursor en la marca ``$0``."""
        sel_start, sel_end = sorted(self.selection)
        start = start if start is not None else sel_start
        end = end if end is not None else sel_end
        indent = _LEADING.match(self.document.get_line(start[0])).group()
        text = snippet.replace("\n", "\n" + indent)
        marker = text.find("$0")
        if marker != -1:
            text = text[:marker] + text[marker + 2 :]
        result = self.replace(text, start, end)
        if marker == -1:
            self.move_cursor(result.end_location)
        else:
            head = text[:marker]
            newlines = head.count("\n")
            if newlines:
                location = (start[0] + newlines, len(head) - head.rfind("\n") - 1)
            else:
                location = (start[0], start[1] + len(head))
            self.move_cursor(location)
        self.focus()

    def wrap_selection(self, before: str, after: str) -> None:
        """Rodea la selección (o el cursor) con ``before`` y ``after``."""
        start, end = sorted(self.selection)
        selected = self.get_text_range(start, end)
        self.replace(before + selected + after, start, end)
        if selected:
            lines = (before + selected + after).split("\n")
            last_row = start[0] + len(lines) - 1
            last_col = len(lines[-1]) if len(lines) > 1 else start[1] + len(lines[0])
            self.move_cursor((last_row, last_col))
        else:
            self.move_cursor((start[0], start[1] + len(before)))
        self.focus()

    def goto_line(self, line: int, column: int = 0) -> None:
        """Lleva el cursor a ``line`` (base 0) y centra la vista."""
        line = max(0, min(line, self.document.line_count - 1))
        column = max(0, min(column, len(self.document.get_line(line))))
        self.move_cursor((line, column), center=True)
        self.focus()

    # ---------------------------------------------------- autocompletado

    def _completions_for(self, before: str) -> tuple[list[Completion], int] | None:
        """Sugerencias para el texto previo al cursor y longitud del prefijo."""
        if match := _CTX_ENV.search(before):
            kind, prefix = match.groups()
            names = [name for name in ENVIRONMENTS if name.startswith(prefix)]
            items = [Completion(name, name, "entorno", f"env.{kind}") for name in names]
            return items, len(prefix)

        if match := _CTX_REF.search(before):
            prefix = match.group(1)
            labels = [label for label in find_labels(self.text) if prefix.lower() in label.lower()]
            return [Completion(label, label, "etiqueta", "label") for label in labels], len(prefix)

        if match := _CTX_CITE.search(before):
            prefix = match.group(1)
            directory = self.path.parent if self.path else None
            keys = [key for key in find_cite_keys(self.text, directory) if prefix.lower() in key.lower()]
            return [Completion(key, key, "cita", "cite") for key in keys], len(prefix)

        if match := _CTX_COMMAND.search(before):
            prefix = match.group(1)
            exact = sorted(
                (c for c in COMMANDS if c.name.startswith(prefix)), key=lambda c: c.name != prefix
            )
            loose = [c for c in COMMANDS if c.name.lower().startswith(prefix.lower()) and c not in exact]
            found = exact + loose
            if len(found) == 1 and found[0].name == prefix and found[0].snippet == "\\" + prefix:
                return [], 0
            items = [Completion("\\" + c.name, c.snippet, c.help, "command") for c in found]
            return items, len(prefix) + 1
        return None

    def _update_completion(self) -> None:
        popup = self.completion
        if popup is None or not self._highlight:
            return
        if not self.selection.is_empty or not self.has_focus:
            popup.close()
            return
        row, column = self.cursor_location
        before = self.document.get_line(row)[:column]
        found = self._completions_for(before)
        if not found or not found[0]:
            popup.close()
            self._completion_range = None
            return
        items, prefix_length = found
        self._completion_range = (row, column - prefix_length, column)
        offset = self.cursor_screen_offset
        popup.open(items[:MAX_COMPLETIONS], offset.x - prefix_length, offset.y)
        # Si la edición desplazó la vista, la posición real se conoce tras el refresco.
        self.call_after_refresh(self._place_completion)

    def _place_completion(self) -> None:
        popup = self.completion
        if popup is not None and popup.is_open and self._completion_range is not None:
            _, start_column, end_column = self._completion_range
            offset = self.cursor_screen_offset
            popup.place(offset.x - (end_column - start_column), offset.y)

    def _completion_is_typed(self) -> bool:
        """``True`` si la sugerencia resaltada es justo lo que ya está escrito."""
        popup = self.completion
        if popup is None or popup.current is None or self._completion_range is None:
            return False
        row, start_column, end_column = self._completion_range
        return popup.current.insert == self.document.get_line(row)[start_column:end_column]

    def close_completion(self) -> None:
        if self.completion is not None:
            self.completion.close()

    def accept_completion(self) -> None:
        popup = self.completion
        if popup is None or self._completion_range is None:
            return
        item = popup.current
        popup.close()
        if item is None:
            return
        row, start_column, end_column = self._completion_range
        self._completion_range = None
        line = self.document.get_line(row)
        closes = end_column < len(line) and line[end_column] == "}"

        if item.kind == "env.begin":
            # Sustituye el nombre y la llave de cierre, y añade el cuerpo y el \end.
            end = end_column + (1 if closes else 0)
            args = ENV_ARGS.get(item.insert, "")
            body = ENVIRONMENTS.get(item.insert, "$0").replace("\n", "\n" + INDENT)
            snippet = f"{item.insert}}}{args}\n{INDENT}{body}"
            if self._needs_end_after_insert(item.insert):
                snippet += f"\n\\end{{{item.insert}}}"
            self.insert_snippet(snippet, (row, start_column), (row, end))
        elif item.kind in ("env.end", "label", "cite"):
            self.replace(item.insert, (row, start_column), (row, end_column))
            target = start_column + len(item.insert) + (1 if closes and item.kind != "cite" else 0)
            self.move_cursor((row, target))
        else:
            snippet = item.insert
            if closes and snippet.endswith("{$0}"):
                # El par automático ya puso la llave de cierre.
                end_column += 1
            self.insert_snippet(snippet, (row, start_column), (row, end_column))
            # Encadena sugerencias: \begin{ abre la lista de entornos.
            self.call_later(self._update_completion)

    def _needs_end_after_insert(self, env: str) -> bool:
        text = self.text
        return text.count(f"\\begin{{{env}}}") >= text.count(f"\\end{{{env}}}")

    # ----------------------------------------------------------- eventos

    def _watch_selection(self, previous_selection: Selection, selection: Selection) -> None:
        super()._watch_selection(previous_selection, selection)
        self.post_message(self.CursorMoved(self))
        popup = self.completion
        if popup is not None and popup.is_open and self._completion_range is not None:
            row, start_column, _ = self._completion_range
            cursor_row, cursor_column = selection.end
            if cursor_row != row or cursor_column < start_column or not selection.is_empty:
                popup.close()

    def _on_blur(self, event: events.Blur) -> None:
        self.close_completion()

    def _on_mouse_down(self, event: events.MouseDown) -> None:
        self.close_completion()
