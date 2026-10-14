import pytest

from relay_screen.input import keys
from relay_screen.input.driver import InputDriver
from relay_screen.input.events import KeyEvent, MouseButton, MouseMove, MouseWheel, UnicodeEvent
from tests.conftest import RecordingBackend

CTRL_DOWN = KeyEvent(keys.VK_CONTROL, True)
CTRL_UP = KeyEvent(keys.VK_CONTROL, False)


def driver(**chars):
    backend = RecordingBackend(chars)
    return InputDriver(backend, step_delay=0), backend


def test_left_click_at_position():
    d, b = driver()
    d.click((100, -20))
    assert b.events == [
        MouseMove(100, -20),
        MouseButton("left", True),
        MouseButton("left", False),
    ]


def test_double_click_with_modifier_in_place():
    d, b = driver()
    d.click(None, count=2, modifiers=("ctrl",))
    assert b.events == [
        CTRL_DOWN,
        MouseButton("left", True),
        MouseButton("left", False),
        MouseButton("left", True),
        MouseButton("left", False),
        CTRL_UP,
    ]


def test_click_count_validated():
    d, _ = driver()
    with pytest.raises(ValueError):
        d.click(None, count=4)


def test_scroll_down_and_left():
    d, b = driver()
    d.scroll("down", 3)
    d.scroll("left", 1, position=(5, 5))
    assert b.events == [
        MouseWheel(-120),
        MouseWheel(-120),
        MouseWheel(-120),
        MouseMove(5, 5),
        MouseWheel(-120, horizontal=True),
    ]


@pytest.mark.parametrize(("direction", "amount"), [("sideways", 1), ("up", 0), ("up", 101)])
def test_scroll_validation(direction, amount):
    d, _ = driver()
    with pytest.raises(ValueError):
        d.scroll(direction, amount)


def test_drag_moves_in_steps_and_releases():
    d, b = driver()
    d.drag((0, 0), (120, 60), steps=4)
    events = b.events
    assert events[0] == MouseMove(0, 0)
    assert events[1] == MouseButton("left", True)
    assert [e for e in events if isinstance(e, MouseMove)][1:] == [
        MouseMove(30, 15),
        MouseMove(60, 30),
        MouseMove(90, 45),
        MouseMove(120, 60),
    ]
    assert events[-1] == MouseButton("left", False)


def test_drag_releases_button_on_failure():
    d, b = driver()
    calls = {"n": 0}
    original = b.send

    def flaky(events):
        calls["n"] += 1
        if calls["n"] == 4:
            raise RuntimeError("boom")
        original(events)

    b.send = flaky  # type: ignore[method-assign]
    with pytest.raises(RuntimeError):
        d.drag((0, 0), (10, 10), steps=4)
    assert b.events[-1] == MouseButton("left", False)


def test_key_chord_uses_layout_for_chars():
    d, b = driver(s=(0x53, ()))
    d.key("ctrl+s")
    assert b.events == [CTRL_DOWN, KeyEvent(0x53, True), KeyEvent(0x53, False), CTRL_UP]


def test_key_adds_shift_required_by_layout():
    d, b = driver(**{"?": (0xBF, ("shift",))})
    d.key("?")
    assert b.events == [
        KeyEvent(keys.VK_SHIFT, True),
        KeyEvent(0xBF, True),
        KeyEvent(0xBF, False),
        KeyEvent(keys.VK_SHIFT, False),
    ]


def test_key_falls_back_to_unicode_for_unmapped_char():
    d, b = driver()
    d.key("é")
    assert b.events == [UnicodeEvent(ord("é"), True), UnicodeEvent(ord("é"), False)]


def test_key_repeat_and_extended():
    d, b = driver()
    d.key("Down", repeat=3)
    assert b.events.count(KeyEvent(keys.VK_DOWN, True, extended=True)) == 3
    with pytest.raises(ValueError):
        d.key("Down", repeat=0)


def test_super_alone_is_extended_win_key():
    d, b = driver()
    d.key("super")
    assert b.events == [KeyEvent(keys.VK_LWIN, True, True), KeyEvent(keys.VK_LWIN, False, True)]


def test_hold_key_releases_after_duration():
    d, b = driver()
    d.hold_key("shift", 1.5)
    assert b.events == [KeyEvent(keys.VK_SHIFT, True), KeyEvent(keys.VK_SHIFT, False)]
    assert 1.5 in b.sleeps
    with pytest.raises(ValueError):
        d.hold_key("shift", 301)


def test_type_text_unicode_newlines_and_surrogates():
    d, b = driver()
    d.type_text("Hi\r\n✓😀\t")
    events = b.events
    assert events[:4] == [
        UnicodeEvent(ord("H"), True),
        UnicodeEvent(ord("H"), False),
        UnicodeEvent(ord("i"), True),
        UnicodeEvent(ord("i"), False),
    ]
    assert events[4:6] == [KeyEvent(keys.VK_RETURN, True), KeyEvent(keys.VK_RETURN, False)]
    assert events[6:8] == [UnicodeEvent(0x2713, True), UnicodeEvent(0x2713, False)]
    assert events[8:12] == [
        UnicodeEvent(0xD83D, True),
        UnicodeEvent(0xD83D, False),
        UnicodeEvent(0xDE00, True),
        UnicodeEvent(0xDE00, False),
    ]
    assert events[12:] == [KeyEvent(keys.VK_TAB, True), KeyEvent(keys.VK_TAB, False)]


def test_type_text_sends_one_character_per_call_with_pause():
    backend = RecordingBackend()
    d = InputDriver(backend, step_delay=0, char_delay=0.01)
    d.type_text("abc")
    assert len(backend.batches) == 3
    assert backend.sleeps == [0.01, 0.01, 0.01]
    with pytest.raises(ValueError):
        d.type_text("")
