import pytest
from PIL import Image

from relay_screen.actions import (
    CHANGE_PATIENCE,
    CHANGE_POLL,
    CHANGE_SETTLE,
    HALT_TEXT,
    MEMBERS,
    ActionError,
    Computer,
)
from relay_screen.input.driver import InputDriver
from relay_screen.input.events import KeyEvent, MouseButton, MouseMove, MouseWheel
from relay_screen.input.keys import VK_CONTROL, VK_LWIN, VK_RETURN
from relay_screen.safety.approval import ActRequest
from relay_screen.scaling import Rect
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber, RecordingBackend


class FlipGrabber(GradientGrabber):
    """Returns black until the backend has sent input, then white inside `changes`."""

    def __init__(self, backend: RecordingBackend, changes: Rect | None) -> None:
        super().__init__()
        self.backend = backend
        self.changes = changes

    def grab(self, rect: Rect) -> Image.Image:
        image = super().grab(rect)
        if self.backend.batches and self.changes is not None:
            area = self.changes.intersect(rect)
            if area is not None:
                box = (area.left - rect.left, area.top - rect.top)
                image.paste((255, 255, 255), (*box, box[0] + area.width, box[1] + area.height))
        return image


def make(owner_displays, *, changes: Rect | None = None, report: bool = False):
    backend = RecordingBackend({"a": (0x41, ()), "A": (0x41, ("shift",))})
    state = ScreenState(lambda: owner_displays, lambda ds: None)
    grabber = FlipGrabber(backend, changes)
    computer = Computer(
        state, grabber, InputDriver(backend), report_changes=report, sleep=lambda s: None
    )
    return computer, backend, grabber


def test_members_match_the_toolset():
    assert set(MEMBERS) == {
        "screenshot",
        "zoom",
        "left_click",
        "right_click",
        "middle_click",
        "double_click",
        "triple_click",
        "left_click_drag",
        "mouse_move",
        "left_mouse_down",
        "left_mouse_up",
        "cursor_position",
        "scroll",
        "type",
        "key",
        "hold_key",
        "wait",
    }


def test_left_click_converts_screenshot_pixels(owner_displays):
    computer, backend, _ = make(owner_displays)
    geometry = computer.state.geometry()
    result = computer.run("left_click", {"coordinate": [784, 328]})
    assert result.text == "OK" and not result.is_error
    assert backend.events[0] == MouseMove(*geometry.to_physical(784, 328))
    assert backend.events[1:] == [MouseButton("left", True), MouseButton("left", False)]


def test_click_with_modifiers_and_count(owner_displays):
    computer, backend, _ = make(owner_displays)
    computer.run("double_click", {"text": "ctrl"})
    assert backend.events == [
        KeyEvent(VK_CONTROL, True),
        MouseButton("left", True),
        MouseButton("left", False),
        MouseButton("left", True),
        MouseButton("left", False),
        KeyEvent(VK_CONTROL, False),
    ]


@pytest.mark.parametrize(
    ("name", "button", "count"),
    [("right_click", "right", 1), ("middle_click", "middle", 1), ("triple_click", "left", 3)],
)
def test_other_clicks(owner_displays, name, button, count):
    computer, backend, _ = make(owner_displays)
    computer.run(name, {"coordinate": [10, 10]})
    downs = [e for e in backend.events if isinstance(e, MouseButton) and e.down]
    assert downs == [MouseButton(button, True)] * count


def test_coordinate_outside_screenshot_is_error(owner_displays):
    computer, backend, _ = make(owner_displays)
    with pytest.raises(ActionError, match="coordinate"):
        computer.run("left_click", {"coordinate": [1568, 10]})
    with pytest.raises(ActionError, match=r"\[x, y\] integers"):
        computer.run("mouse_move", {"coordinate": [1.5, 2]})
    assert backend.events == []


def test_unknown_parameter_and_action(owner_displays):
    computer, _, _ = make(owner_displays)
    with pytest.raises(ActionError, match="does not take repeat"):
        computer.run("type", {"text": "x", "repeat": 2})
    with pytest.raises(ActionError, match="unknown action"):
        computer.run("open_app", {})


