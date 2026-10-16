from __future__ import annotations

import json
import time
import urllib.request
from pathlib import Path
from typing import Any

from relay_screen.actions import ActionResult
from relay_screen.agent.service import OperatorService, serve_port
from relay_screen.diagnostics import cli
from relay_screen.diagnostics import run as diag

from .test_agent import FakeHands
from .test_bench import FakeDesktop
from .test_service import Events, FakeRunner


class FakeLink:
    name = "fake"

    def __init__(self, *events: dict[str, Any], before: Any = None) -> None:
        self.events = list(events)
        self.before = before
        self.tasks: list[tuple[str, bool]] = []
        self.answers: list[str] = []

    def run(
        self,
        task: str,
        on_event: diag.Event,
        *,
        offline: bool,
        max_steps: int,
        answer: str = diag.ANSWER,
    ) -> None:
        self.tasks.append((task, offline))
        self.answers.append(answer)
        if self.before:
            self.before()
        for event in self.events:
            on_event(event)


class FakeClaude:
    def __init__(self, logged_in: bool = True, answer: str = "READY") -> None:
        self.logged_in = logged_in
        self.answer = answer
        self.prompts: list[str] = []

    def status(self) -> dict[str, Any]:
        return {"logged_in": self.logged_in, "version": "2.1"}

    def ask(self, question: str) -> str:
        return self.answer

    def run_task(self, prompt: str, folder: Path, on_line: diag.Event) -> dict[str, Any]:
        self.prompts.append(prompt)
        on_line({"event": "tool_use", "tool": "mcp__relay-screen__ui_task"})
        return {"tools": ["mcp__relay-screen__ui_task"], "result": "DONE", "duration_ms": 900}


class Clock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        self.now += 0.5
        return self.now


def tools(link: Any, hands: FakeHands | None = None, **kw: Any) -> diag.Tools:
    hands = hands or FakeHands({"uia_tree": "Elements:\n[e1] Edit 'Relay diagnostics 1'"})
    desktop = kw.pop("desktop", None) or FakeDesktop(hands, {1: "Relay"})
    return diag.Tools(
        hands=hands,
        desktop=desktop,
        link=link,
        claude=kw.pop("claude", None),
        capture=lambda: b"jpeg",
        clock=Clock(),
        sleep=lambda s: None,
        **kw,
    )


def step(n: int) -> dict[str, Any]:
    return {"event": "step", "step": n, "say": f"Did {n}", "action": "x", "result": "OK"}


def test_an_operator_scenario_is_checked_from_outside_and_recorded(tmp_path: Path) -> None:
    link = FakeLink(
        {"event": "plan", "goal": "type", "app": "notepad", "steps": ["open", "type"]},
        step(1),
        {"event": "timing", "step": 1, "think_ms": 700, "perceive_ms": 40, "act_ms": 90},
        step(2),
        {"event": "timing", "step": 2, "think_ms": 300, "screenshot_sent": True},
        {"event": "done", "message": "Typed it.", "steps": 2},
        {"event": "model", "model": "holo-3.1-9b"},
    )
    t = tools(link)
    seen: list[dict[str, Any]] = []
    s = diag.Scenario(
        "notepad-type",
        "Type",
        "operator",
        "Open Notepad and type: Relay diagnostics 1",
        ({"uia_text": {"window": "Notepad", "contains": "Relay diagnostics 1"}},),
    )
    [result] = diag.run_all([s], t, tmp_path, seen.append)
    assert result.status == "passed" and result.model == "holo-3.1-9b" and result.steps == 2
    assert result.timings["think_ms_total"] == 1000 and result.timings["model_calls"] == 2
    assert result.timings["screenshots_sent"] == 1
    assert result.details["plan"]["app"] == "notepad"
    assert result.screenshots == ["before.jpg", "step-01.jpg", "step-02.jpg", "after.jpg"]
    folder = tmp_path / "01-notepad-type"
    lines = [json.loads(x) for x in (folder / "events.jsonl").read_text().splitlines()]
    assert [x["event"] for x in lines][:2] == ["plan", "step"] and all("t" in x for x in lines)
    assert json.loads((folder / "result.json").read_text())["status"] == "passed"
    assert [e["event"] for e in seen] == ["scenario"] + ["progress"] * 7 + ["result"]


