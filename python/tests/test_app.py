from pathlib import Path

import pytest

from miyulatex.app import MiyuApp
from miyulatex.screens import ConfirmScreen

DOC = "\\documentclass{article}\n\\begin{document}\n\\section{Uno}\nHola mundo\n\n\\end{document}\n"


@pytest.fixture(autouse=True)
def isolated(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("XDG_CONFIG_HOME", str(tmp_path / "config"))


@pytest.fixture
def project(tmp_path: Path) -> Path:
    directory = tmp_path / "proyecto"
    directory.mkdir()
    (directory / "main.tex").write_text(DOC)
    return directory


async def type_text(pilot, text: str) -> None:
    await pilot.press(*text)
    await pilot.pause()


async def test_opens_main_and_builds_outline(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        assert app.editor is not None and app.editor.path == project / "main.tex"
        assert [item.title for item in app._outline_items] == ["Uno"]
        assert not app.editor.dirty


async def test_pairs_and_backspace(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(4)
        await type_text(pilot, "f(x")
        assert editor.document.get_line(4) == "f(x)"
        await pilot.press(")")
        assert editor.cursor_location == (4, 4)
        await type_text(pilot, " $a")
        assert editor.document.get_line(4) == "f(x) $a$"
        await pilot.press("backspace", "backspace")
        assert editor.document.get_line(4) == "f(x) "
        await type_text(pilot, "\\{")
        assert editor.document.get_line(4) == "f(x) \\{"


async def test_environment_completion(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(4)
        await type_text(pilot, "\\beg")
        assert app.query_one("#completion").is_open
        await pilot.press("tab")
        await pilot.pause()
        await type_text(pilot, "enum")
        await pilot.press("enter")
        await pilot.pause()
        await type_text(pilot, "a")
        await pilot.press("enter")
        await type_text(pilot, "b")
        assert editor.document.lines[4:8] == [
            "\\begin{enumerate}",
            "    \\item a",
            "    \\item b",
            "\\end{enumerate}",
        ]


async def test_label_completion(project: Path):
    (project / "main.tex").write_text(DOC.replace("Hola mundo", "\\label{sec:uno}\\label{eq:dos}"))
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(4)
        await type_text(pilot, "\\ref{eq")
        popup = app.query_one("#completion")
        assert [item.label for item in popup.items] == ["eq:dos"]
        await pilot.press("enter")
        await pilot.pause()
        assert editor.document.get_line(4) == "\\ref{eq:dos}"
        assert editor.cursor_location == (4, 12)


async def test_enter_after_finished_command_is_a_newline(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(4)
        await type_text(pilot, "\\to")
        assert app.query_one("#completion").is_open
        await pilot.press("enter")
        await pilot.pause()
        assert editor.document.get_line(4) == "\\to"
        assert editor.cursor_location == (5, 0)


async def test_comment_and_indent(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(3)
        await pilot.press("ctrl+underscore")
        assert editor.document.get_line(3) == "% Hola mundo"
        await pilot.press("ctrl+underscore")
        assert editor.document.get_line(3) == "Hola mundo"
        editor.select_all()
        await pilot.press("tab")
        assert editor.document.get_line(2) == "    \\section{Uno}"
        await pilot.press("shift+tab")
        assert editor.text == DOC


async def test_find_and_replace_all(project: Path):
    app = MiyuApp(project)
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        await pilot.press("ctrl+f")
        await type_text(pilot, "document")
        assert len(app._matches) == 3
        app.query_one("#replace-input").value = "DOC"
        await pilot.click("#replace-all")
        await pilot.pause()
        assert editor.text.count("DOC") == 3 and "document" not in editor.text
        await pilot.press("escape")
        await pilot.pause()
        assert not app.query_one("#findbar").display
        assert app.focused is editor


async def test_save_and_quit_confirmation(project: Path):
    app = MiyuApp(project)
    app.config.autocompile = False
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        editor = app.editor
        editor.goto_line(4)
        await type_text(pilot, "nuevo")
        assert editor.dirty
        await pilot.press("ctrl+q")
        await pilot.pause(0.2)
        assert isinstance(app.screen, ConfirmScreen)
        await pilot.press("escape")
        await pilot.pause(0.2)
        await pilot.press("ctrl+s")
        await pilot.pause(0.2)
        assert not editor.dirty
        assert "nuevo" in (project / "main.tex").read_text()


async def test_compile_with_fake_engine(project: Path, tmp_path: Path):
    engine = tmp_path / "tectonic"
    engine.write_text(
        "#!/bin/sh\n"
        'echo "warning: main.tex:3: algo raro"\n'
        'echo "error: main.tex:4: Undefined control sequence"\n'
        "exit 1\n"
    )
    engine.chmod(0o755)
    app = MiyuApp(project)
    app.engines = {"tectonic": str(engine)}
    async with app.run_test(size=(140, 40)) as pilot:
        await pilot.pause(0.2)
        await pilot.press("f5")
        for _ in range(50):
            await pilot.pause(0.1)
            if app.last_result is not None:
                break
        result = app.last_result
        assert result is not None and not result.ok
        assert [(p.severity, p.line) for p in result.problems] == [("error", 4), ("warning", 3)]
        assert app.query_one("#panel").display
        problems = app.query_one("#problems")
        problems.focus()
        problems.highlighted = 0
        await pilot.press("enter")
        await pilot.pause(0.2)
        assert app.editor.cursor_location[0] == 3