def test_drag_scroll_move_and_buttons(owner_displays):
    computer, backend, _ = make(owner_displays)
    geometry = computer.state.geometry()
    computer.run("left_click_drag", {"start_coordinate": [0, 0], "coordinate": [100, 50]})
    assert backend.events[0] == MouseMove(*geometry.to_physical(0, 0))
    assert MouseMove(*geometry.to_physical(100, 50)) in backend.events
    assert backend.events[-1] == MouseButton("left", False)

    backend.batches.clear()
    computer.run("scroll", {"scroll_direction": "down", "scroll_amount": 3})
    assert backend.events == [MouseWheel(-120)] * 3
    with pytest.raises(ActionError, match="scroll_direction"):
        computer.run("scroll", {"scroll_direction": "sideways", "scroll_amount": 1})

    backend.batches.clear()
    computer.run("mouse_move", {"coordinate": [5, 5]})
    computer.run("left_mouse_down")
    computer.run("left_mouse_up")
    assert backend.events[1:] == [MouseButton("left", True), MouseButton("left", False)]


def test_type_key_and_hold(owner_displays):
    computer, backend, _ = make(owner_displays)
    computer.run("type", {"text": "aA"})
    assert backend.events == [
        KeyEvent(0x41, True),
        KeyEvent(0x41, False),
        KeyEvent(0x10, True),
        KeyEvent(0x41, True),
        KeyEvent(0x41, False),
        KeyEvent(0x10, False),
    ]
    backend.batches.clear()
    computer.run("key", {"text": "Return", "repeat": 2})
    assert backend.events == [KeyEvent(VK_RETURN, True), KeyEvent(VK_RETURN, False)] * 2
    backend.batches.clear()
    computer.run("hold_key", {"text": "super", "duration": 0.5})
    assert backend.events == [KeyEvent(VK_LWIN, True, True), KeyEvent(VK_LWIN, False, True)]
    assert 0.5 in backend.sleeps
    with pytest.raises(ActionError, match="unknown key name"):
        computer.run("key", {"text": "hyper"})
    with pytest.raises(ActionError, match="duration"):
        computer.run("wait", {"duration": 301})


def test_cursor_position(owner_displays):
    computer, backend, _ = make(owner_displays)
    geometry = computer.state.geometry()
    backend.position = geometry.to_physical(200, 100)
    assert computer.run("cursor_position").text == "X=200, Y=100"
    backend.position = (100, 2000)  # on the display below
    assert "another display" in (computer.run("cursor_position").text or "")


def test_screenshot_and_zoom_return_png(owner_displays):
    computer, _, _ = make(owner_displays)
    shot = computer.run("screenshot")
    assert shot.image is not None and shot.image.startswith(b"\x89PNG")
    zoomed = computer.run("zoom", {"region": [0, 0, 100, 100]})
    assert zoomed.image is not None
    with pytest.raises(ActionError, match="region"):
        computer.run("zoom", {"region": [0, 0, 100]})


def test_read_actions_work_without_input(owner_displays):
    state = ScreenState(lambda: owner_displays, lambda ds: None)
    computer = Computer(state, GradientGrabber(), None)
    assert computer.run("screenshot").image is not None
    with pytest.raises(ActionError, match="input is not available"):
        computer.run("key", {"text": "a"})


def test_before_act_hooks_run_for_acts_only(owner_displays):
    computer, backend, _ = make(owner_displays)
    seen: list[str] = []

    def gate(request: ActRequest) -> str | None:
        seen.append(request.name)
        if request.name == "type":
            raise ActionError("denied")
        return None

    computer.before_act.append(gate)
    computer.run("screenshot")
    computer.run("key", {"text": "a"})
    with pytest.raises(ActionError, match="denied"):
        computer.run("type", {"text": "a"})
    assert seen == ["key", "type"]
    assert len(backend.batches) == 1


# -- batch -------------------------------------------------------------------


def test_batch_runs_in_order(owner_displays):
    computer, backend, _ = make(owner_displays)
    results = computer.run_batch(
        [
            {"name": "key", "input": {"text": "super"}},
            {"name": "type", "input": {"text": "a"}},
            {"name": "key", "input": {"text": "Return"}},
            {"name": "screenshot"},
        ]
    )
    assert [r.is_error for r in results] == [False] * 4
    assert results[-1].image is not None
    assert backend.events[0] == KeyEvent(VK_LWIN, True, True)
    assert backend.events[-2:] == [KeyEvent(VK_RETURN, True), KeyEvent(VK_RETURN, False)]


