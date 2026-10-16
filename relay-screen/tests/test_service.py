from __future__ import annotations

import json
import subprocess
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

from relay_screen.actions import ActionResult
from relay_screen.agent.local import Outcome, plain
from relay_screen.agent.service import (
    Awareness,
    OperatorService,
    Write,
    ask_claude,
    hand_off,
    route,
    serve_port,
)
from relay_screen.runtime.supervisor import RuntimeUnavailable


@pytest.mark.parametrize(
    ("text", "to"),
    [
        ("open Notepad", "operator"),
        ("go to dice.com and search for python jobs", "operator"),
        ("In Brave, click the Remote filter", "operator"),
        ("write a cover letter for the Acme job", "claude"),
        ("what is the weather like?", "claude"),
        ("thanks", "claude"),
        ("apply to the first job and write a short note to the recruiter", "claude"),
    ],
)
def test_route_sends_screen_work_to_the_local_ai(text: str, to: str) -> None:
    assert route(text)[0] == to


class Events:
    def __init__(self) -> None:
        self.items: list[dict[str, Any]] = []
        self.lock = threading.Lock()

    def __call__(self, event: dict[str, Any]) -> None:
        with self.lock:
            self.items.append(event)

    def kinds(self) -> list[str]:
        return [e["event"] for e in self.items]


class FakeRunner:
    """Runs until stopped, reading notes, so tests can drive a task in progress."""

    def __init__(self, steps: int = 1) -> None:
        self.steps = steps
        self.seen_notes: list[str] = []
        self.tasks: list[str] = []
        self.started = threading.Event()
        self.go = threading.Event()

    def __call__(
        self,
        task: str,
        on_event: Write,
        inbox: Callable[[], list[str]],
        cancelled: Callable[[], bool],
    ) -> tuple[str, Outcome]:
        self.tasks.append(task)
        self.started.set()
        self.go.wait(5)
        self.seen_notes += inbox()
        if cancelled():
            on_event({"event": "stopped", "message": "Stopped by the owner.", "steps": 0})
            return "m", Outcome("stopped", "Stopped by the owner.")
        on_event({"event": "step", "step": 1, "say": "Opened Notepad", "action": "x"})
        on_event({"event": "done", "message": "Opened it.", "steps": 1})
        return "m", Outcome("done", "Opened it.")


def test_a_task_streams_events_with_its_id() -> None:
    events, runner = Events(), FakeRunner()
    runner.go.set()
    service = OperatorService(runner, None, events)
    thread = service.handle({"op": "task", "id": "t1", "text": "open Notepad"})
    assert thread is not None
    thread.join(5)
    assert events.kinds() == ["accepted", "step", "done", "model"]
    assert all(e["id"] == "t1" for e in events.items)
    assert service.running is None


def test_follow_ups_reach_the_running_task_and_stop_ends_it() -> None:
    events, runner = Events(), FakeRunner()
    service = OperatorService(runner, None, events)
    thread = service.handle({"op": "task", "id": "t1", "text": "search jobs"})
    assert thread is not None and runner.started.wait(5)
    service.handle({"op": "say", "id": "t1", "text": "remote only"})
    busy = Events()
    service.handle({"op": "task", "id": "t2", "text": "other"}, busy)
    assert busy.items[0]["event"] == "failed"  # one task at a time
    service.handle({"op": "stop", "id": "t1"})
    runner.go.set()
    thread.join(5)
    assert runner.seen_notes == ["remote only"]
    assert "stopped" in events.kinds()


def test_a_follow_up_after_a_task_starts_one_that_knows_it() -> None:
    events, runner = Events(), FakeRunner()
    runner.go.set()
    service = OperatorService(runner, None, events)
    first = service.handle({"op": "task", "id": "t1", "text": "open Notepad"})
    assert first is not None
    first.join(5)
    second = service.handle({"op": "say", "id": "t2", "text": "now type hello"})
    assert second is not None
    second.join(5)
    assert runner.tasks[1].startswith("now type hello")
    assert "'open Notepad'" in runner.tasks[1]


def test_route_and_state_answer_at_once() -> None:
    events = Events()
    hands = FakeHands()
    awareness = Awareness(hands, clock=lambda: 1000.0)
    awareness.poll()
    service = OperatorService(FakeRunner(), awareness, events)
    service.handle({"op": "route", "id": "r1", "text": "open Notepad"})
    service.handle({"op": "state", "id": "s1"})
    route_event, state = events.items
    assert route_event == {
        "id": "r1",
        "event": "route",
        "to": "operator",
        "why": route_event["why"],
    }
    assert state["foreground"] == "Jobs - Dice - Brave"
    assert state["browser_url"] == "https://www.dice.com/jobs"
    assert state["recent"] == [{"at": 1000, "window": "Jobs - Dice - Brave"}]


