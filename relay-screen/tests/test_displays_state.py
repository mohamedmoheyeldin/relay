import pytest

from relay_screen.displays import display_at, virtual_desktop
from relay_screen.scaling import Rect
from relay_screen.state import DisplayNotFoundError, ScreenState


def test_order_puts_primary_first_then_left_to_right(owner_displays):
    assert [(d.id, d.name) for d in owner_displays] == [
        (1, "DISPLAY2"),
        (2, "DISPLAY5"),
        (3, "DISPLAY1"),
    ]


def test_virtual_desktop_spans_all(owner_displays):
    assert virtual_desktop(owner_displays) == Rect(-2560, 0, 6000, 2880)


def test_display_at(owner_displays):
    left = display_at(owner_displays, -1, 400)
    below = display_at(owner_displays, 100, 1500)
    assert left is not None and left.name == "DISPLAY5"
    assert below is not None and below.name == "DISPLAY1"
    assert display_at(owner_displays, -1, 0) is None


def make_state(displays, foreground=None):
    current = {"displays": displays}
    state = ScreenState(lambda: current["displays"], lambda ds: foreground)
    return state, current


def test_active_defaults_to_foreground_then_stays_pinned(owner_displays):
    fg = {"d": owner_displays[2]}
    state = ScreenState(lambda: owner_displays, lambda ds: fg["d"])
    assert state.active().name == "DISPLAY1"
    fg["d"] = owner_displays[0]
    assert state.active().name == "DISPLAY1"


def test_active_falls_back_to_primary(owner_displays):
    state, _ = make_state(owner_displays)
    assert state.active().primary


def test_select_display(owner_displays):
    state, _ = make_state(owner_displays)
    assert state.select(2).name == "DISPLAY5"
    assert state.active().name == "DISPLAY5"
    with pytest.raises(DisplayNotFoundError, match="available: 1, 2, 3"):
        state.select(9)


def test_repins_when_layout_changes(owner_displays):
    state, current = make_state(owner_displays)
    state.select(2)
    current["displays"] = [d for d in owner_displays if d.name != "DISPLAY5"]
    assert state.active().primary


def test_geometry_uses_budget(owner_displays):
    state, _ = make_state(owner_displays)
    g = state.geometry()
    assert (g.width, g.height) == (1568, 656)
