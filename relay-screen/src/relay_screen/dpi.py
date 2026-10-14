"""Per-monitor DPI awareness.

Must run before any capture or UI library is imported. Without it, Windows
virtualizes coordinates on scaled or mixed-DPI setups and screenshots, UIA rectangles
and clicks end up in different coordinate spaces (docs/decisions.md D-013).
"""

from __future__ import annotations

import ctypes
import logging

from relay_screen import win32

log = logging.getLogger(__name__)

_done = False


def set_dpi_awareness() -> str:
    """Make the process per-monitor-v2 DPI aware. Returns the mode that took effect."""
    global _done
    if not win32.IS_WINDOWS:
        return "not-windows"
    if _done:
        return "already-set"
    _done = True
    context = ctypes.c_void_p(win32.DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
    if win32.user32().SetProcessDpiAwarenessContext(context):
        return "per-monitor-v2"
    # Fails with ERROR_ACCESS_DENIED if awareness was already set (by a manifest or an
    # earlier import). Fall back to the Windows 8.1 API for older hosts.
    if win32.shcore().SetProcessDpiAwareness(win32.PROCESS_PER_MONITOR_DPI_AWARE) == 0:
        return "per-monitor"
    log.warning("could not change DPI awareness; it was probably set earlier in the process")
    return "unchanged"
