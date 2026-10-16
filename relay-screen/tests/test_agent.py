from __future__ import annotations

import io
import json
from collections.abc import Mapping
from typing import Any

import pytest
from PIL import Image

from relay_screen.actions import ActionResult
from relay_screen.agent.cli import idle_seconds, pick_model
from relay_screen.agent.local import Frame, LocalAgent, ReplyError, parse_reply, to_member
from relay_screen.errors import ActionError
from relay_screen.runtime.store import InstalledFile, InstalledModel
from relay_screen.runtime.supervisor import RuntimeUnavailable
from relay_screen.safety.approval import ApprovalDeniedError
from relay_screen.safety.killswitch import KillSwitchError


def png(width: int = 2000, height: int = 1000) -> bytes:
    out = io.BytesIO()
    Image.new("RGB", (width, height), "white").save(out, "PNG")
    return out.getvalue()


READS = ("screenshot", "uia_tree", "find_text", "list_apps", "list_windows")


class FakeHands:
    def __init__(self, results: Mapping[str, Any] | None = None) -> None:
        self.calls: list[tuple[str, dict[str, Any]]] = []
        self.results = dict(results or {})
        self.shot = png()

    def run(self, name: str, params: Mapping[str, Any] | None = None) -> ActionResult:
        self.calls.append((name, dict(params or {})))
        if name == "screenshot":
            return ActionResult(image=self.shot)
        outcome = self.results.get(name, "OK · screen changed")
        if callable(outcome):
            outcome = outcome(dict(params or {}))
        if isinstance(outcome, Exception):
            raise outcome
        return ActionResult(str(outcome))

    def acts(self) -> list[tuple[str, dict[str, Any]]]:
        return [c for c in self.calls if c[0] not in READS]


class Script:
    """A fake model that answers with the given replies in order."""

    def __init__(self, *replies: Any) -> None:
        self.replies = [r if isinstance(r, str) else json.dumps(r) for r in replies]
        self.seen: list[list[dict[str, Any]]] = []

    def __call__(self, messages: list[dict[str, Any]]) -> str:
        self.seen.append(messages)
        return self.replies.pop(0)

    def prompt(self, i: int) -> str:
        return self.seen[i][1]["content"][0]["text"]


def run(hands: FakeHands, model: Script, **kw: Any):
    events: list[dict[str, Any]] = []
    outcome = LocalAgent(hands, model, on_event=events.append, **kw).run("write hello")
    return outcome, events


# --- replies and coordinates ------------------------------------------------


def test_parse_reply_tolerates_fences_prose_and_coordinate_lists() -> None:
    reply = 'Sure!\n```json\n{"thought": "t", "action": "Click", "coordinate": [10, 20]}\n```'
    assert parse_reply(reply) == {"thought": "t", "action": "click", "x": 10.0, "y": 20.0}
    with pytest.raises(ReplyError, match="no JSON"):
        parse_reply("I would click the button")
    with pytest.raises(ReplyError, match="action"):
        parse_reply('{"thought": "hmm"}')


def test_frame_maps_model_coordinates_to_screenshot_pixels() -> None:
    frame = Frame.from_png(png(3440, 1440))
    assert frame.sent_size == (1280, 544)  # aligned to the 32 px patch grid
    assert frame.to_screen(500, 500, "rel1000") == [1720, 720]
    assert frame.to_screen(1000, 1000, "rel1000") == [3439, 1439]
    assert frame.to_screen(640, 272, "pixels") == [1720, 720]
    assert frame.data_url().startswith("data:image/jpeg;base64,")
    with pytest.raises(ReplyError):
        frame.to_screen("left", 3, "rel1000")


def test_actions_map_onto_toolset_members() -> None:
    frame = Frame.from_png(png())
    assert to_member({"action": "double_click", "x": 100, "y": 900}, frame, "rel1000") == (
        "double_click",
        {"coordinate": [200, 900]},
    )
    assert to_member({"action": "key", "keys": "ctrl+s"}, frame, "rel1000") == (
        "key",
        {"text": "ctrl+s"},
    )
    assert to_member(
        {"action": "launch_app", "name": "browser", "args": "https://a.b"}, frame, "rel1000"
    ) == ("launch_app", {"name": "browser", "args": ["https://a.b"]})
    assert to_member({"action": "wait", "seconds": 60}, frame, "rel1000") == (
        "wait",
        {"duration": 10.0},
    )
    with pytest.raises(ReplyError, match="unknown action"):
        to_member({"action": "teleport"}, frame, "rel1000")


