"""Per-process screen state: which display is active and how its screenshots are sized."""

from __future__ import annotations

import threading
from collections.abc import Callable

from relay_screen.displays import Display
from relay_screen.scaling import ImageBudget, ScreenshotGeometry


class DisplayNotFoundError(ValueError):
    pass


class ScreenState:
    """Tracks the active display.

    The active display is pinned the first time it is needed (to the display holding the
    foreground window, else the primary) and stays pinned until `select_display`, so the
    coordinates of a screenshot keep meaning the same thing while Claude acts on it.
    """

    def __init__(
        self,
        list_displays: Callable[[], list[Display]],
        foreground: Callable[[list[Display]], Display | None],
        budget: ImageBudget | None = None,
    ) -> None:
        self._list_displays = list_displays
        self._foreground = foreground
        self.budget = budget or ImageBudget()
        self._active_rect_key: tuple[int, int, int, int] | None = None
        self._lock = threading.Lock()

    def displays(self) -> list[Display]:
        return self._list_displays()

    def _pin(self, display: Display) -> None:
        r = display.rect
        self._active_rect_key = (r.left, r.top, r.width, r.height)

    def active(self) -> Display:
        with self._lock:
            displays = self.displays()
            if not displays:
                raise DisplayNotFoundError("no displays found")
            if self._active_rect_key is not None:
                for d in displays:
                    r = d.rect
                    if (r.left, r.top, r.width, r.height) == self._active_rect_key:
                        return d
                # Layout changed (monitor unplugged or resized): fall through and re-pin.
            chosen = self._foreground(displays) or next(
                (d for d in displays if d.primary), displays[0]
            )
            self._pin(chosen)
            return chosen

    def select(self, display_id: int) -> Display:
        with self._lock:
            displays = self.displays()
            for d in displays:
                if d.id == display_id:
                    self._pin(d)
                    return d
            ids = ", ".join(str(d.id) for d in displays)
            raise DisplayNotFoundError(f"no display {display_id}; available: {ids}")

    def geometry(self, display: Display | None = None) -> ScreenshotGeometry:
        return ScreenshotGeometry.for_display((display or self.active()).rect, self.budget)
