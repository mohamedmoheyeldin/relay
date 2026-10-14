from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass, field

import pytest

from relay_screen.actions import Computer
from relay_screen.errors import ActionError
from relay_screen.input.driver import InputDriver
from relay_screen.safety.approval import ApprovalGate, Decision, PromptRequest
from relay_screen.scaling import Rect
from relay_screen.state import ScreenState
from relay_screen.uia import MAX_NODES, UiaService
from tests.conftest import GradientGrabber, RecordingBackend


@dataclass
class El:
    role: str
    name: str = ""
    automation_id: str = ""
    enabled: bool = True
    rect: Rect | None = None
    kids: list[El] = field(default_factory=list)
    pats: frozenset[str] = frozenset()
    val: str | None = None
    log: list[str] = field(default_factory=list)

    def children(self) -> Sequence[El]:
        return self.kids

    def patterns(self) -> frozenset[str]:
        return self.pats

    def value(self) -> str | None:
        return self.val

    def invoke(self) -> str:
        self.log.append("invoke")
        return "expanded" if "expand" in self.pats else "invoked"

    def set_value(self, value: str) -> None:
        self.val = value


class Backend:
    def __init__(self, windows: dict[str, list[El]]) -> None:
        self.by_title = windows
        self.calls = 0

    def windows(self, title: str | None) -> Sequence[El]:
        if title is None:
            return self.by_title["foreground"]
        return [w for t, ws in self.by_title.items() if title.lower() in t.lower() for w in ws]

    def run(self, work: Callable[[], object]) -> object:
        self.calls += 1
        return work()


def notepad() -> tuple[El, El, El]:
    save = El(
        "MenuItem", "Save", "SaveItem", rect=Rect(110, 140, 200, 30), pats=frozenset({"invoke"})
    )
    file_menu = El(
        "MenuItem", "File", rect=Rect(10, 40, 60, 30), pats=frozenset({"expand"}), kids=[save]
    )
    editor = El(
        "Document",
        "Text editor",
        "RichEditBox",
        rect=Rect(0, 80, 3440, 1300),
        pats=frozenset({"value"}),
        val="hello",
    )
    window = El(
        "Window",
        "Untitled - Notepad",
        rect=Rect(0, 0, 3440, 1440),
        kids=[El("MenuBar", "Menu", kids=[file_menu]), editor],
    )
    return window, file_menu, save


def geometry(owner_displays):
    return ScreenState(lambda: owner_displays, lambda ds: None).geometry()


def test_tree_lists_elements_with_ids_and_screenshot_bounds(owner_displays):
    window, _, _ = notepad()
    uia = UiaService(Backend({"foreground": [window]}))
    text = uia.tree(geometry(owner_displays))
    lines = text.splitlines()
    assert lines[1].startswith('[e1] Window "Untitled - Notepad"')
    assert '  [e2] MenuBar "Menu"' in text
    file_line = next(line for line in lines if '"File"' in line)
    assert file_line.startswith('    [e3] MenuItem "File" {expand} center (')
    assert "[e5] Document \"Text editor\" #RichEditBox value='hello' {value}" in text
    assert uia.element("e4").name == "Save"


def test_depth_and_filter(owner_displays):
    window, _, _ = notepad()
    uia = UiaService(Backend({"foreground": [window]}))
    shallow = uia.tree(geometry(owner_displays), depth=1)
    assert "File" not in shallow and "Text editor" in shallow
    only = uia.tree(geometry(owner_displays), filter="menuitem")
    assert [line.split("]")[1].split('"')[1] for line in only.splitlines()[1:]] == [
        "File",
        "Save",
    ]
    assert uia.tree(geometry(owner_displays), filter="nothing") == "No elements match 'nothing'."
    with pytest.raises(ActionError, match="depth"):
        uia.tree(geometry(owner_displays), depth=0)


def test_big_trees_are_cut(owner_displays):
    rows = [El("ListItem", f"row {i}", rect=Rect(0, i, 100, 1)) for i in range(2000)]
    uia = UiaService(Backend({"foreground": [El("List", "big", kids=rows)]}))
    text = uia.tree(geometry(owner_displays))
    assert len(text.splitlines()) <= MAX_NODES + 2  # header + elements + note
    assert text.endswith("Narrow it with depth, filter, window or uia_find.")
    assert len(text) < 24_000 + 200