class FakeHands:
    def run(self, name: str, params: Any = None) -> ActionResult:
        if name == "list_windows":
            return ActionResult(
                "3 open window(s), topmost first:\n"
                '"Relay" · relay.exe · Relay itself, never the target\n'
                '"Jobs - Dice - Brave" · brave.exe · in front\n'
                '"notes.txt - Notepad" · notepad.exe'
            )
        if name == "uia_find":
            return ActionResult(
                '1 match(es), best first:\n[e3] Edit "Address and search bar" '
                "value='https://www.dice.com/jobs' {value} center (400, 60)"
            )
        return ActionResult("OK")


def test_ui_task_hands_off_to_the_service_over_localhost(tmp_path: Path) -> None:
    app = Events()
    runner = FakeRunner()
    runner.go.set()
    service = OperatorService(runner, None, app)
    sock = serve_port(service, tmp_path)
    try:
        handed = hand_off(tmp_path, "open Notepad", 10, timeout=5)
    finally:
        service.stopping.set()
        sock.close()
    assert handed is not None
    model, outcome, lines = handed
    assert (model, outcome.status, lines) == ("m", "done", ["Step 1: Opened Notepad"])
    # The app sees Claude's hand-off too.
    assert [e.get("source") for e in app.items] == ["claude"] * 4


def test_no_service_means_no_hand_off(tmp_path: Path) -> None:
    assert hand_off(tmp_path, "open Notepad", 10) is None
    (tmp_path / "operator.json").write_text(json.dumps({"port": 1, "token": "x"}))
    assert hand_off(tmp_path, "open Notepad", 10) is None


def test_hand_off_reports_an_unavailable_model(tmp_path: Path) -> None:
    def runner(
        task: str,
        on_event: Write,
        inbox: Callable[[], list[str]],
        cancelled: Callable[[], bool],
    ) -> tuple[str, Outcome]:
        raise RuntimeUnavailable("Relay is downloading the model")

    service = OperatorService(runner, None, Events())
    sock = serve_port(service, tmp_path)
    try:
        with pytest.raises(RuntimeUnavailable, match="downloading"):
            hand_off(tmp_path, "type hello in Notepad", 10, timeout=5)
    finally:
        service.stopping.set()
        sock.close()


def test_ask_claude_uses_the_cli_without_tools(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("RELAY_CLAUDE", "claude")
    calls: list[dict[str, Any]] = []

    def run(command: list[str], **kw: Any) -> subprocess.CompletedProcess[bytes]:
        calls.append({"command": command, **kw})
        return subprocess.CompletedProcess(command, 0, b"Dear Acme team, ...\n", b"")

    assert ask_claude("What should the cover letter say?", "apply on Dice", run) == (
        "Dear Acme team, ..."
    )
    command = calls[0]["command"]
    assert command[:2] == ["claude", "-p"] and "--strict-mcp-config" in command
    assert "Bash" in command[command.index("--disallowedTools") + 1]
    assert b"apply on Dice" in calls[0]["input"]


def test_steps_read_as_plain_words() -> None:
    elements = 'Elements:\n[e20] Edit "Search jobs" {value}\n[e3] Button "Apply"'
    assert plain(
        {"action": "set_value", "element": "e20", "value": "python"}, "OK", False, elements
    ) == ('Typed "python" into "Search jobs"')
    assert plain({"action": "invoke", "element": "e3"}, "OK", False, elements) == 'Pressed "Apply"'
    assert plain({"action": "open_url", "url": "dice.com"}, "OK", False, "") == "Opened dice.com"
    assert plain({"action": "key", "keys": "Return"}, "failed: no window", True, "") == (
        "Pressed Return (didn't work: no window)"
    )


def test_a_new_task_from_the_app_re_arms_the_kill_switch(tmp_path: Path) -> None:
    rearmed: list[str] = []
    runner = FakeRunner()
    runner.go.set()
    service = OperatorService(runner, None, Events(), lambda: rearmed.append("app"))
    thread = service.handle({"op": "task", "id": "t1", "text": "open Notepad"})
    assert thread is not None
    thread.join(5)
    assert rearmed == ["app"]
    sock = serve_port(service, tmp_path)
    try:
        hand_off(tmp_path, "open Notepad", 10, timeout=5)  # Claude's hand-off: no re-arm
    finally:
        service.stopping.set()
        sock.close()
    assert rearmed == ["app"]
