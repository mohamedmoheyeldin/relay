"""Screen capture for the `screenshot` and `zoom` tools."""

from __future__ import annotations

import io
import threading
from typing import Protocol

from PIL import Image

from relay_screen.scaling import Rect, ScreenshotGeometry


class Grabber(Protocol):
    def grab(self, rect: Rect) -> Image.Image:
        """Capture a physical virtual-desktop rectangle at full resolution (RGB)."""
        ...


class MssGrabber:
    """GDI capture via mss. Works across GPUs, including displays on an iGPU (D-013).

    mss handles are thread-bound, so each thread gets its own instance.
    """

    def __init__(self) -> None:
        self._local = threading.local()

    def _sct(self):  # type: ignore[no-untyped-def]
        sct = getattr(self._local, "sct", None)
        if sct is None:
            import mss

            sct = mss.mss()
            self._local.sct = sct
        return sct

    def grab(self, rect: Rect) -> Image.Image:
        shot = self._sct().grab(
            {"left": rect.left, "top": rect.top, "width": rect.width, "height": rect.height}
        )
        return Image.frombytes("RGB", shot.size, shot.bgra, "raw", "BGRX")


def screenshot(grabber: Grabber, geometry: ScreenshotGeometry) -> Image.Image:
    """Capture the whole display and resize it to the geometry's screenshot size."""
    image = grabber.grab(geometry.display)
    if image.size != (geometry.width, geometry.height):
        image = image.resize((geometry.width, geometry.height), Image.Resampling.LANCZOS)
    return image


def zoom(
    grabber: Grabber,
    geometry: ScreenshotGeometry,
    region: tuple[int, int, int, int],
) -> Image.Image:
    """Capture a screenshot-space region at full resolution, scaled to fit the screenshot box.

    The box is the display's normal screenshot size, which already fits the image budget,
    so the zoomed image does too. Coordinates Claude sends after a zoom stay in the full
    screenshot's space, so the zoomed image needs no geometry of its own.
    """
    physical = geometry.region_to_physical(*region)
    image = grabber.grab(physical)
    scale = min(geometry.width / physical.width, geometry.height / physical.height)
    size = (max(1, round(physical.width * scale)), max(1, round(physical.height * scale)))
    if size != image.size:
        image = image.resize(size, Image.Resampling.LANCZOS)
    return image


def encode_png(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="PNG", optimize=False, compress_level=6)
    return buffer.getvalue()
