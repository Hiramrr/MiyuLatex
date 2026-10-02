"""MiyuLaTeX: un editor de LaTeX para la terminal."""

from __future__ import annotations

import argparse
import os
from pathlib import Path

FPS = 120
# Textual lee su tope de refresco al importarse; por defecto son 60.
os.environ.setdefault("TEXTUAL_FPS", str(FPS))

__version__ = "0.1.0"


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="miyu",
        description="MiyuLaTeX: editor de LaTeX para la terminal, con compilación y vista previa.",
    )
    parser.add_argument("ruta", nargs="?", type=Path, help="archivo .tex o carpeta del proyecto")
    parser.add_argument("--version", action="version", version=f"MiyuLaTeX {__version__}")
    args = parser.parse_args()

    from .app import MiyuApp

    MiyuApp(args.ruta).run()
