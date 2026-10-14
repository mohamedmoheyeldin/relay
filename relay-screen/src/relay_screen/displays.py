"""Monitor enumeration and the virtual-desktop layout."""

from __future__ import annotations

import ctypes
from ctypes import wintypes
from dataclasses import asdict, dataclass

from relay_screen import win32
from relay_screen.scaling import Rect


@dataclass(frozen=True)
class Display:
    """One monitor. ``id`` is 1-based and stable for a given layout (primary first)."""

    id: int
    name: str
    rect: Rect
    primary: bool
    scale: float  # Windows scale factor, 1.0 = 100%. Informational: rects are physical px.
    handle: int = 0

    def describe(self) -> dict[str, object]:
        data = asdict(self)
        data.pop("handle")
        return data


def order_displays(raw: list[Display]) -> list[Display]:
    """Primary first, then left-to-right, top-to-bottom; renumber ids from 1."""
    ordered = sorted(raw, key=lambda d: (not d.primary, d.rect.left, d.rect.top))
    return [
        Display(i, d.name, d.rect, d.primary, d.scale, d.handle)
        for i, d in enumerate(ordered, start=1)
    ]


def virtual_desktop(displays: list[Display]) -> Rect:
    """Bounding box of all displays."""
    if not displays:
        raise ValueError("no displays")
    left = min(d.rect.left for d in displays)
    top = min(d.rect.top for d in displays)
    right = max(d.rect.right for d in displays)
    bottom = max(d.rect.bottom for d in displays)
    return Rect(left, top, right - left, bottom - top)


def display_at(displays: list[Display], x: int, y: int) -> Display | None:
    return next((d for d in displays if d.rect.contains(x, y)), None)


def enumerate_displays() -> list[Display]:
    """Ask Windows for the current monitors (requires DPI awareness to be set)."""
    user32 = win32.user32()
    shcore = win32.shcore()
    found: list[Display] = []

    def callback(hmonitor, _hdc, _rect, _lparam):  # type: ignore[no-untyped-def]
        info = win32.MONITORINFOEXW()
        info.cbSize = ctypes.sizeof(win32.MONITORINFOEXW)
        if not user32.GetMonitorInfoW(hmonitor, ctypes.byref(info)):
            return True
        rc = info.rcMonitor
        dpi_x, dpi_y = wintypes.UINT(), wintypes.UINT()
        hr = shcore.GetDpiForMonitor(
            hmonitor, win32.MDT_EFFECTIVE_DPI, ctypes.byref(dpi_x), ctypes.byref(dpi_y)
        )
        scale = dpi_x.value / 96 if hr == 0 and dpi_x.value else 1.0
        found.append(
            Display(
                id=0,
                name=info.szDevice.removeprefix("\\\\.\\"),
                rect=Rect(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top),
                primary=bool(info.dwFlags & win32.MONITORINFOF_PRIMARY),
                scale=round(scale, 2),
                handle=int(hmonitor or 0),
            )
        )
        return True

    proc = win32.MONITORENUMPROC(callback)
    if not user32.EnumDisplayMonitors(None, None, proc, 0):
        raise ctypes.WinError(ctypes.get_last_error())
    return order_displays(found)


def foreground_display(displays: list[Display]) -> Display | None:
    """The display holding the foreground window, if any."""
    user32 = win32.user32()
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return None
    hmonitor = user32.MonitorFromWindow(hwnd, win32.MONITOR_DEFAULTTONULL)
    if not hmonitor:
        return None
    return next((d for d in displays if d.handle == int(hmonitor)), None)