def test_a_failed_check_fails_even_when_the_model_says_done(tmp_path: Path) -> None:
    link = FakeLink({"event": "done", "message": "Done.", "steps": 1})
    s = diag.Scenario("x", "X", "operator", "t", ({"window_title": "Downloads"},))
    [result] = diag.run_all([s], tools(link), tmp_path, lambda e: None)
    assert result.status == "failed"
    assert result.failed_checks == ["no window titled like 'Downloads'"]
    assert result.details["false_done"]


def test_new_windows_and_the_tab_are_closed_afterwards(tmp_path: Path) -> None:
    hands = FakeHands()
    desktop = FakeDesktop(hands, {1: "Relay"})

    def opens() -> None:
        desktop.open[7] = "relay diagnostics 1 - Brave Search"

    link = FakeLink({"event": "done", "message": "ok", "steps": 1}, before=opens)
    s = diag.Scenario("b", "B", "operator", "t", close_tab="relay diagnostics 1")
    diag.run_all([s], tools(link, hands, desktop=desktop), tmp_path, lambda e: None)
    assert ("key", {"text": "ctrl+w"}) in hands.calls
    assert desktop.closed == [7]


def test_a_stop_ends_the_run(tmp_path: Path) -> None:
    link = FakeLink({"event": "stopped", "message": "Ctrl+Alt+Pause pressed", "steps": 0})
    chosen = [diag.Scenario("a", "A", "operator", "t"), diag.Scenario("b", "B", "operator", "t")]
    results = diag.run_all(chosen, tools(link), tmp_path, lambda e: None)
    assert [r.id for r in results] == ["a"] and results[0].status == "failed"


def test_an_unavailable_model_fails_with_the_reason(tmp_path: Path) -> None:
    link = FakeLink({"event": "unavailable", "message": "No local model is downloaded."})
    [result] = diag.run_all(
        [diag.Scenario("a", "A", "operator", "t")], tools(link), tmp_path, lambda e: None
    )
    assert result.status == "failed" and "could not run" in result.failed_checks[0]


def test_the_web_form_is_checked_by_what_the_server_received() -> None:
    form = diag.FormServer()
    try:
        page = urllib.request.urlopen(form.url, timeout=5).read().decode()
        assert "<title>Relay form</title>" in page
        assert form.check({"name": "Relay Test"}) == "the form was never submitted to /sent"
        sent = form.base + "/sent?name=Relay+Test&colour=blue&agree=on"
        assert "Relay form sent" in urllib.request.urlopen(sent, timeout=5).read().decode()
        assert form.check({"name": "relay test", "colour": "blue", "agree": "on"}) is None
        assert "expected" in (form.check({"colour": "red"}) or "")
    finally:
        form.close()


def test_every_display_is_shot_and_the_pointer_read_back(tmp_path: Path) -> None:
    cursor = {"xy": (10, 10)}

    def move(params: dict[str, Any]) -> str:
        cursor["xy"] = tuple(params["coordinate"])
        return "OK"

    hands = FakeHands(
        {"mouse_move": move, "cursor_position": lambda p: "X={}, Y={}".format(*cursor["xy"])}
    )
    selected: list[int] = []
    size = diag._png_size(hands.shot)
    displays = [
        {"id": 0, "name": "A", "screenshot_size": size},
        {"id": 1, "name": "B", "screenshot_size": size},
    ]
    t = tools(
        None, hands, displays=lambda: displays, select_display=selected.append,
        active_display=lambda: 1,
    )  # fmt: skip
    s = diag.Scenario("displays", "Displays", "displays")
    [result] = diag.run_all([s], t, tmp_path, lambda e: None)
    assert result.status == "passed", result.failed_checks
    assert selected == [0, 1, 1]  # each display, then back to the one that was active
    assert cursor["xy"] == (10, 10)  # the pointer goes back where it was
    assert result.screenshots == ["display-0.png", "display-1.png"]


def test_the_window_in_front_is_checked() -> None:
    listing = 'Windows:\n"Downloads" · explorer.exe · in front\n"Untitled - Notepad" · notepad.exe'
    t = tools(None, FakeHands({"list_windows": listing}))
    assert diag.check(t, {"front_window": "downloads"}) is None
    assert diag.check(t, {"front_window": "Notepad"}) == (
        "\"Downloads\" is in front, not a window titled like 'Notepad'"
    )


def test_routing_is_checked(tmp_path: Path) -> None:
    [result] = diag.run_all(
        [diag.Scenario("route", "Route", "route")], tools(None), tmp_path, lambda e: None
    )
    assert result.status == "passed", result.failed_checks
    assert len(result.details["routes"]) == len(diag.ROUTES)


