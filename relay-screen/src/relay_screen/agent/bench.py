"""Measure the local operator's task success rate on a fixed set of desktop tasks (D-068).

Each task is run by `LocalAgent` with nothing but the local model, then checked from
outside the agent: a window title, text in a window's UI Automation tree, or a file's
contents. The model's own "done" does not count. The owner's target is at least 95%.

This opens windows and moves the mouse, so it only runs when the owner asks.
"""

from __future__ import annotations

import json
import os
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import asdict, dataclass, field
from importlib import resources
from pathlib import Path
from typing import Any

from relay_screen.agent.local import Hands, LocalAgent, Model
from relay_screen.runtime.catalog import Coords

TARGET = 0.95
WM_CLOSE = 0x0010


@dataclass(frozen=True)
class BenchTask:
    id: str
    task: str
    checks: list[dict[str, Any]]
    max_steps: int = 20
    cleanup_files: list[str] = field(default_factory=list)


@dataclass
class TaskResult:
    id: str
    run: int
    passed: bool
    status: str
    message: str
    steps: int
    seconds: float
    failed_checks: list[str] = field(default_factory=list)


def load_tasks(path: Path | None = None) -> list[BenchTask]:
    """The task file, by default the one shipped with relay-screen."""
    if path is None:
        text = resources.files("relay_screen.agent").joinpath("bench_tasks.json").read_text("utf-8")
    else:
        text = path.read_text(encoding="utf-8")
    return [
        BenchTask(
            t["id"],
            t["task"],
            list(t["checks"]),
            int(t.get("max_steps", 20)),
            list(t.get("cleanup_files", [])),
        )
        for t in json.loads(text)["tasks"]
    ]


def expand(path: str) -> Path:
    """`~` and `%VARS%` in task file paths, e.g. ~/Downloads/relay-bench-2.txt."""
    return Path(os.path.expandvars(os.path.expanduser(path)))


class Desktop:
    """What the checks and cleanup need to see and close windows."""

    def __init__(
        self,
        titles: Callable[[], Mapping[int, str]],
        close: Callable[[int], None],
        hands: Hands,
    ) -> None:
        self.titles = titles
        self.close = close
        self.hands = hands

    def check(self, check: Mapping[str, Any]) -> str | None:
        """None if the check passes, else what was wrong."""
        if "window_title" in check:
            want = str(check["window_title"]).lower()
            if any(want in t.lower() for t in self.titles().values()):
                return None
            return f"no window titled like {check['window_title']!r}"
        if "uia_text" in check:
            spec = check["uia_text"]
            try:
                tree = self.hands.run(
                    "uia_tree", {"window": spec["window"], "depth": 14, "filter": spec["contains"]}
                ).text
            except Exception as exc:
                return f"could not read {spec['window']!r}: {exc}"
            if tree and str(spec["contains"]).lower() in tree.lower() and "No elements" not in tree:
                return None
            return f"{spec['contains']!r} not found in {spec['window']!r}"
        if "file_contains" in check:
            spec = check["file_contains"]
            path = expand(spec["path"])
            try:
                text = path.read_text(encoding="utf-8-sig", errors="replace")
            except OSError:
                return f"{path} was not created"
            if str(spec["text"]) in text:
                return None
            return f"{path} does not contain {spec['text']!r}"
        return f"unknown check {sorted(check)}"


def windows_desktop(hands: Hands) -> Desktop:
    """The real desktop: top-level window titles, closed with WM_CLOSE (Windows only)."""
    import ctypes
    from ctypes import wintypes

    from relay_screen.apps_windows import WinAppBackend

    backend = WinAppBackend()
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    user32.PostMessageW.argtypes = (wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM)

    def titles() -> dict[int, str]:
        return {w.handle: w.title for w in backend.windows()}

    def close(handle: int) -> None:
        user32.PostMessageW(wintypes.HWND(handle), WM_CLOSE, 0, 0)

    return Desktop(titles, close, hands)


def run_bench(
    tasks: Sequence[BenchTask],
    hands: Hands,
    model: Model,
    desktop: Desktop,
    *,
    coords: Coords = "rel1000",
    repeat: int = 1,
    settle: Callable[[float], None] = time.sleep,
    clock: Callable[[], float] = time.monotonic,
    on_result: Callable[[TaskResult], None] | None = None,
) -> list[TaskResult]:
    results: list[TaskResult] = []
    for n in range(1, repeat + 1):
        for task in tasks:
            before = set(desktop.titles())
            for name in task.cleanup_files:
                expand(name).unlink(missing_ok=True)
            start = clock()
            agent = LocalAgent(hands, model, coords=coords, max_steps=task.max_steps)
            try:
                outcome = agent.run(task.task)
                status, message, steps = outcome.status, outcome.message, len(outcome.steps)
            except Exception as exc:
                status, message, steps = "error", f"{type(exc).__name__}: {exc}", 0
            settle(1.0)
            failed = [why for c in task.checks if (why := desktop.check(c)) is not None]
            result = TaskResult(
                task.id,
                n,
                not failed and status != "stopped",
                status,
                message,
                steps,
                round(clock() - start, 1),
                failed,
            )
            results.append(result)
            if on_result:
                on_result(result)
            for handle in set(desktop.titles()) - before:
                desktop.close(handle)
            for name in task.cleanup_files:
                expand(name).unlink(missing_ok=True)
            if status == "stopped":
                return results  # the owner stopped Relay: don't keep going
            settle(1.0)
    return results


def summary(results: Sequence[TaskResult]) -> dict[str, Any]:
    passed = sum(r.passed for r in results)
    rate = passed / len(results) if results else 0.0
    return {
        "runs": len(results),
        "passed": passed,
        "rate": round(rate, 3),
        "target": TARGET,
        "meets_target": rate >= TARGET,
        "median_seconds": sorted(r.seconds for r in results)[len(results) // 2] if results else 0,
        "results": [asdict(r) for r in results],
    }