# --- the loop ---------------------------------------------------------------


def test_runs_actions_until_done() -> None:
    hands = FakeHands()
    model = Script(
        {"thought": "focus the editor", "action": "click", "x": 500, "y": 500},
        {"action": "type", "text": "hello"},
        {"action": "done", "summary": "Typed hello."},
    )
    outcome, events = run(hands, model)
    assert outcome.status == "done" and outcome.message == "Typed hello."
    assert hands.acts() == [
        ("left_click", {"coordinate": [1000, 500]}),
        ("type", {"text": "hello"}),
    ]
    assert [e["event"] for e in events if e["event"] != "timing"] == ["step", "step", "done"]
    assert events[0]["thought"] == "focus the editor"
    # Each step sends a fresh screenshot and the history so far.
    image = model.seen[2][1]["content"][1]["image_url"]["url"]
    assert image.startswith("data:image/jpeg;base64,")
    assert 'Step 2: type "hello" -> OK · screen changed' in model.prompt(2)


def test_an_unusable_reply_is_retried_then_gives_up() -> None:
    hands = FakeHands()
    model = Script("click the button", {"action": "done", "summary": "ok"})
    outcome, events = run(hands, model)
    assert outcome.status == "done"
    assert "could not be used" in model.prompt(1)
    assert events[0]["event"] == "bad_reply"

    outcome, _ = run(FakeHands(), Script("no", "no", "no"))
    assert outcome.status == "failed" and "could not be used" in outcome.message


def test_a_failed_action_goes_back_to_the_model() -> None:
    hands = FakeHands({"launch_app": ActionError("No installed app matches 'Notpad'")})
    model = Script(
        {"action": "launch_app", "name": "Notpad"},
        {"action": "launch_app", "name": "Notepad"},
        {"action": "done", "summary": "opened"},
    )
    outcome, _ = run(hands, model)
    assert outcome.status == "done"
    assert "failed: No installed app matches" in model.prompt(1)


@pytest.mark.parametrize(
    "error",
    [
        ApprovalDeniedError("The owner denied this action."),
        KillSwitchError("Stopped by kill switch"),
    ],
)
def test_a_denial_or_the_kill_switch_stops_the_run(error: Exception) -> None:
    hands = FakeHands({"left_click": error})
    outcome, events = run(hands, Script({"action": "click", "x": 1, "y": 1}))
    assert outcome.status == "stopped"
    assert outcome.message == str(error)
    assert events[-1]["event"] == "stopped"


def test_stops_when_the_same_action_keeps_changing_nothing() -> None:
    hands = FakeHands({"left_click": "OK · no visible change"})
    click = {"action": "click", "x": 10, "y": 10}
    outcome, _ = run(hands, Script(click, click, click))
    assert outcome.status == "failed" and "kept changing nothing" in outcome.message
    assert len(hands.acts()) == 3


def test_gives_up_after_max_steps() -> None:
    model = Script(*[{"action": "wait", "seconds": 1}] * 3)
    outcome, _ = run(FakeHands(), model, max_steps=3)
    assert outcome.status == "failed" and "3 steps" in outcome.message


def test_the_model_can_fail_with_a_reason() -> None:
    outcome, _ = run(FakeHands(), Script({"action": "fail", "reason": "sign-in needed"}))
    assert (outcome.status, outcome.message) == ("failed", "sign-in needed")