def test_batch_stops_at_first_failure(owner_displays):
    computer, backend, _ = make(owner_displays)
    results = computer.run_batch(
        [
            {"name": "key", "input": {"text": "a"}},
            {"name": "left_click", "input": {"coordinate": [9999, 0]}},
            {"name": "type", "input": {"text": "a"}},
            {"name": "key", "input": {"text": "Return"}},
        ]
    )
    assert [r.is_error for r in results] == [False, True, True, True]
    assert "coordinate" in (results[1].text or "")
    assert [r.text for r in results[2:]] == [HALT_TEXT, HALT_TEXT]
    assert len(backend.batches) == 1  # only the first key press was sent


def test_batch_rejects_malformed_entries(owner_displays):
    computer, _, _ = make(owner_displays)
    results = computer.run_batch(["key", {"name": "key", "input": {"text": "a"}}])
    assert results[0].is_error and "'name'" in (results[0].text or "")
    assert results[1].text == HALT_TEXT


def test_batch_reports_unexpected_exceptions(owner_displays):
    computer, backend, _ = make(owner_displays)

    def boom(events):
        raise RuntimeError("blocked by UIPI")

    backend.send = boom  # type: ignore[method-assign]
    results = computer.run_batch(
        [{"name": "key", "input": {"text": "a"}}, {"name": "wait", "input": {"duration": 1}}]
    )
    assert results[0].text == "RuntimeError: blocked by UIPI"
    assert results[1].text == HALT_TEXT


# -- change verdicts (D-018) -------------------------------------------------


def test_click_reports_target_changed(owner_displays):
    computer, _, grabber = make(owner_displays, changes=Rect(1700, 700, 40, 40), report=True)
    x, y = computer.state.geometry().to_screenshot(1720, 720)
    result = computer.run("left_click", {"coordinate": [x, y]})
    assert result.text == "OK · target changed"
    assert grabber.grabs[0].width == 240  # region around the target, before and after


def test_click_reports_no_visible_change(owner_displays):
    computer, _, _ = make(owner_displays, changes=Rect(3000, 1300, 40, 40), report=True)
    result = computer.run("left_click", {"coordinate": [10, 10]})
    assert result.text == "OK · no visible change"


def test_key_reports_screen_changed(owner_displays):
    computer, _, grabber = make(owner_displays, changes=Rect(0, 0, 800, 400), report=True)
    assert computer.run("key", {"text": "super"}).text == "OK · screen changed"
    assert grabber.grabs[0] == computer.state.geometry().display


class LateGrabber(FlipGrabber):
    """Like FlipGrabber, but the change only shows from the `late`-th grab on."""

    def __init__(self, backend: RecordingBackend, changes: Rect, late: int) -> None:
        super().__init__(backend, changes)
        self.late = late

    def grab(self, rect: Rect) -> Image.Image:
        if len(self.grabs) + 1 < self.late:
            return GradientGrabber.grab(self, rect)
        return super().grab(rect)


def make_timed(owner_displays, grabber_for):
    backend = RecordingBackend()
    state = ScreenState(lambda: owner_displays, lambda ds: None)
    grabber = grabber_for(backend)
    sleeps: list[float] = []
    computer = Computer(
        state, grabber, InputDriver(backend), report_changes=True, sleep=sleeps.append
    )
    return computer, grabber, sleeps


def test_slow_animation_still_counts_as_changed(owner_displays):
    # Start menu opens over a few hundred ms: the first look after the act sees nothing yet.
    computer, grabber, sleeps = make_timed(
        owner_displays, lambda b: LateGrabber(b, Rect(0, 0, 800, 400), late=4)
    )
    assert computer.run("key", {"text": "super"}).text == "OK · screen changed"
    assert len(grabber.grabs) == 4  # before, then three looks
    assert sleeps == [CHANGE_SETTLE, CHANGE_POLL, CHANGE_POLL]


def test_no_change_gives_up_after_patience(owner_displays):
    computer, grabber, sleeps = make_timed(owner_displays, lambda b: FlipGrabber(b, None))
    assert computer.run("key", {"text": "super"}).text == "OK · no visible change"
    assert sum(sleeps) <= CHANGE_PATIENCE + 1e-9
    assert sum(sleeps) > CHANGE_PATIENCE - CHANGE_POLL
    assert len(grabber.grabs) == len(sleeps) + 1


def test_verdicts_off_skips_capture(owner_displays):
    computer, _, grabber = make(owner_displays)
    computer.run("key", {"text": "a"})
    assert grabber.grabs == []