def test_time_limit_stops_the_walk(owner_displays):
    now = [0.0]

    def clock() -> float:
        now[0] += 1.0
        return now[0]

    rows = [El("ListItem", f"row {i}") for i in range(50)]
    uia = UiaService(Backend({"foreground": [El("List", "slow", kids=rows)]}), clock=clock)
    assert "time limit" in uia.tree(geometry(owner_displays))


def test_find_puts_exact_matches_first_and_filters_role(owner_displays):
    window, _, _ = notepad()
    window.kids.append(El("Button", "Save as", rect=Rect(500, 40, 50, 20)))
    uia = UiaService(Backend({"foreground": [window]}))
    text = uia.find(geometry(owner_displays), "save")
    lines = text.splitlines()
    assert lines[0] == "2 match(es), best first:"
    assert 'MenuItem "Save" #SaveItem' in lines[1] and "Save as" in lines[2]
    only = uia.find(geometry(owner_displays), "save", role="button")
    assert "Save as" in only and "MenuItem" not in only
    assert uia.find(geometry(owner_displays), "zzz") == "No element matches 'zzz'."


def test_window_by_title(owner_displays):
    window, _, _ = notepad()
    edge = El("Window", "Relay - Microsoft Edge", rect=Rect(0, 1440, 3440, 1440))
    uia = UiaService(Backend({"foreground": [window], "Relay - Microsoft Edge": [edge]}))
    text = uia.tree(geometry(owner_displays), window="edge")
    assert "Microsoft Edge" in text and "not on the active display" in text
    with pytest.raises(ActionError, match="no open window"):
        uia.tree(geometry(owner_displays), window="photoshop")


def test_unknown_or_unsupported_elements(owner_displays):
    window, _, _ = notepad()
    uia = UiaService(Backend({"foreground": [window]}))
    uia.tree(geometry(owner_displays))
    with pytest.raises(ActionError, match="unknown element"):
        uia.invoke("e99")
    with pytest.raises(ActionError, match="no invoke"):
        uia.invoke("e1")
    with pytest.raises(ActionError, match="no value"):
        uia.set_value("e3", "x")
    uia.element("e3").enabled = False  # type: ignore[misc]
    with pytest.raises(ActionError, match="disabled"):
        uia.invoke("e3")


class Answers:
    def __init__(self, *answers: Decision) -> None:
        self.answers = list(answers)
        self.requests: list[PromptRequest] = []

    def ask(self, request: PromptRequest) -> Decision:
        self.requests.append(request)
        return self.answers.pop(0) if self.answers else "deny"


def test_notepad_file_save_without_vision(owner_displays):
    window, file_menu, save = notepad()
    uia = UiaService(Backend({"foreground": [window]}))
    prompter = Answers("approve", "approve")
    backend = RecordingBackend()
    computer = Computer(
        ScreenState(lambda: owner_displays, lambda ds: None),
        GradientGrabber(),
        InputDriver(backend),
        report_changes=False,
        before_act=[ApprovalGate(prompter).check],
        sleep=lambda s: None,
        uia=uia,
    )
    found = computer.run("uia_find", {"query": "File", "role": "MenuItem"}).text or ""
    file_id = found.splitlines()[1].split("]")[0].strip("[")
    assert computer.run("uia_invoke", {"element": file_id}).text == "Expanded · OK"
    found = computer.run("uia_find", {"query": "Save"}).text or ""
    save_id = found.splitlines()[1].split("]")[0].strip("[")
    assert computer.run("uia_invoke", {"element": save_id}).text == "Invoked · OK"
    assert file_menu.log == ["invoke"] and save.log == ["invoke"]
    assert [r.detail for r in prompter.requests] == [
        'Activate MenuItem "File"',
        'Activate MenuItem "Save"',
    ]
    assert backend.events == []  # no mouse or keyboard input at all


def test_set_value_is_an_act(owner_displays):
    window, _, _ = notepad()
    uia = UiaService(Backend({"foreground": [window]}))
    prompter = Answers("deny")
    computer = Computer(
        ScreenState(lambda: owner_displays, lambda ds: None),
        GradientGrabber(),
        None,
        report_changes=False,
        before_act=[ApprovalGate(prompter).check],
        uia=uia,
    )
    computer.run("uia_tree")
    with pytest.raises(ActionError, match="denied"):
        computer.run("uia_set_value", {"element": "e5", "value": "new text"})
    assert prompter.requests[0].detail == 'Set Document "Text editor" to "new text"'
    assert uia.element("e5").value() == "hello"