def test_claude_scenarios(tmp_path: Path) -> None:
    claude = FakeClaude()
    chosen = [
        diag.Scenario("claude-text", "Text", "claude-text"),
        diag.Scenario(
            "claude-ui",
            "UI",
            "claude-ui",
            "Open Notepad and type: Claude sent this 1",
            ({"uia_text": {"window": "Notepad", "contains": "Claude sent this 1"}},),
        ),
    ]
    hands = FakeHands({"uia_tree": "Elements:\n[e1] Edit 'Claude sent this 1'"})
    results = diag.run_all(chosen, tools(None, hands, claude=claude), tmp_path, lambda e: None)
    assert [r.status for r in results] == ["passed", "passed"]
    assert "ui_task" in claude.prompts[0] and results[1].timings["claude_ms"] == 900
    out = diag.run_all(chosen, tools(None, claude=FakeClaude(False)), tmp_path, lambda e: None)
    assert [r.status for r in out] == ["skipped", "skipped"]


def test_claude_stream_lines_are_read() -> None:
    found: dict[str, Any] = {"tools": []}
    seen: list[dict[str, Any]] = []
    lines = [
        {"type": "system", "subtype": "init", "model": "m", "mcp_servers": [{"status": "ok"}]},
        {
            "type": "assistant",
            "message": {"content": [{"type": "tool_use", "name": "mcp__relay-screen__ui_task"}]},
        },
        {
            "type": "user",
            "message": {
                "content": [{"type": "tool_result", "content": [{"type": "text", "text": "ok"}]}]
            },
        },
        {"type": "result", "is_error": False, "duration_ms": 5, "num_turns": 2, "result": "DONE"},
    ]
    for n, line in enumerate(lines):
        diag.parse_claude_line(line, found, float(n), seen.append)
    assert found["tools"] == ["mcp__relay-screen__ui_task"] and found["first_tool_s"] == 1.0
    assert found["result"] == "DONE" and found["model"] == "m"
    assert [e["event"] for e in seen] == ["claude_init", "tool_use", "tool_result", "claude_result"]


def test_the_summary_report_and_latest(tmp_path: Path) -> None:
    link = FakeLink({"event": "done", "message": "ok", "steps": 1})
    chosen = [
        diag.Scenario("a", "Alpha", "operator", "t"),
        diag.Scenario("b", "Beta", "operator", "t", ({"window_title": "Nope"},)),
    ]
    folder = tmp_path / "run1"
    results = diag.run_all(chosen, tools(link), folder, lambda e: None)
    summary = diag.summarize(results, {"gpu": "x"}, folder, "now", 12.0)
    assert (summary["passed"], summary["failed"], summary["ok"]) == (1, 1, False)
    assert summary["problems"] == ["Beta: no window titled like 'Nope'"]
    assert summary["overview"]["operator_tasks"] == 2
    diag.write_summary(summary, folder)
    assert diag.latest(tmp_path) == json.loads((folder / "summary.json").read_text())
    page = (folder / "report.html").read_text()
    assert "Alpha" in page and "01-a/step" not in page and "02-b/before.jpg" in page


def test_the_cli_lists_and_reads_the_latest(tmp_path: Path, capsys: Any) -> None:
    assert cli.main(["--list"]) == 0
    assert "web-form" in capsys.readouterr().out
    assert cli.main(["--latest", "--out", str(tmp_path)]) == 2
    assert cli.main(["--only", "nope", "--out", str(tmp_path)]) == 2


def test_the_scenarios_cover_the_owners_ask() -> None:
    chosen = diag.scenarios("1", "http://127.0.0.1:1")
    kinds = {s.kind for s in chosen}
    assert kinds == {"operator", "displays", "route", "claude-text", "claude-ui", "app"}
    assert any(s.offline for s in chosen) and len({s.id for s in chosen}) == len(chosen)
    # One run covers every area, in order: the app, real tasks, Claude, the system.
    areas = [diag.area_of(s) for s in chosen]
    assert list(dict.fromkeys(areas)) == list(diag.AREAS)
    assert diag.area_of(next(s for s in chosen if s.id == "local-only")) == diag.AREAS[2]


