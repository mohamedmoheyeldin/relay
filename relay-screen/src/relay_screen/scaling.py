"""Coordinate math between screenshot pixels and physical desktop pixels.

Claude sees screenshots of one display, resized to fit an image budget. Every
coordinate it sends back is in that screenshot's pixel space (computer_toolset
semantics, docs/decisions.md D-009). This module is the only place that converts
between that space and physical pixels on the Windows virtual desktop.

The process is per-monitor-v2 DPI aware (see dpi.py), so display rectangles are
physical pixels and the Windows scale factor (100/125/150%) does not enter the math.
"""

from __future__ import annotations

import math
from dataclasses import dataclass

DEFAULT_LONG_EDGE = 1568
MAX_LONG_EDGE = 2576
MAX_PIXELS = 3_750_000


@dataclass(frozen=True)
class Rect:
    """Axis-aligned rectangle in physical virtual-desktop pixels. May have negative origin."""

    left: int
    top: int
    width: int
    height: int

    @property
    def right(self) -> int:
        """Exclusive right edge."""
        return self.left + self.width

    @property
    def bottom(self) -> int:
        """Exclusive bottom edge."""
        return self.top + self.height

    def contains(self, x: int, y: int) -> bool:
        return self.left <= x < self.right and self.top <= y < self.bottom

    def intersect(self, other: Rect) -> Rect | None:
        left = max(self.left, other.left)
        top = max(self.top, other.top)
        right = min(self.right, other.right)
        bottom = min(self.bottom, other.bottom)
        if right <= left or bottom <= top:
            return None
        return Rect(left, top, right - left, bottom - top)


@dataclass(frozen=True)
class ImageBudget:
    """Size limits for images returned to Claude."""

    long_edge: int = DEFAULT_LONG_EDGE
    max_pixels: int = MAX_PIXELS

    def __post_init__(self) -> None:
        if not 64 <= self.long_edge <= MAX_LONG_EDGE:
            raise ValueError(f"long_edge must be between 64 and {MAX_LONG_EDGE}")
        if not 4096 <= self.max_pixels <= MAX_PIXELS:
            raise ValueError(f"max_pixels must be between 4096 and {MAX_PIXELS}")


def fit_size(width: int, height: int, budget: ImageBudget) -> tuple[int, int]:
    """Largest size with the same aspect ratio that fits the budget. Never upscales."""
    if width <= 0 or height <= 0:
        raise ValueError("width and height must be positive")
    scale = min(
        1.0,
        budget.long_edge / max(width, height),
        math.sqrt(budget.max_pixels / (width * height)),
    )
    return max(1, round(width * scale)), max(1, round(height * scale))


@dataclass(frozen=True)
class ScreenshotGeometry:
    """How a display's physical pixels map onto the screenshot Claude sees."""

    display: Rect
    width: int
    height: int

    @classmethod
    def for_display(cls, display: Rect, budget: ImageBudget) -> ScreenshotGeometry:
        width, height = fit_size(display.width, display.height, budget)
        return cls(display, width, height)

    @property
    def scale_x(self) -> float:
        return self.width / self.display.width

    @property
    def scale_y(self) -> float:
        return self.height / self.display.height

    def check_point(self, x: int, y: int) -> None:
        """Raise ValueError if (x, y) is outside the screenshot."""
        if not (0 <= x < self.width and 0 <= y < self.height):
            raise ValueError(
                f"coordinate ({x}, {y}) is outside the screenshot "
                f"(0..{self.width - 1}, 0..{self.height - 1})"
            )

    def to_physical(self, x: float, y: float) -> tuple[int, int]:
        """Screenshot pixel -> physical virtual-desktop pixel.

        Maps the centre of the screenshot pixel to the matching physical pixel and
        clamps to the display, so an edge pixel never lands on a neighbouring monitor.
        """
        px = math.floor((x + 0.5) / self.scale_x)
        py = math.floor((y + 0.5) / self.scale_y)
        px = min(max(px, 0), self.display.width - 1)
        py = min(max(py, 0), self.display.height - 1)
        return self.display.left + px, self.display.top + py

    def to_screenshot(self, x: int, y: int) -> tuple[int, int]:
        """Physical virtual-desktop pixel -> screenshot pixel (clamped to the image)."""
        sx = math.floor((x - self.display.left + 0.5) * self.scale_x)
        sy = math.floor((y - self.display.top + 0.5) * self.scale_y)
        return min(max(sx, 0), self.width - 1), min(max(sy, 0), self.height - 1)

    def region_to_physical(self, x0: int, y0: int, x1: int, y1: int) -> Rect:
        """Screenshot region [x0, y0, x1, y1] (inclusive corners) -> physical Rect."""
        if x1 < x0 or y1 < y0:
            raise ValueError("region must be [x0, y0, x1, y1] with x0 <= x1 and y0 <= y1")
        self.check_point(x0, y0)
        self.check_point(x1, y1)
        left = self.display.left + math.floor(x0 / self.scale_x)
        top = self.display.top + math.floor(y0 / self.scale_y)
        right = self.display.left + math.ceil((x1 + 1) / self.scale_x)
        bottom = self.display.top + math.ceil((y1 + 1) / self.scale_y)
        right = min(right, self.display.right)
        bottom = min(bottom, self.display.bottom)
        return Rect(left, top, right - left, bottom - top)


def normalize_absolute(x: int, y: int, virtual: Rect) -> tuple[int, int]:
    """Physical virtual-desktop pixel -> SendInput absolute coordinates (0..65535).

    Used with MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK, which map 0..65535 onto
    the whole virtual desktop.
    """
    if not virtual.contains(x, y):
        raise ValueError(f"point ({x}, {y}) is outside the virtual desktop {virtual}")
    nx = round((x - virtual.left) * 65535 / max(virtual.width - 1, 1))
    ny = round((y - virtual.top) * 65535 / max(virtual.height - 1, 1))
    return nx, ny
