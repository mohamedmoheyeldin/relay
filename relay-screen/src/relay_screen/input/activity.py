"""When the owner last used the mouse or keyboard (D-099).

The operator works alongside the owner: before real input it waits for a pause in
theirs, instead of fighting them for the cursor. Windows counts Relay's own SendInput
too, so callers compare against when Relay last acted.
"""

from __future__ import annotations

import ctypes
import sys
from ctypes import wintypes


class _LastInputInfo(ctypes.Structure):
    _fields_ = [("cbSize", wintypes.UINT), ("dwTime", wintypes.DWORD)]


def seconds_since_input() -> float | None:
    """Seconds since the last mouse or keyboard input on this desktop; None off Windows."""
    if sys.platform != "win32":
        return None
    info = _LastInputInfo()
    info.cbSize = ctypes.sizeof(info)
    user32 = ctypes.WinDLL("user32")
    kernel32 = ctypes.WinDLL("kernel32")
    if not user32.GetLastInputInfo(ctypes.byref(info)):
        return None
    now = kernel32.GetTickCount() & 0xFFFFFFFF
    return ((now - info.dwTime) & 0xFFFFFFFF) / 1000.0
