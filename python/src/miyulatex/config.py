"""Preferencias persistentes en ``~/.config/miyulatex/config.json``."""

from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass, fields
from pathlib import Path

from .theme import DEFAULT_THEME


def config_path() -> Path:
    base = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(base) / "miyulatex" / "config.json"


@dataclass
class Config:
    theme: str = DEFAULT_THEME
    engine: str = "auto"
    autocompile: bool = True
    show_sidebar: bool = True
    show_preview: bool = True
    soft_wrap: bool = True
    invert_preview: bool = False
    background: str = ""
    background_style: str = "dither"
    background_intensity: float = 0.7
    background_palette: bool = True
    background_pixels: bool = False

    @classmethod
    def load(cls) -> "Config":
        try:
            raw = json.loads(config_path().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return cls()
        if not isinstance(raw, dict):
            return cls()
        known = {f.name: f.type for f in fields(cls)}
        defaults = cls()
        values = {
            key: value
            for key, value in raw.items()
            if key in known and isinstance(value, type(getattr(defaults, key)))
        }
        return cls(**values)

    def save(self) -> None:
        path = config_path()
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(asdict(self), indent=2) + "\n", encoding="utf-8")
        except OSError:
            pass