def test_uses_the_element_list_when_ui_automation_can_read_the_window() -> None:
    tree = (
        'Elements (ids ...):\n[e1] Window "Untitled - Notepad"\n'
        '  [e5] Document "Text editor" {value}'
    )
    hands = FakeHands({"uia_tree": tree, "uia_set_value": "OK · target changed"})
    model = Script(
        {"action": "set_value", "element": "e5", "value": "hello"},
        {"action": "invoke", "element": "e9"},
        {"action": "done", "summary": "ok"},
    )
    outcome, events = run(hands, model)
    assert outcome.status == "done"
    # e9 isn't in the list, so that reply is sent back instead of run.
    assert hands.acts() == [("uia_set_value", {"element": "e5", "value": "hello"})]
    assert any(e["event"] == "bad_reply" and "e9" in e["error"] for e in events)
    assert '[e5] Document "Text editor"' in model.prompt(0)
    assert events[0]["action"] == 'set_value e5 = "hello"'
    with pytest.raises(ReplyError, match="element id"):
        to_member({"action": "invoke", "element": "Save"}, Frame.from_png(png()), "rel1000")


def test_works_without_ui_automation() -> None:
    hands = FakeHands({"uia_tree": ActionError("UI Automation is not available in this server")})
    model = Script({"action": "done", "summary": "ok"})
    outcome, _ = run(hands, model)
    assert outcome.status == "done" and "Element list" not in model.prompt(0)


# --- model choice -----------------------------------------------------------


def installed(*ids: str) -> dict[str, InstalledModel]:
    f = InstalledFile("m.gguf", "u", "s", 1)
    return {i: InstalledModel(i, "Org/R", "abc", "Q4_K_M", f, None) for i in ids}


def test_pick_model_prefers_the_flag_then_settings_then_a_planner() -> None:
    have = installed("ui-venus-1.5-8b", "qwen3-vl-8b-instruct", "mine")
    assert pick_model(have, {}, "mine") == "mine"
    assert pick_model(have, {"agent": {"model_id": "mine"}}, None) == "mine"
    # A describe pick is for looking; a downloaded planner comes first (D-087).
    assert pick_model(have, {"describe": {"model_id": "mine"}}, None) == "qwen3-vl-8b-instruct"
    assert pick_model(installed("mine", "x"), {"describe": {"model_id": "x"}}, None) == "x"
    assert pick_model(installed("holo-3.1-9b", "qwen3-vl-8b-instruct"), {}, None) == "holo-3.1-9b"
    assert pick_model(have, {"agent": {"model_id": "gone"}}, None) == "qwen3-vl-8b-instruct"
    assert pick_model(installed("mine"), {}, None) == "mine"
    with pytest.raises(RuntimeUnavailable, match="not downloaded"):
        pick_model(have, {}, "gone")
    with pytest.raises(RuntimeUnavailable, match="No local model"):
        pick_model({}, {}, None)


def test_idle_unload_time_comes_from_settings() -> None:
    assert idle_seconds({}) == 60
    assert idle_seconds({"idle_unload_secs": 300}) == 300
    assert idle_seconds({"idle_unload_secs": "soon"}) == 60


