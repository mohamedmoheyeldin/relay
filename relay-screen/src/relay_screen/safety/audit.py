"""Audit log: one JSON line per action in runs/<run-id>/actions.jsonl, with before
and after thumbnails of the active display for acts. Runs are personal data and
gitignored (D-012).
"""

from __future__ import annotations

import json
import os
import threading
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

from PIL import Image

THUMBNAIL_EDGE = 480  # px, long edge


def default_runs_dir() -> Path:
    return Path(os.environ.get("RELAY_RUNS_DIR") or Path.cwd() / "runs")


def new_run_id() -> str:
    return datetime.now().strftime("%Y%m%d-%H%M%S") + f"-{os.getpid()}"


@dataclass
class AuditLog:
    directory: Path
    _seq: int = field(default=0, init=False)
    _lock: threading.Lock = field(default_factory=threading.Lock, init=False)

    @classmethod
    def create(cls, runs_dir: Path | None = None, run_id: str | None = None) -> AuditLog:
        directory = (runs_dir or default_runs_dir()) / (run_id or new_run_id())
        directory.mkdir(parents=True, exist_ok=True)
        return cls(directory)

    @property
    def path(self) -> Path:
        return self.directory / "actions.jsonl"

    def next_seq(self) -> int:
        with self._lock:
            self._seq += 1
            return self._seq

    def save_thumbnail(self, seq: int, label: str, image: Image.Image) -> str:
        thumb = image.copy()
        thumb.thumbnail((THUMBNAIL_EDGE, THUMBNAIL_EDGE))
        name = f"{seq:05d}-{label}.png"
        thumb.save(self.directory / name, format="PNG")
        return name

    def record(self, entry: dict[str, Any]) -> None:
        line = {"time": datetime.now(UTC).isoformat(timespec="milliseconds"), **entry}
        with self._lock, self.path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line, ensure_ascii=False, default=str) + "\n")


def elapsed_ms(start: float) -> int:
    return round((time.monotonic() - start) * 1000)
