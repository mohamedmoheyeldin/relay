"""Low-level input events and the backend interface that delivers them.

The driver turns toolset actions into these events; a backend sends them. Tests use a
recording backend, Windows uses SendInput (sendinput.py). Coordinates are physical
virtual-desktop pixels.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import Literal, Protocol

Button = Literal["left", "right", "middle"]


@dataclass(frozen=True)
class MouseMove:
    x: int
    y: int


@dataclass(frozen=True)
class MouseButton:
    button: Button
    down: bool


@dataclass(frozen=True)
class MouseWheel:
    delta: int  # multiples of 120 (WHEEL_DELTA); positive = up / right
    horizontal: bool = False


@dataclass(frozen=True)
class KeyEvent:
    vk: int
    down: bool
    extended: bool = False


@dataclass(frozen=True)
class UnicodeEvent:
    code_unit: int  # one UTF-16 code unit; astral characters are sent as a surrogate pair
    down: bool


Event = MouseMove | MouseButton | MouseWheel | KeyEvent | UnicodeEvent


class InputBlockedError(RuntimeError):
    """Windows refused the input (often UIPI: the target window runs elevated)."""


class Backend(Protocol):
    def send(self, events: Sequence[Event]) -> None:
        """Deliver events in order. Raises InputBlockedError if Windows rejects them."""
        ...

    def cursor(self) -> tuple[int, int]:
        """Current cursor position in physical virtual-desktop pixels."""
        ...

    def resolve_char(self, char: str) -> tuple[int, tuple[str, ...]] | None:
        """Virtual key and required modifiers for a character in the current layout."""
        ...

    def sleep(self, seconds: float) -> None: ...
