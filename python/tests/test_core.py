from pathlib import Path

from miyulatex.compiler import build_command, find_root, parse_output
from miyulatex.highlight import START, State, tokenize, tokenize_line
from miyulatex.outline import count_words, find_cite_keys, find_labels, parse_outline


def names(line: str, state: State = START) -> dict[str, str]:
    spans, _ = tokenize_line(line, state)
    return {line[start:end]: name for start, end, name in spans}


def test_commands_and_arguments():
    found = names(r"\section{Intro} \textbf{hola} % nota")
    assert found[r"\section"] == "section"
    assert found["Intro"] == "title"
    assert found[r"\textbf"] == "command"
    assert found["hola"] == "bold"
    assert found["% nota"] == "comment"


def test_escaped_percent_is_not_a_comment():
    found = names(r"un 50\% de \emph{x}")
    assert "comment" not in found.values()
    assert found[r"\%"] == "special"


def test_inline_math():
    line = r"sea $x^2 + \alpha$ fin"
    spans, state = tokenize_line(line)
    assert state.math is None
    assert (4, 18, "math") in spans
    assert names(line)[r"\alpha"] == "math.command"


def test_math_environment_spans_lines():
    result = tokenize([r"\begin{align*}", r"a &= \frac{1}{2}", r"\end{align*}", "texto"])
    assert (0, 16, "math") in result[1]
    assert not any(name == "math" for _, _, name in result[3])


def test_verbatim_is_not_tokenized():
    result = tokenize([r"\begin{verbatim}", r"\foo $x$ % y", r"\end{verbatim} \bar"])
    assert result[1] == ((0, 12, "verbatim"),)
    assert (15, 19, "command") in result[2]


def test_inline_math_ends_at_paragraph_break():
    _, state = tokenize_line("precio: $5")
    assert state.math == "$"
    _, state = tokenize_line("", state)
    assert state.math is None


def test_outline():
    text = "\\section{Uno}\n% \\section{No}\n\\subsection*{Dos \\emph{bis}}\n\\chapter[c]{Tres}"
    items = parse_outline(text)
    assert [(i.kind, i.title, i.line) for i in items] == [
        ("section", "Uno", 0),
        ("subsection", "Dos bis", 2),
        ("chapter", "Tres", 3),
    ]


def test_labels_and_cite_keys(tmp_path: Path):
    (tmp_path / "refs.bib").write_text("@article{knuth84,\n title={x}}\n@book{ lamport94 ,\n}")
    assert find_labels("\\label{a} % \\label{b}\n\\label{c}\\label{a}") == ["a", "c"]
    assert find_cite_keys("\\bibitem{local}", tmp_path) == ["local", "knuth84", "lamport94"]


def test_word_count_ignores_markup():
    text = "\\documentclass{article}\n\\begin{document}\nHola mundo \\textbf{cruel}. % no cuenta\n\\end{document}"
    assert count_words(text) == 3


def test_parse_tectonic():
    output = (
        "note: Running TeX ...\n"
        "warning: main.tex:12: Overfull \\hbox (3.0pt too wide)\n"
        "error: main.tex:24: Undefined control sequence\n"
        "error: halted on potentially-recoverable error as specified\n"
    )
    problems = parse_output(output, Path("main.tex"))
    assert [(p.severity, p.file, p.line) for p in problems] == [
        ("error", "main.tex", 24),
        ("warning", "main.tex", 12),
    ]


def test_parse_pdflatex():
    output = (
        "./cap/uno.tex:7: Undefined control sequence.\n"
        "l.7 \\foo\n"
        "! Missing $ inserted.\n"
        "<inserted text>\n"
        "l.31 x_1\n"
        "LaTeX Warning: Reference `fig:a' on page 1 undefined on input line 9.\n"
        "Overfull \\hbox (4.2pt too wide) in paragraph at lines 14--15\n"
        "LaTeX Warning: Label(s) may have changed. Rerun to get cross-references right.\n"
    )
    problems = parse_output(output, Path("main.tex"))
    assert [(p.severity, p.file, p.line) for p in problems] == [
        ("error", "./cap/uno.tex", 7),
        ("error", "main.tex", 31),
        ("warning", "main.tex", 9),
        ("info", "main.tex", 14),
    ]


def test_find_root(tmp_path: Path):
    main = tmp_path / "main.tex"
    main.write_text("\\documentclass{article}")
    chapter = tmp_path / "cap" / "uno.tex"
    chapter.parent.mkdir()
    chapter.write_text("\\section{x}")
    assert find_root(main, main.read_text()) == main
    assert find_root(chapter, chapter.read_text()) == main
    other = tmp_path / "otro.tex"
    other.write_text("\\documentclass{book}")
    assert find_root(chapter, "% !TEX root = ../otro.tex\n\\section{x}") == other


def test_build_command():
    assert build_command("tectonic", "/bin/tectonic", Path("/p/a.tex")) == ["/bin/tectonic", "-X", "compile", "a.tex"]
    assert build_command("pdflatex", "pdflatex", Path("/p/a.tex"))[-1] == "a.tex"


