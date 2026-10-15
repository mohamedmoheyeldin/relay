"""Where Relay keeps its runtime, models, lock file and logs."""

from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


def default_home() -> Path:
    """`RELAY_HOME`, else `%LOCALAPPDATA%\\Relay` (D-004), else ~/.local/share/relay."""
    if home := os.environ.get("RELAY_HOME"):
        return Path(home)
    if sys.platform == "win32" and (local := os.environ.get("LOCALAPPDATA")):
        return Path(local) / "Relay"
    return Path.home() / ".local" / "share" / "relay"


def settings_file() -> Path | None:
    """`%APPDATA%\\Relay\\settings.json`, owned by relay-core (D-048); None off Windows."""
    if appdata := os.environ.get("APPDATA"):
        return Path(appdata) / "Relay" / "settings.json"
    return None


def models_folder_setting(path: Path | None) -> Path | None:
    """The owner's models folder from Settings > Local AI (D-046), if they changed it.

    The app stores it as `local_ai.models_dir`; a top-level `models_folder` is read too.
    """
    if path is None or not path.is_file():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        local_ai = data.get("local_ai")
        folder = local_ai.get("models_dir") if isinstance(local_ai, dict) else None
        folder = folder or data.get("models_folder")
    except (OSError, ValueError, AttributeError):
        return None
    return Path(folder) if isinstance(folder, str) and folder else None


def local_ai_settings(path: Path | None) -> dict[str, Any]:
    """The `local_ai` section of settings.json (D-048), or {} if it is missing or unreadable."""
    if path is None or not path.is_file():
        return {}
    try:
        section = json.loads(path.read_text(encoding="utf-8")).get("local_ai")
    except (OSError, ValueError, AttributeError):
        return {}
    return section if isinstance(section, dict) else {}


@dataclass(frozen=True)
class RelayPaths:
    home: Path
    models: Path

    @classmethod
    def default(cls) -> RelayPaths:
        home = default_home()
        return cls(home, models_folder_setting(settings_file()) or home / "models")

    @classmethod
    def at(cls, home: Path) -> RelayPaths:
        return cls(home, home / "models")

    @property
    def runtime(self) -> Path:
        return self.home / "runtime"

    @property
    def lock(self) -> Path:
        """What is installed and the SHA-256 each file was verified against."""
        return self.home / "installed.json"

    @property
    def log(self) -> Path:
        return self.home / "logs" / "runtime.jsonl"
