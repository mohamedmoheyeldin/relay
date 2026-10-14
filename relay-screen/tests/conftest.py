from __future__ import annotations

from collections.abc import Sequence

import pytest
from PIL import Image

from relay_screen.displays import Display, order_displays
from relay_screen.input.events import Event
from relay_screen.scaling import Rect

# The owner's layout (docs/setup.md): primary ultrawide, a second one below it, and a
# 2560x720 strip on the iGPU placed to the left (negative x) for coverage.
OWNER_LAYOUT = [
    Display(0, "DISPLAY2", Rect(0, 0, 3440, 1440), True, 1.0, handle=11),
    Display(0, "DISPLAY1", Rect(0, 1440, 3440, 1440), False, 1.0, handle=22),
    Display(0, "DISPLAY5", Rect(-2560, 360, 2560, 720), False, 1.0, handle=33),
]


@pytest.fixture
def owner_displays() -> list[Display]:
    return order_displays(OWNER_LAYOUT)


class GradientGrabber:
    """Fake capture that records every rectangle it was asked to grab."""

    def __init__(self) -> None:
        self.grabs: list[Rect] = []

    def grab(self, rect: Rect) -> Image.Image:
        self.grabs.append(rect)
        image = Image.new("RGB", (rect.width, rect.height))
        image.putpixel((0, 0), ((rect.left // 16) % 256, (rect.top // 16) % 256, 7))
        return image


class RecordingBackend:
    def __init__(self, chars: dict[str, tuple[int, tuple[str, ...]]] | None = None) -> None:
        self.batches: list[list[Event]] = []
        self.sleeps: list[float] = []
        self.chars = chars or {}
        self.position = (0, 0)

    @property
    def events(self) -> list[Event]:
        return [e for batch in self.batches for e in batch]

    def send(self, events: Sequence[Event]) -> None:
        self.batches.append(list(events))

    def cursor(self) -> tuple[int, int]:
        return self.position

    def resolve_char(self, char: str) -> tuple[int, tuple[str, ...]] | None:
        return self.chars.get(char)

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
