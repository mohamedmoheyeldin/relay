"""Kill switch (D-012): the owner moves the mouse into the top-left corner of the
primary display, or presses Ctrl+Alt+Pause, and Relay stops acting until re-armed.

The primary display's top-left corner is always (0, 0) of the virtual desktop. Relay
refuses to move the mouse into that corner itself, so a cursor found there was put
there by the owner.
"""

from __future__ import annotations

import contextlib
import threading
import time
from collections.abc import Callable, Sequence

from relay_screen.errors import ActionError
from relay_screen.input.events import (
    Backend,
    Event,
    KeyEvent,
    MouseButton,
    MouseMove,
    UnicodeEvent,
)
from relay_screen.scaling import Rect

KILL_ZONE = Rect(0, 0, 8, 8)
POLL_INTERVAL = 0.05  # s between cursor checks
SLEEP_SLICE = 0.05  # s; long waits wake this often to check the switch

STOPPED_TEXT = (
    "Stopped by the owner's kill switch ({reason}). Relay will not act until the "
    "owner re-arms it (the rearm tool asks them). Do not try to work around this."
)


class KillSwitchError(ActionError):
    """The owner stopped Relay."""


class KillZoneError(ActionError):
    """Relay was asked to move the mouse into the kill-switch corner."""


class KillSwitch:
    def __init__(self, zone: Rect = KILL_ZONE) -> None:
        self.zone = zone
        self._tripped = threading.Event()
        self._reason = ""
        self._lock = threading.Lock()

    @property
    def tripped(self) -> bool:
        return self._tripped.is_set()

    @property
    def reason(self) -> str:
        return self._reason

    def trip(self, reason: str) -> None:
        with self._lock:
            if not self._tripped.is_set():
                self._reason = reason
                self._tripped.set()

    def rearm(self) -> None:
        with self._lock:
            self._reason = ""
            self._tripped.clear()

    def check(self) -> None:
        if self._tripped.is_set():
            raise KillSwitchError(STOPPED_TEXT.format(reason=self._reason))

    def in_zone(self, x: int, y: int) -> bool:
        return self.zone.contains(x, y)

    def poll_cursor(self, cursor: Callable[[], tuple[int, int]]) -> None:
        """One watcher step: trip if the cursor sits in the corner."""
        x, y = cursor()
        if self.in_zone(x, y):
            self.trip("mouse moved to the top-left corner")

    def wait(self, seconds: float) -> None:
        """Sleep, but stop early (raising) if the switch trips."""
        self.check()
        if self._tripped.wait(max(0.0, seconds)):
            self.check()

    # -- background watchers (Windows) ----------------------------------------

    def start_cursor_watcher(self, cursor: Callable[[], tuple[int, int]]) -> threading.Thread:
        def loop() -> None:
            while True:
                with contextlib.suppress(OSError):  # e.g. secure desktop active
                    self.poll_cursor(cursor)
                time.sleep(POLL_INTERVAL)

        thread = threading.Thread(target=loop, name="relay-killswitch-cursor", daemon=True)
        thread.start()
        return thread

    def start_hotkey_watcher(self) -> threading.Thread:
        from relay_screen.safety import hotkey

        thread = threading.Thread(
            target=hotkey.run,
            args=(lambda: self.trip("Ctrl+Alt+Pause pressed"),),
            name="relay-killswitch-hotkey",
            daemon=True,
        )
        thread.start()
        return thread


def _is_release(event: Event) -> bool:
    match event:
        case MouseButton(_, down) | KeyEvent(_, down, _) | UnicodeEvent(_, down):
            return not down
        case _:
            return False


class GuardedBackend:
    """Wraps an input backend: checks the switch before every send and during sleeps.

    Releases (key up, button up) always go through so a stop never leaves keys held.
    """

    def __init__(self, inner: Backend, switch: KillSwitch) -> None:
        self.inner = inner
        self.switch = switch

    def send(self, events: Sequence[Event]) -> None:
        if not all(_is_release(e) for e in events):
            self.switch.check()
            for e in events:
                if isinstance(e, MouseMove) and self.switch.in_zone(e.x, e.y):
                    raise KillZoneError(
                        "that point is inside the kill-switch corner (top-left of the "
                        "primary display); Relay never moves the mouse there"
                    )
        self.inner.send(events)

    def cursor(self) -> tuple[int, int]:
        return self.inner.cursor()

    def resolve_char(self, char: str) -> tuple[int, tuple[str, ...]] | None:
        return self.inner.resolve_char(char)

    def sleep(self, seconds: float) -> None:
        self.switch.wait(seconds)
