"""Global Ctrl+Alt+Pause hotkey for the kill switch (Windows only).

With Ctrl held, the Pause key arrives as VK_CANCEL (Break) on most keyboards, so
both are registered.
"""

from __future__ import annotations

import ctypes
from collections.abc import Callable
from ctypes import wintypes

from relay_screen import win32

MOD_ALT = 0x0001
MOD_CONTROL = 0x0002
MOD_NOREPEAT = 0x4000
VK_CANCEL = 0x03
VK_PAUSE = 0x13
WM_HOTKEY = 0x0312

_IDS = {1: VK_PAUSE, 2: VK_CANCEL}


def run(on_press: Callable[[], None]) -> None:
    """Register the hotkey on this thread and pump its messages forever."""
    user32 = win32.user32()
    registered = [
        hotkey_id
        for hotkey_id, vk in _IDS.items()
        if user32.RegisterHotKey(None, hotkey_id, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, vk)
    ]
    if not registered:
        return  # another program owns the hotkey; the corner still works
    msg = wintypes.MSG()
    while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
        if msg.message == WM_HOTKEY and msg.wParam in registered:
            on_press()