def test_parse_real_tectonic_output():
    output = (
        "warning: malo.tex:3: inputenc package ignored with utf8 based engines.\n"
        "error: malo.tex:24: Undefined control sequence\n"
        "error: something bad happened inside XeTeX; its output follows:\n"
        "! Undefined control sequence.\n"
        "l.24 Escribe \\comandomalo\n"
        "error: the XeTeX engine had an unrecoverable error\n"
        "caused by: halted on potentially-recoverable error as specified\n"
        "warning: p.tex:16: Missing character: There is no 5 in font nullfont!\n"
        "warning: could not represent character \"5\" (0x35) in font \"nullfont\"\n"
        "warning: p.tex:38:\n"
        "error: llave.tex: !File ended while scanning use of \\@xdblarg\n"
        "! File ended while scanning use of \\@xdblarg.\n"
    )
    problems = parse_output(output, Path("malo.tex"))
    assert [(p.severity, p.line, p.message) for p in problems] == [
        ("error", 24, "Undefined control sequence"),
        ("error", None, "File ended while scanning use of \\@xdblarg"),
        ("warning", 3, "inputenc package ignored with utf8 based engines."),
        ("info", 16, "Missing character: There is no 5 in font nullfont!"),
    ]


def test_backdrop_puts_the_photo_behind_everything(tmp_path: Path):
    from PIL import Image
    from rich.segment import Segment
    from rich.style import Style
    from textual.strip import Strip

    from miyulatex.backdrop import BAYER, Backdrop, clean_path, dominant_tone, photo_theme

    assert sorted(v for row in BAYER for v in row)[:2] == [0.5 / 64, 1.5 / 64]
    assert clean_path("'/tmp/mi\\ fondo.png' ") == Path("/tmp/mi fondo.png")

    image = tmp_path / "rojo.png"
    Image.new("RGB", (64, 64), (230, 40, 40)).save(image)
    backdrop = Backdrop()
    assert backdrop.load(image) and not backdrop.active  # falta saber el fondo del tema
    backdrop.set_colors((0, 0, 0), ((20, 20, 20),))
    backdrop.configure(intensity=1.0)
    assert backdrop.active

    hue, saturation = backdrop.tone
    assert (hue < 5 or hue > 355) and saturation > 0.7
    assert dominant_tone(Image.new("RGB", (8, 8), "gray")) is None
    assert photo_theme(backdrop.tone, dark=True).dark and not photo_theme(backdrop.tone, dark=False).dark

    row = backdrop.grid(20, 4)[0]
    shade, glyph, empty, fill = row[0]
    assert len(row) == 20 and fill == shade and shade[0] > shade[1] and "\u2800" < glyph <= "\u28ff"
    assert empty.color.triplet[0] > shade[0]  # los puntos destacan sobre la sombra

    base, cursor, other = Style(bgcolor="#000000"), Style(bgcolor="#141414"), Style(bgcolor="#ff00ff")
    strip = Strip([Segment("ab cd", base), Segment("X", other), Segment("      ", cursor)], 12)
    painted = backdrop._paint(strip, row, left=2)
    cells = [(char, segment.style) for segment in painted for char in segment.text]
    assert painted.cell_length == 12
    # El texto se conserva (y el espacio entre palabras no lleva puntos), sobre la sombra de la foto.
    assert "".join(char for char, _ in cells[:6]) == "ab cdX"
    assert cells[0][1].bgcolor.triplet == row[2][0] and cells[2][1].bgcolor.triplet == row[4][0]
    # Otros fondos (cursor, selección, barras) quedan opacos.
    assert cells[5][1] is other
    # Los huecos anchos llevan puntos; en la línea del cursor, sobre su tono algo más claro.
    assert all("\u2800" < char <= "\u28ff" for char, _ in cells[6:])
    assert cells[6][1].bgcolor.triplet[0] == min(255, row[8][3][0] + 20)

    # Modo de píxeles: los huecos no pintan fondo (asoma la imagen de la terminal) y el texto queda opaco.
    backdrop.pixels = True
    holes = backdrop._paint(strip, row, left=2)
    assert "".join(segment.text for segment in holes) == "ab cdX      "
    assert [segment.style.bgcolor is None for segment in holes] == [False, False, False]
    wide = Strip([Segment("ab", base), Segment("      ", base)], 8)
    assert [segment.style.bgcolor is None for segment in backdrop._paint(wide, row, left=0)] == [False, True]
    picture = backdrop.render_pixels(80, 40, 4)
    assert picture.size == (80, 40) and len(set(picture.getdata())) >= 2
    commands = backdrop.kitty_place(10, 5, 8, 8)
    assert commands.startswith("\x1b7\x1b[H\x1b_Ga=T,f=100") and commands.endswith("\x1b\\\x1b8") and "c=10,r=5" in commands