def test_the_service_runs_a_diagnostics_task_as_the_owners_own(tmp_path: Path) -> None:
    app = Events()
    granted: list[str | None] = []
    rearmed: list[str] = []
    runner = FakeRunner()
    runner.go.set()
    service = OperatorService(runner, None, app, lambda: rearmed.append("x"), granted.append)
    sock = serve_port(service, tmp_path)
    try:
        link = diag.ServiceLink.find(tmp_path)
        assert link is not None
        seen: list[dict[str, Any]] = []
        link.run("submit the form", seen.append, offline=True, max_steps=5)
    finally:
        service.stopping.set()
        sock.close()
    assert [e["event"] for e in seen] == ["accepted", "step", "done", "model"]
    assert app.items == []  # not shown in the owner's chat
    assert granted == ["submit the form", None] and rearmed == ["x"]


def test_offline_tasks_reach_the_runner(tmp_path: Path) -> None:
    service = OperatorService(FakeRunner(), None, Events())
    service.handle({"op": "task", "id": "t", "text": "x", "offline": True})
    assert service.running is not None and service.running.offline
    service.running.stop.set()


def test_png_size() -> None:
    hands = FakeHands()
    shot = hands.run("screenshot")
    assert isinstance(shot, ActionResult) and diag._png_size(shot.image or b"") is not None
    assert diag._png_size(b"nope") is None


def test_the_job_application_spans_three_pages() -> None:
    form = diag.FormServer()
    try:
        first = urllib.request.urlopen(form.base + "/job", timeout=5).read().decode()
        assert 'action="/job2"' in first
        second = (
            urllib.request.urlopen(
                form.base + "/job2?full_name=Alex+Relay&email=a%40b.c&phone=1", timeout=5
            )
            .read()
            .decode()
        )
        assert 'name="full_name" value="Alex Relay"' in second and "Backend developer" in second
        third = (
            urllib.request.urlopen(form.base + "/job3?full_name=Alex+Relay&role=backend", timeout=5)
            .read()
            .decode()
        )
        assert "I confirm" in third
        assert "never submitted to /applied (it reached /job2, /job3)" in (
            form.check({"confirm": "on"}, "/applied") or ""
        )
        urllib.request.urlopen(
            form.base + "/applied?full_name=Alex+Relay&why=I+like+reliable+tools.&confirm=on",
            timeout=5,
        ).read()
        want = {"full_name": "alex relay", "why": "I like reliable tools", "confirm": "on"}
        assert form.check(want, "/applied") is None
        prices = urllib.request.urlopen(form.base + "/prices", timeout=5).read().decode()
        assert "Nimbus 14" in prices and "$749" in prices
    finally:
        form.close()


def test_setup_interference_answers_and_files(tmp_path: Path) -> None:
    hands = FakeHands({"uia_tree": "Elements:\n[e1] Edit 'x'"})
    desktop = FakeDesktop(hands, {1: "Relay"})
    did: list[str] = []
    link = FakeLink(step(1), step(2), {"event": "done", "message": "ok", "steps": 2})
    t = tools(
        link,
        hands,
        desktop=desktop,
        setup=lambda s: did.append("setup") or None,
        interfere=lambda: did.append("interfere") or "moved",
    )
    leftover = tmp_path / "leftover.txt"
    leftover.write_text("x")
    s = diag.Scenario(
        "i", "I", "operator", "t", setup="x", interfere=True, answer="Downloads",
        cleanup_files=(str(leftover),),
    )  # fmt: skip
    [result] = diag.run_all([s], t, tmp_path, lambda e: None)
    for _ in range(100):
        if "interfere" in did:
            break
        time.sleep(0.01)
    assert did == ["setup", "interfere"] and link.answers == ["Downloads"]
    assert result.details["interfered"] and not leftover.exists()
    skipped = diag.run_all(
        [diag.Scenario("j", "J", "operator", "t", setup="x")],
        tools(link, setup=lambda s: "only one monitor is connected"),
        tmp_path,
        lambda e: None,
    )
    assert skipped[0].status == "skipped" and skipped[0].message == "only one monitor is connected"


