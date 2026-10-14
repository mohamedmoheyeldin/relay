"""High-level mouse and keyboard actions built from low-level events.

Coordinates here are physical virtual-desktop pixels; the computer-toolset layer converts
from screenshot pixels before calling in (scaling.py).
"""

from __future__ import annotations

from collections.abc import Sequence

from relay_screen.input.events import (
    Backend,
    Button,
    Event,
    KeyEvent,
    MouseButton,
    MouseMove,
    MouseWheel,
    UnicodeEvent,
)
from relay_screen.input.keys import (
    MODIFIER_VKS,
    VK_RETURN,
    VK_TAB,
    Chord,
    Key,
    KeyNameError,
    parse_chord,
)

WHEEL_DELTA = 120
TYPE_CHUNK = 32  # characters per SendInput call when typing
MAX_REPEAT = 100
MAX_DURATION = 300.0

ScrollDirection = str  # "up" | "down" | "left" | "right"


def _modifier_events(modifiers: Sequence[str], down: bool) -> list[Event]:
    order = modifiers if down else list(reversed(modifiers))
    return [KeyEvent(MODIFIER_VKS[m], down, extended=m == "super") for m in order]


class InputDriver:
    def __init__(self, backend: Backend, step_delay: float = 0.02) -> None:
        self.backend = backend
        self.step_delay = step_delay

    # -- mouse -----------------------------------------------------------------

    def move(self, x: int, y: int) -> None:
        self.backend.send([MouseMove(x, y)])

    def mouse_down(self, button: Button = "left") -> None:
        self.backend.send([MouseButton(button, down=True)])

    def mouse_up(self, button: Button = "left") -> None:
        self.backend.send([MouseButton(button, down=False)])

    def click(
        self,
        position: tuple[int, int] | None,
        button: Button = "left",
        count: int = 1,
        modifiers: Sequence[str] = (),
    ) -> None:
        if count not in (1, 2, 3):
            raise ValueError("count must be 1, 2 or 3")
        if position is not None:
            self.move(*position)
            self.backend.sleep(self.step_delay)
        events: list[Event] = _modifier_events(modifiers, down=True)
        for _ in range(count):
            events += [MouseButton(button, True), MouseButton(button, False)]
        events += _modifier_events(modifiers, down=False)
        self.backend.send(events)

    def scroll(
        self,
        direction: ScrollDirection,
        amount: int,
        position: tuple[int, int] | None = None,
        modifiers: Sequence[str] = (),
    ) -> None:
        if direction not in ("up", "down", "left", "right"):
            raise ValueError("scroll_direction must be up, down, left or right")
        if not 1 <= amount <= MAX_REPEAT:
            raise ValueError(f"scroll_amount must be between 1 and {MAX_REPEAT}")
        if position is not None:
            self.move(*position)
            self.backend.sleep(self.step_delay)
        horizontal = direction in ("left", "right")
        sign = 1 if direction in ("up", "right") else -1
        events: list[Event] = _modifier_events(modifiers, down=True)
        events += [MouseWheel(sign * WHEEL_DELTA, horizontal) for _ in range(amount)]
        events += _modifier_events(modifiers, down=False)
        self.backend.send(events)

    def drag(
        self,
        start: tuple[int, int],
        end: tuple[int, int],
        modifiers: Sequence[str] = (),
        steps: int = 12,
    ) -> None:
        """Press at start, glide to end in small steps (apps need intermediate moves), release."""
        self.move(*start)
        self.backend.sleep(self.step_delay)
        pressed: list[Event] = [*_modifier_events(modifiers, down=True), MouseButton("left", True)]
        self.backend.send(pressed)
        try:
            for i in range(1, steps + 1):
                x = round(start[0] + (end[0] - start[0]) * i / steps)
                y = round(start[1] + (end[1] - start[1]) * i / steps)
                self.backend.send([MouseMove(x, y)])
                self.backend.sleep(self.step_delay)
        finally:
            released: list[Event] = [
                MouseButton("left", False),
                *_modifier_events(modifiers, down=False),
            ]
            self.backend.send(released)

    # -- keyboard --------------------------------------------------------------

    def _key_events(self, key: Key, down: bool) -> tuple[list[Event], tuple[str, ...]]:
        """Events for one key plus any modifiers the layout needs (e.g. shift for '?')."""
        if key.vk is not None:
            return [KeyEvent(key.vk, down, key.extended)], ()
        assert key.char is not None
        resolved = self.backend.resolve_char(key.char)
        if resolved is None:
            return [UnicodeEvent(ord(key.char), down)], ()
        vk, needed = resolved
        return [KeyEvent(vk, down)], needed

    def _chord_events(self, chord: Chord, down: bool) -> list[Event]:
        if chord.key is None:
            return _modifier_events(chord.modifiers, down)
        key_events, needed = self._key_events(chord.key, down)
        modifiers = list(chord.modifiers) + [m for m in needed if m not in chord.modifiers]
        if down:
            return [*_modifier_events(modifiers, True), *key_events]
        return [*key_events, *_modifier_events(modifiers, False)]

    def key(self, text: str, repeat: int = 1) -> None:
        if not 1 <= repeat <= MAX_REPEAT:
            raise ValueError(f"repeat must be between 1 and {MAX_REPEAT}")
        chord = parse_chord(text)
        for i in range(repeat):
            self.backend.send([*self._chord_events(chord, True), *self._chord_events(chord, False)])
            if i < repeat - 1:
                self.backend.sleep(self.step_delay)

    def hold_key(self, text: str, duration: float) -> None:
        if not 0 < duration <= MAX_DURATION:
            raise ValueError(f"duration must be between 0 and {MAX_DURATION} seconds")
        chord = parse_chord(text)
        self.backend.send(self._chord_events(chord, True))
        try:
            self.backend.sleep(duration)
        finally:
            self.backend.send(self._chord_events(chord, False))

    def type_text(self, text: str) -> None:
        """Type text layout-independently with Unicode events; newlines and tabs as keys."""
        if not text:
            raise ValueError("text must not be empty")
        events: list[Event] = []
        normalized = text.replace("\r\n", "\n").replace("\r", "\n")
        for count, char in enumerate(normalized, start=1):
            if char == "\n":
                events += [KeyEvent(VK_RETURN, True), KeyEvent(VK_RETURN, False)]
            elif char == "\t":
                events += [KeyEvent(VK_TAB, True), KeyEvent(VK_TAB, False)]
            else:
                for unit in _utf16_units(char):
                    events += [UnicodeEvent(unit, True), UnicodeEvent(unit, False)]
            if count % TYPE_CHUNK == 0:
                self.backend.send(events)
                events = []
                self.backend.sleep(self.step_delay)
        if events:
            self.backend.send(events)


def _utf16_units(char: str) -> list[int]:
    data = char.encode("utf-16-le")
    return [int.from_bytes(data[i : i + 2], "little") for i in range(0, len(data), 2)]


__all__ = ["InputDriver", "KeyNameError"]
