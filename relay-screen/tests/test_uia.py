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
    on: bool | None = None

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

    def toggled(self) -> bool | None:
        return self.on


class Backend:
    def __init__(self, windows: dict[str, list[El]]) -> None:
        self.by_title = windows
        self.calls = 0
        self.at: El | None = None

    def element_at(self, x: int, y: int) -> El | None:
        return self.at

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
    # A text box's contents count: the second line of a Notepad document (D-106).
    assert "Text editor" in uia.tree(geometry(owner_displays), filter="hello")
    with pytest.raises(ActionError, match="depth"):
        uia.tree(geometry(owner_displays), depth=0)


def test_tree_skips_unnamed_layout_and_hidden_elements(owner_displays):
    window, _, _ = notepad()
    icon = El("Button", rect=Rect(300, 40, 20, 20), pats=frozenset({"invoke"}))
    hidden = El("Button", pats=frozenset({"invoke"}))
    layout = El("Pane", rect=Rect(0, 0, 3440, 80), kids=[El("Pane", kids=[icon, hidden])])
    window.kids.insert(0, layout)
    uia = UiaService(Backend({"foreground": [window]}))
    lines = uia.tree(geometry(owner_displays)).splitlines()[1:]
    assert not any(" Pane" in line for line in lines)
    assert sum(" Button" in line for line in lines) == 1
    assert next(line for line in lines if " Button" in line).startswith("  [e")  # hoisted
    filtered = uia.tree(geometry(owner_displays), filter="pane")
    assert filtered.count(" Pane") == 2


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
    text = uia.tree(geometry(owner_displays), window="edge", displays=owner_displays)
    assert "on display 3, not the active one (select_display 3)" in text
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


def test_risky_mode_asks_only_before_irreversible_clicks(owner_displays):
    window, _, _ = notepad()
    backend_uia = Backend({"foreground": [window]})
    send = El("Button", "Send", rect=Rect(100, 100, 80, 30), pats=frozenset({"invoke"}))
    prompter = Answers("approve")
    backend = RecordingBackend()
    computer = Computer(
        ScreenState(lambda: owner_displays, lambda ds: None),
        GradientGrabber(),
        InputDriver(backend),
        report_changes=False,
        before_act=[ApprovalGate(prompter, mode="risky").check],
        sleep=lambda s: None,
        uia=UiaService(backend_uia),
    )
    computer.run("left_click", {"coordinate": [300, 300]})  # UIA sees nothing there
    backend_uia.at = window.children()[0]
    computer.run("left_click", {"coordinate": [300, 300]})
    computer.run("type", {"text": "please send this"})
    assert prompter.requests == []
    backend_uia.at = send
    computer.run("left_click", {"coordinate": [120, 110]})
    assert [r.detail for r in prompter.requests] == ['Left click at (120, 110) on Button "Send"']


# --- D-089: cached snapshots, page roots, interactive listings ----------------


class SnapshotBackend(Backend):
    def __init__(self, windows: dict[str, list[El]], fail: bool = False) -> None:
        super().__init__(windows)
        self.snapshots = 0
        self.fail = fail

    def snapshot(self, roots: Sequence[El]) -> Sequence[El]:
        self.snapshots += 1
        if self.fail:
            raise OSError("the app refused the cache request")
        return roots


@pytest.mark.parametrize("fail", [False, True])
def test_listings_read_through_one_snapshot_or_fall_back(owner_displays, fail: bool):
    window, _, _ = notepad()
    backend = SnapshotBackend({"foreground": [window]}, fail=fail)
    uia = UiaService(backend)
    assert '"Text editor"' in uia.tree(geometry(owner_displays))
    assert "match" in uia.find(geometry(owner_displays), "Save")
    assert backend.snapshots == 2


def test_a_page_lists_from_its_document_and_only_what_can_be_used(owner_displays):
    search = El("Edit", "Search jobs", rect=Rect(100, 300, 400, 40), pats=frozenset({"value"}))
    blurb = El("Text", "Thousands of tech jobs", rect=Rect(100, 200, 600, 40))
    link = El("Hyperlink", "Python Developer", rect=Rect(100, 400, 300, 30))
    page = El("Document", "Dice", rect=Rect(0, 150, 3440, 1200), kids=[blurb, search, link])
    toolbar = El("ToolBar", "Navigation", rect=Rect(0, 0, 3440, 100))
    window = El("Window", "Dice - Brave", rect=Rect(0, 0, 3440, 1440), kids=[toolbar, page])
    uia = UiaService(Backend({"foreground": [window]}))
    found = uia.find(geometry(owner_displays), "document", role="Document")
    doc = found.splitlines()[1].split("]")[0].removeprefix("[")
    listing = uia.tree(geometry(owner_displays), root=doc, depth=40, interactive=True)
    assert '"Search jobs"' in listing and '"Python Developer"' in listing
    assert "Thousands" not in listing and "Navigation" not in listing


def test_checkboxes_show_whether_they_are_ticked(owner_displays):
    """The web-form run skipped "I agree" and said done (D-100)."""
    agree = El(
        "CheckBox", "I agree", rect=Rect(10, 10, 20, 20), pats=frozenset({"toggle"}), on=False
    )
    news = El(
        "CheckBox", "Newsletter", rect=Rect(10, 40, 20, 20), pats=frozenset({"toggle"}), on=True
    )
    form = El("Window", "Form", rect=Rect(0, 0, 400, 300), kids=[agree, news])
    text = UiaService(Backend({"foreground": [form]})).tree(geometry(owner_displays))
    assert 'CheckBox "I agree" unchecked {toggle}' in text
    assert 'CheckBox "Newsletter" checked {toggle}' in text


def test_popups_must_sit_over_the_front_window() -> None:
    from types import SimpleNamespace as Box

    from relay_screen.uia_windows import _overlaps

    front = Box(left=0, top=0, right=800, bottom=600)
    assert _overlaps(Box(left=700, top=500, right=900, bottom=700), front)
    assert not _overlaps(Box(left=0, top=1400, right=3440, bottom=1440), front)
    assert not _overlaps(None, front)