def test_cli_reports_unavailable_without_a_model(
    tmp_path: Any, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    from relay_screen.agent import cli

    monkeypatch.setenv("RELAY_HOME", str(tmp_path))
    monkeypatch.delenv("APPDATA", raising=False)
    assert cli.main(["open", "notepad", "--json"]) == 2
    event = json.loads(capsys.readouterr().out.strip().splitlines()[-1])
    assert event["event"] == "unavailable" and "No local model" in event["message"]


def test_operator_needs_a_downloaded_model(tmp_path: Any, monkeypatch: pytest.MonkeyPatch) -> None:
    from relay_screen.agent.operator import LocalOperator

    monkeypatch.setenv("RELAY_HOME", str(tmp_path))
    monkeypatch.delenv("APPDATA", raising=False)
    operator = LocalOperator(connect=lambda url: Script())
    started: list[Any] = []
    operator.setup.spawn = started.append  # no real downloads in tests
    try:
        with pytest.raises(RuntimeUnavailable, match="downloading the local model runtime and"):
            operator.run("write hello in notepad", FakeHands(), 5)
        with pytest.raises(RuntimeUnavailable, match="ready once the download finishes"):
            operator.run("write hello in notepad", FakeHands(), 5)
        assert len(started) == 1  # a second task doesn't start a second download
    finally:
        operator.supervisor.stop_all()


def test_setup_downloads_runtime_then_model_and_reports_failures() -> None:
    from relay_screen.agent.operator import Setup
    from relay_screen.runtime.store import Installed

    class FakeStore:
        def __init__(self) -> None:
            self.state = Installed()
            self.calls: list[str] = []
            self.fail = False

        def installed(self) -> Installed:
            return self.state

        def install_runtime(self, tag: Any, cuda: Any) -> None:
            self.calls.append("runtime")
            self.state.runtime = object()  # type: ignore[assignment]

        def install_model(self, spec: Any) -> None:
            self.calls.append(spec.id)
            if self.fail:
                raise OSError("disk full")
            self.state.models[spec.id] = object()  # type: ignore[assignment]

    store = FakeStore()
    events: list[dict[str, Any]] = []
    setup = Setup(store, record=events.append, spawn=lambda work: work())  # type: ignore[arg-type]
    store.fail = True
    assert setup.start().startswith("Relay is downloading")
    assert setup.error == "OSError: disk full" and not setup.running
    store.fail = False
    setup.start()
    assert store.calls == ["runtime", "holo-3.1-9b", "holo-3.1-9b"]
    assert setup.missing() == [] and setup.start() == "ready"
    assert [e["status"] for e in events] == ["started", "failed", "started", "done"]


def test_reads_screen_text_and_clicks_it_when_ui_automation_sees_little() -> None:
    ocr = '2 line(s) of text (OCR 0.20 s):\n"File" center (40, 20) box [0, 0, 1, 1]\n'
    ocr += '"Send message" center (900, 700) box [0, 0, 1, 1]'
    apps = "3 apps installed (...)\nNotepad\nBrave Origin · default browser\nCalculator"
    match = "1 match(es) for 'Send message', best first (OCR 0.20 s):\n"
    match += '"Send message" center (900, 700) box [0, 0, 1, 1]'

    def find_text(params: dict[str, Any]) -> str:
        return match if params.get("text") else ocr

    hands = FakeHands({"find_text": find_text, "list_apps": apps, "uia_tree": "No elements"})
    model = Script({"action": "click_text", "text": "Send message"}, done())
    outcome, _ = run(hands, model)
    assert outcome.status == "done"
    prompt = model.prompt(0)
    assert "Installed apps: Notepad, Brave Origin · default browser, Calculator" in prompt
    assert "Text on the screen (OCR), usable with click_text:\nFile\nSend message" in prompt
    assert ("find_text", {"text": "Send message"}) in hands.calls
    assert hands.acts() == [("left_click", {"coordinate": [900, 700]})]

    hands = FakeHands({"find_text": "No match for 'Nope' (OCR 0.10 s)."})
    outcome, _ = run(hands, Script({"action": "click_text", "text": "Nope"}, done()))
    assert outcome.status == "done" and hands.acts() == []


def done() -> dict[str, Any]:
    return {"action": "done", "summary": "ok"}


# --- open windows (D-078) ---------------------------------------------------

WINDOWS = (
    "3 open window(s), topmost first:\n"
    '"Relay" · relay.exe · Relay itself, never the target · in front\n'
    '"Monster Jobs - Brave" · brave.exe · minimized\n'
    '"notes.txt - Notepad" · notepad.exe'
)


def test_focuses_the_program_the_task_names_before_the_first_step() -> None:
    hands = FakeHands({"list_windows": WINDOWS})
    model = Script({"action": "done", "summary": "ok"})
    LocalAgent(hands, model).run("In Brave, apply to the Monster job")
    assert hands.acts() == [("focus_window", {"window": "brave"})]
    prompt = model.prompt(0)
    assert "Open windows (topmost first):" in prompt
    assert "Relay itself, never the target" in prompt


def test_no_focus_when_no_open_program_is_named() -> None:
    hands = FakeHands({"list_windows": WINDOWS})
    LocalAgent(hands, Script({"action": "done", "summary": "ok"})).run("add 2 and 3 in Calculator")
    assert hands.acts() == []


def test_focus_window_action_maps_to_the_member() -> None:
    frame = Frame.from_png(png())
    assert to_member({"action": "focus_window", "window": " Brave "}, frame, "rel1000") == (
        "focus_window",
        {"window": "Brave"},
    )
    with pytest.raises(ReplyError, match="focus_window needs"):
        to_member({"action": "focus_window"}, frame, "rel1000")


# --- going to web pages on screen (D-079) -----------------------------------


@pytest.mark.parametrize(
    ("task", "url"),
    [
        ("go to https://www.dice.com/ and search for fde jobs", "https://www.dice.com/"),
        ("Go to dice.com and search for FDE jobs", "dice.com"),
        ("visit www.monster.com.", "www.monster.com"),
        ("open notes.txt in Notepad", None),
        ("open Notepad", None),
    ],
)
def test_finds_the_web_address_a_task_names(task: str, url: str | None) -> None:
    from relay_screen.agent.local import _task_url

    assert _task_url(task) == url


def test_opens_the_page_the_task_names_before_the_first_step() -> None:
    hands = FakeHands({"list_windows": WINDOWS})
    model = Script({"action": "done", "summary": "ok"})
    LocalAgent(hands, model).run("go to https://www.dice.com/ and search for fde jobs")
    assert hands.acts() == [("open_url", {"url": "https://www.dice.com/"})]


def test_open_url_action_maps_to_the_member() -> None:
    frame = Frame.from_png(png())
    assert to_member({"action": "open_url", "url": "dice.com"}, frame, "rel1000") == (
        "open_url",
        {"url": "dice.com"},
    )
    with pytest.raises(ReplyError, match="open_url needs"):
        to_member({"action": "go_to"}, frame, "rel1000")


# --- D-084: instant moves, batches and text-only steps ----------------------

RICH_TREE = (
    'Elements (ids ...):\n[e1] Window "Dice"\n'
    + "\n".join(f'  [e{i}] Button "b{i}" {{invoke}}' for i in range(2, 12))
    + '\n  [e20] Edit "Search jobs" {value}'
)


@pytest.mark.parametrize(
    ("task", "plan"),
    [
        ("go to dice.com", [("open_url", {"url": "dice.com"})]),
        (
            "Please open https://www.dice.com/jobs.",
            [("open_url", {"url": "https://www.dice.com/jobs"})],
        ),
        ("switch to Outlook", [("focus_window", {"window": "Outlook"})]),
        (
            "open Notepad",
            [("focus_window", {"window": "Notepad"}), ("launch_app", {"name": "Notepad"})],
        ),
        ("open Notepad and write hello", None),
        ("go to dice.com and search for python jobs", None),
        ("write hello", None),
        ("In Brave, apply to the Monster job", None),
    ],
)
def test_instant_plan_covers_one_move_tasks_only(task: str, plan: Any) -> None:
    from relay_screen.agent.local import instant_plan

    assert instant_plan(task) == plan


def test_instant_tasks_need_no_model() -> None:
    hands = FakeHands({"focus_window": ActionError("no open window matches 'Notepad'")})
    model = Script()
    events: list[dict[str, Any]] = []
    outcome = LocalAgent(hands, model, on_event=events.append).run("open Notepad")
    assert outcome.status == "done" and model.seen == []
    assert hands.acts() == [
        ("focus_window", {"window": "Notepad"}),
        ("launch_app", {"name": "Notepad"}),
    ]
    assert events[-1]["event"] == "done" and "instant_ms" in events[-1]


def test_an_instant_move_that_fails_goes_to_the_model() -> None:
    hands = FakeHands({"open_url": ActionError("no browser")})
    model = Script({"action": "fail", "reason": "no browser"})
    outcome = LocalAgent(hands, model).run("go to dice.com")
    assert outcome.status == "failed" and len(model.seen) == 1


def test_operator_runs_instant_tasks_before_any_download(
    tmp_path: Any, monkeypatch: pytest.MonkeyPatch
) -> None:
    from relay_screen.agent.operator import LocalOperator

    monkeypatch.setenv("RELAY_HOME", str(tmp_path))
    monkeypatch.delenv("APPDATA", raising=False)
    operator = LocalOperator(connect=lambda url: Script())
    started: list[Any] = []
    operator.setup.spawn = started.append
    try:
        model_id, outcome = operator.run("go to dice.com", FakeHands(), 5)
        assert (model_id, outcome.status, started) == ("instant", "done", [])
    finally:
        operator.supervisor.stop_all()


def test_one_reply_can_run_several_actions() -> None:
    hands = FakeHands({"uia_tree": RICH_TREE})
    model = Script(
        {
            "thought": "search",
            "actions": [
                {"action": "set_value", "element": "e20", "value": "python"},
                {"action": "key", "keys": "Return"},
                {"action": "done", "summary": "too early"},
            ],
        },
        {"action": "done", "summary": "Searched."},
    )
    outcome, events = run(hands, model)
    assert outcome.message == "Searched." and len(model.seen) == 2
    assert hands.acts() == [
        ("uia_set_value", {"element": "e20", "value": "python"}),
        ("key", {"text": "Return"}),
    ]
    timing = [e for e in events if e["event"] == "timing"]
    assert timing[0]["actions"] == 2 and timing[0]["screenshot_sent"] is True


def test_a_batch_stops_at_the_first_failure_and_looks() -> None:
    hands = FakeHands({"uia_tree": RICH_TREE, "uia_invoke": ActionError("gone")})
    model = Script(
        {"actions": [{"action": "invoke", "element": "e3"}, {"action": "type", "text": "x"}]},
        {"action": "done", "summary": "ok"},
    )
    run(hands, model)
    assert hands.acts() == [("uia_invoke", {"element": "e3"})]
    assert len(model.seen[1][1]["content"]) == 2  # it sees the screen after the failure


def test_rich_element_lists_skip_the_screenshot() -> None:
    hands = FakeHands({"uia_tree": RICH_TREE})
    model = Script(
        {"action": "invoke", "element": "e3"},
        {"action": "click", "x": 5, "y": 5},
        {"action": "look"},
        {"action": "done", "summary": "ok"},
    )
    outcome, events = run(hands, model)
    assert outcome.status == "done"
    shots = [len(m[1]["content"]) == 2 for m in model.seen]
    assert shots == [True, False, False, True]  # first step, then text only until look
    assert "No screenshot this time" in model.prompt(1)
    assert any(e["event"] == "bad_reply" and "look" in e["error"] for e in events)
    assert hands.acts() == [("uia_invoke", {"element": "e3"})]


def test_a_browser_in_front_is_read_from_its_page() -> None:
    windows = (
        "2 open window(s), topmost first:\n"
        '"Jobs - Dice - Brave" · brave.exe · in front\n'
        '"Relay" · relay.exe · Relay itself, never the target'
    )
    calls: list[tuple[str, dict[str, Any]]] = []

    def find(params: dict[str, Any]) -> str:
        if params.get("role") == "Document":
            return '1 match(es), best first:\n[e40] Document "Dice" center (1720, 700) box [..]'
        return "1 match(es), best first:\n[e41] Edit \"Address and search bar\" value='dice.com'"

    def tree(params: dict[str, Any]) -> str:
        calls.append(("uia_tree", params))
        return 'Elements (ids ...):\n[e40] Document "Dice"\n  [e42] Edit "Search jobs" {value}'

    hands = FakeHands({"list_windows": windows, "uia_find": find, "uia_tree": tree})
    model = Script({"action": "done", "summary": "ok"})
    LocalAgent(hands, model).run("search python jobs on this page")
    prompt = model.prompt(0)
    assert "[e41] Edit \"Address and search bar\" value='dice.com'" in prompt
    assert '[e42] Edit "Search jobs"' in prompt
    assert calls[0][1] == {"root": "e40", "depth": 40, "interactive": True}


def test_a_thinking_block_before_the_json_is_ignored() -> None:
    reply = '<think>The button {maybe} is here</think>{"action": "key", "keys": "Return"}'
    assert parse_reply(reply) == {"action": "key", "keys": "Return"}