def test_resources_are_grouped_by_role_and_checked_against_the_budget(tmp_path: Path) -> None:
    mb = diag.MB
    procs = [
        {"pid": 1, "parent": 0, "name": "explorer.exe", "ram": 50 * mb, "private": 0},
        {"pid": 10, "parent": 1, "name": "relay.exe", "ram": 300 * mb, "private": 200 * mb},
        {"pid": 11, "parent": 10, "name": "msedgewebview2.exe", "ram": 900 * mb, "private": 0},
        {"pid": 12, "parent": 10, "name": "python.exe", "ram": 400 * mb, "private": 0},
        {"pid": 13, "parent": 10, "name": "llama-server.exe", "ram": 600 * mb, "private": 0},
        {"pid": 20, "parent": 1, "name": "msedgewebview2.exe", "ram": 999 * mb, "private": 0},
        {"pid": 30, "parent": 1, "name": "python.exe", "ram": 80 * mb, "private": 0},
    ]
    groups = diag.group_processes(procs, {13: 7000 * mb}, me=30)
    assert groups["app"]["ram_mb"] == 300 and groups["app window (WebView2)"]["ram_mb"] == 900
    assert groups["local AI service"]["processes"] == 1
    assert groups["llama-server"]["vram_mb"] == 7000 and groups["this test"]["ram_mb"] == 80
    assert sum(g["processes"] for g in groups.values()) == 5  # not the other webview
    watch = diag.ResourceWatch(lambda: groups, tmp_path / "r.jsonl")
    watch.take()
    out = watch.end()
    assert out["peak_relay_ram_mb"] == 1600 and out["over_budget"][0].startswith("Relay used 1600")
    summary = diag.summarize([], {}, tmp_path, "now", 1.0, out)
    assert summary["problems"] == out["over_budget"]
    # Another program's Python under the Claude desktop app is not Relay's.
    relay_py = "C:\\Program Files\\Relay\\python\\python.exe"
    other_py = "C:\\Users\\me\\uv\\python.exe"
    mixed = [
        {"pid": 40, "parent": 1, "name": "claude.exe", "ram": 0, "private": 0},
        {"pid": 41, "parent": 40, "name": "python.exe", "ram": 70 * mb, "path": other_py},
        {"pid": 42, "parent": 40, "name": "python.exe", "ram": 60 * mb, "path": relay_py},
    ]
    found = diag.group_processes(mixed, {}, me=99, python_dir="C:\\Program Files\\Relay\\python")
    assert found["relay-screen for Claude"] == {
        "processes": 1, "ram_mb": 60, "private_mb": 0, "vram_mb": 0,
    }  # fmt: skip


def test_an_unanswered_approval_fails_one_scenario_not_the_run(tmp_path: Path) -> None:
    message = "No answer from the owner within 120 s, so this action was not run."
    link = FakeLink({"event": "stopped", "message": message, "steps": 2})
    chosen = [diag.Scenario("a", "A", "operator", "t"), diag.Scenario("b", "B", "operator", "t")]
    results = diag.run_all(chosen, tools(link), tmp_path, lambda e: None)
    assert [r.id for r in results] == ["a", "b"]
    assert all(r.status == "failed" for r in results)


def test_notepad_tabs_from_test_runs_are_closed_without_saving(tmp_path: Path) -> None:
    desktop: Any = None
    front = [""]

    def ctrl_w(params: dict[str, Any]) -> str:
        if params["text"] == "ctrl+w":  # the active tab closes; its title goes
            desktop.open.pop(next(h for h, t in desktop.open.items() if t == front[0]))
        return "OK"

    def focus(params: dict[str, Any]) -> str:
        front[0] = params["window"]
        return "OK"

    hands = FakeHands(
        {
            "key": ctrl_w,
            "focus_window": focus,
            "uia_find": '1 match(es):\n[e9] Button "Don\'t save" center (5, 5)',
        }
    )
    desktop = FakeDesktop(
        hands,
        {
            1: "Relay",
            2: "*First line 010427 - Notepad",
            3: "*Nimbus 14 - $749 - Notepad",
            4: "*my plans - Notepad",
            5: "relay diagnostics 1 - Brave",
        },
    )
    assert diag.close_test_docs(tools(FakeLink(), hands, desktop=desktop)) == 2
    assert sorted(desktop.open) == [1, 4, 5]  # the owner's own tab and other windows stay
    assert hands.calls.count(("uia_invoke", {"element": "e9"})) == 2


def test_library_lists_one_reserve_button_per_matching_copy() -> None:
    from relay_screen.diagnostics.run import site_page

    empty = site_page("/library", {})
    assert "Search the catalogue" in empty and "Reserve" not in empty
    found = site_page("/library", {"q": "dune"})
    assert found.count(">Reserve<") == 3 and 'value="dune-paperback"' in found
    assert "Foundation" not in found
    assert "Reserved: Dune" in site_page("/reserved", {"book": "dune-paperback"})
