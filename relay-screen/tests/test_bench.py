from __future__ import annotations

import json
from collections.abc import Mapping
from pathlib import Path
from typing import Any

import pytest

from relay_screen.agent.bench import BenchTask, Desktop, load_tasks, run_bench, summary
from relay_screen.safety.killswitch import KillSwitchError

from .test_agent import FakeHands, Script


class FakeDesktop(Desktop):
    def __init__(self, hands: FakeHands, windows: Mapping[int, str] | None = None) -> None:
        self.open = dict(windows or {1: "Relay"})
        self.closed: list[int] = []
        super().__init__(lambda: dict(self.open), self._close, hands)

    def _close(self, handle: int) -> None:
        self.closed.append(handle)
        self.open.pop(handle, None)


def done() -> dict[str, Any]:
    return {"action": "done", "summary": "ok"}


def test_the_packaged_task_file_loads() -> None:
    tasks = load_tasks()
    assert len(tasks) >= 9 and len({t.id for t in tasks}) == len(tasks)
    assert all(t.checks for t in tasks)


def test_checks_look_from_outside_the_agent(tmp_path: Path) -> None:
    hands = FakeHands({"uia_tree": 'Elements:\n[e3] Text "Display is 408"'})
    desktop = FakeDesktop(hands, {1: "Example Domain - Brave"})
    assert desktop.check({"window_title": "example domain"}) is None
    assert "no window" in str(desktop.check({"window_title": "Alan Turing"}))
    spec = {"window": "Calculator", "contains": "Display is 408"}
    assert desktop.check({"uia_text": spec}) is None
    assert hands.calls[-1] == (
        "uia_tree",
        {"window": "Calculator", "depth": 14, "filter": "Display is 408"},
    )
    assert "not found" in str(desktop.check({"uia_text": {**spec, "contains": "409"}}))
    # Two Notepad windows: the text is looked for in each (D-106).
    trees = {"A - Notepad": "Elements:\n[e1] Document", "B - Notepad": 'Elements:\n[e2] "x 1"'}
    hands.results["uia_tree"] = lambda p: trees.get(p["window"], "No elements")
    desktop = FakeDesktop(hands, {1: "A - Notepad", 2: "B - Notepad"})
    assert desktop.check({"uia_text": {"window": "Notepad", "contains": "x 1"}}) is None
    file = tmp_path / "f.txt"
    assert "not created" in str(desktop.check({"file_contains": {"path": str(file), "text": "x"}}))
    file.write_text("hello x", encoding="utf-8")
    assert desktop.check({"file_contains": {"path": str(file), "text": "x"}}) is None
    assert "unknown check" in str(desktop.check({"pixel": 1}))


def test_a_task_passes_only_when_its_checks_pass(tmp_path: Path) -> None:
    hands = FakeHands()
    desktop = FakeDesktop(hands)
    file = tmp_path / "out.txt"

    def type_then_save(name: str, params: Mapping[str, Any] | None = None) -> Any:
        if name == "type":
            file.write_text("saved", encoding="utf-8")
            desktop.open[7] = "out.txt - Notepad"
        return FakeHands.run(hands, name, params)

    hands.run = type_then_save  # type: ignore[method-assign]
    tasks = [
        BenchTask(
            "save",
            "save it",
            [{"file_contains": {"path": str(file), "text": "saved"}}],
            cleanup_files=[str(file)],
        ),
        BenchTask("claims", "say done", [{"window_title": "Nowhere"}]),
    ]
    model = Script({"action": "type", "text": "saved"}, done(), done())
    results = run_bench(tasks, hands, model, desktop, settle=lambda _: None)
    assert [(r.id, r.passed) for r in results] == [("save", True), ("claims", False)]
    assert results[1].status == "done" and results[1].failed_checks
    assert desktop.closed == [7] and not file.exists()  # windows and files cleaned up
    report = summary(results)
    assert (report["passed"], report["rate"], report["meets_target"]) == (1, 0.5, False)
    json.dumps(report)


def test_repeats_and_stops_when_the_owner_stops_relay() -> None:
    hands = FakeHands()
    tasks = [BenchTask("a", "a", [{"window_title": "Relay"}])]
    results = run_bench(
        tasks, hands, Script(done(), done()), FakeDesktop(hands), repeat=2, settle=lambda _: None
    )
    assert [(r.run, r.passed) for r in results] == [(1, True), (2, True)]
    assert summary(results)["meets_target"]

    hands = FakeHands({"left_click": KillSwitchError("Stopped by kill switch")})
    click = {"action": "click", "x": 1, "y": 1}
    results = run_bench(tasks * 3, hands, Script(click), FakeDesktop(hands), settle=lambda _: None)
    assert len(results) == 1 and results[0].status == "stopped" and not results[0].passed


def test_bench_cli_reports_unknown_tasks_and_missing_models(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    from relay_screen.agent import cli

    monkeypatch.setenv("RELAY_HOME", str(tmp_path))
    monkeypatch.delenv("APPDATA", raising=False)
    assert cli.bench_main(["--only", "nope", "--json"]) == 2
    assert "Unknown task ids: nope" in capsys.readouterr().out
    assert cli.bench_main(["--only", "notepad-type", "--json"]) == 2
    assert "No local model" in capsys.readouterr().out
