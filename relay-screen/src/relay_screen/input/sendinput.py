"""Windows SendInput backend (D-013). Replaces the old scaffold's pyautogui."""

from __future__ import annotations

import ctypes
import time
from collections.abc import Sequence
from ctypes import wintypes

from relay_screen import win32
from relay_screen.input.events import (
    Event,
    InputBlockedError,
    KeyEvent,
    MouseButton,
    MouseMove,
    MouseWheel,
    UnicodeEvent,
)
from relay_screen.scaling import Rect, normalize_absolute

INPUT_MOUSE = 0
INPUT_KEYBOARD = 1

MOUSEEVENTF_MOVE = 0x0001
MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004
MOUSEEVENTF_RIGHTDOWN = 0x0008
MOUSEEVENTF_RIGHTUP = 0x0010
MOUSEEVENTF_MIDDLEDOWN = 0x0020
MOUSEEVENTF_MIDDLEUP = 0x0040
MOUSEEVENTF_WHEEL = 0x0800
MOUSEEVENTF_HWHEEL = 0x1000
MOUSEEVENTF_VIRTUALDESK = 0x4000
MOUSEEVENTF_ABSOLUTE = 0x8000

KEYEVENTF_EXTENDEDKEY = 0x0001
KEYEVENTF_KEYUP = 0x0002
KEYEVENTF_UNICODE = 0x0004
KEYEVENTF_SCANCODE = 0x0008

MAPVK_VK_TO_VSC = 0

_BUTTON_FLAGS = {
    ("left", True): MOUSEEVENTF_LEFTDOWN,
    ("left", False): MOUSEEVENTF_LEFTUP,
    ("right", True): MOUSEEVENTF_RIGHTDOWN,
    ("right", False): MOUSEEVENTF_RIGHTUP,
    ("middle", True): MOUSEEVENTF_MIDDLEDOWN,
    ("middle", False): MOUSEEVENTF_MIDDLEUP,
}

ULONG_PTR = ctypes.c_size_t


class MOUSEINPUT(ctypes.Structure):
    _fields_ = (
        ("dx", wintypes.LONG),
        ("dy", wintypes.LONG),
        ("mouseData", wintypes.DWORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    )


class KEYBDINPUT(ctypes.Structure):
    _fields_ = (
        ("wVk", wintypes.WORD),
        ("wScan", wintypes.WORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    )


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = (
        ("uMsg", wintypes.DWORD),
        ("wParamL", wintypes.WORD),
        ("wParamH", wintypes.WORD),
    )


class _INPUTUNION(ctypes.Union):
    _fields_ = (("mi", MOUSEINPUT), ("ki", KEYBDINPUT), ("hi", HARDWAREINPUT))


class INPUT(ctypes.Structure):
    _anonymous_ = ("u",)
    _fields_ = (("type", wintypes.DWORD), ("u", _INPUTUNION))


# Marks Relay's own events so hooks (e.g. a future recorder) can tell them apart.
RELAY_EXTRA_INFO = 0x52454C59  # "RELY"

_SHIFT_STATE_MODIFIERS = ((1, "shift"), (2, "ctrl"), (4, "alt"))


def virtual_screen() -> Rect:
    user32 = win32.user32()
    return Rect(
        user32.GetSystemMetrics(win32.SM_XVIRTUALSCREEN),
        user32.GetSystemMetrics(win32.SM_YVIRTUALSCREEN),
        user32.GetSystemMetrics(win32.SM_CXVIRTUALSCREEN),
        user32.GetSystemMetrics(win32.SM_CYVIRTUALSCREEN),
    )


class SendInputBackend:
    def __init__(self) -> None:
        self._user32 = win32.user32()
        self._user32.SendInput.argtypes = (wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int)
        self._user32.SendInput.restype = wintypes.UINT

    def _to_input(self, event: Event, virtual: Rect) -> INPUT:
        item = INPUT()
        match event:
            case MouseMove(x, y):
                nx, ny = normalize_absolute(x, y, virtual)
                item.type = INPUT_MOUSE
                item.mi = MOUSEINPUT(
                    nx,
                    ny,
                    0,
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                    0,
                    RELAY_EXTRA_INFO,
                )
            case MouseButton(button, down):
                item.type = INPUT_MOUSE
                item.mi = MOUSEINPUT(0, 0, 0, _BUTTON_FLAGS[(button, down)], 0, RELAY_EXTRA_INFO)
            case MouseWheel(delta, horizontal):
                item.type = INPUT_MOUSE
                flags = MOUSEEVENTF_HWHEEL if horizontal else MOUSEEVENTF_WHEEL
                item.mi = MOUSEINPUT(0, 0, delta & 0xFFFFFFFF, flags, 0, RELAY_EXTRA_INFO)
            case KeyEvent(vk, down, extended):
                item.type = INPUT_KEYBOARD
                flags = (0 if down else KEYEVENTF_KEYUP) | (
                    KEYEVENTF_EXTENDEDKEY if extended else 0
                )
                scan = self._user32.MapVirtualKeyW(vk, MAPVK_VK_TO_VSC)
                item.ki = KEYBDINPUT(vk, scan, flags, 0, RELAY_EXTRA_INFO)
            case UnicodeEvent(code_unit, down):
                item.type = INPUT_KEYBOARD
                flags = KEYEVENTF_UNICODE | (0 if down else KEYEVENTF_KEYUP)
                item.ki = KEYBDINPUT(0, code_unit, flags, 0, RELAY_EXTRA_INFO)
        return item

    def send(self, events: Sequence[Event]) -> None:
        if not events:
            return
        virtual = virtual_screen()
        array = (INPUT * len(events))(*(self._to_input(e, virtual) for e in events))
        sent = self._user32.SendInput(len(events), array, ctypes.sizeof(INPUT))
        if sent != len(events):
            # UIPI failures are silent apart from the short count (SendInput docs).
            raise InputBlockedError(
                f"Windows accepted {sent} of {len(events)} input events. The window in "
                "front may be running as administrator, or a secure desktop (UAC, lock "
                "screen) is active; Relay cannot drive those. Ask the user to handle it."
            )
        last_move = next((e for e in reversed(events) if isinstance(e, MouseMove)), None)
        if last_move is not None:
            self._settle_cursor(last_move)

    def _settle_cursor(self, target: MouseMove) -> None:
        """Absolute normalization can be off by a pixel on wide desktops; correct it."""
        x, y = self.cursor()
        if abs(x - target.x) > 0 or abs(y - target.y) > 0:
            self._user32.SetCursorPos(target.x, target.y)

    def cursor(self) -> tuple[int, int]:
        return win32.cursor_position()

    def resolve_char(self, char: str) -> tuple[int, tuple[str, ...]] | None:
        result = self._user32.VkKeyScanW(char)
        if result == -1:
            return None
        vk = result & 0xFF
        state = (result >> 8) & 0xFF
        modifiers = tuple(name for bit, name in _SHIFT_STATE_MODIFIERS if state & bit)
        return vk, modifiers

    def sleep(self, seconds: float) -> None:
        time.sleep(seconds)
