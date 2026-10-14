"""Thin ctypes bindings to the Win32 calls relay-screen needs.

Everything here is resolved lazily so the package imports (and its pure modules are
testable) on any OS. Call ``require_windows()`` before touching a binding.
"""

from __future__ import annotations

import ctypes
import sys
from ctypes import wintypes
from functools import cache

IS_WINDOWS = sys.platform == "win32"

# GetSystemMetrics indices for the virtual desktop
SM_XVIRTUALSCREEN = 76
SM_YVIRTUALSCREEN = 77
SM_CXVIRTUALSCREEN = 78
SM_CYVIRTUALSCREEN = 79

MONITORINFOF_PRIMARY = 0x1
MONITOR_DEFAULTTONULL = 0
MDT_EFFECTIVE_DPI = 0

# DPI awareness contexts are pseudo-handles: negative integers cast to HANDLE
DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
PROCESS_PER_MONITOR_DPI_AWARE = 2


class NotWindowsError(RuntimeError):
    pass


def require_windows() -> None:
    if not IS_WINDOWS:
        raise NotWindowsError("relay-screen desktop features need Windows")


class MONITORINFOEXW(ctypes.Structure):
    _fields_ = (
        ("cbSize", wintypes.DWORD),
        ("rcMonitor", wintypes.RECT),
        ("rcWork", wintypes.RECT),
        ("dwFlags", wintypes.DWORD),
        ("szDevice", wintypes.WCHAR * 32),
    )


# WINFUNCTYPE only exists on Windows; CFUNCTYPE keeps the module importable elsewhere.
_FUNCTYPE = ctypes.WINFUNCTYPE if IS_WINDOWS else ctypes.CFUNCTYPE
MONITORENUMPROC = _FUNCTYPE(
    wintypes.BOOL, wintypes.HMONITOR, wintypes.HDC, ctypes.POINTER(wintypes.RECT), wintypes.LPARAM
)


@cache
def user32() -> ctypes.WinDLL:
    require_windows()
    dll = ctypes.WinDLL("user32", use_last_error=True)
    dll.GetMonitorInfoW.argtypes = (wintypes.HMONITOR, ctypes.POINTER(MONITORINFOEXW))
    dll.GetMonitorInfoW.restype = wintypes.BOOL
    dll.EnumDisplayMonitors.argtypes = (
        wintypes.HDC,
        ctypes.POINTER(wintypes.RECT),
        MONITORENUMPROC,
        wintypes.LPARAM,
    )
    dll.EnumDisplayMonitors.restype = wintypes.BOOL
    dll.MonitorFromWindow.argtypes = (wintypes.HWND, wintypes.DWORD)
    dll.MonitorFromWindow.restype = wintypes.HMONITOR
    dll.GetForegroundWindow.restype = wintypes.HWND
    dll.GetSystemMetrics.argtypes = (ctypes.c_int,)
    dll.GetSystemMetrics.restype = ctypes.c_int
    dll.GetCursorPos.argtypes = (ctypes.POINTER(wintypes.POINT),)
    dll.GetCursorPos.restype = wintypes.BOOL
    dll.SetCursorPos.argtypes = (ctypes.c_int, ctypes.c_int)
    dll.SetCursorPos.restype = wintypes.BOOL
    dll.SetProcessDpiAwarenessContext.argtypes = (wintypes.HANDLE,)
    dll.SetProcessDpiAwarenessContext.restype = wintypes.BOOL
    dll.VkKeyScanW.argtypes = (wintypes.WCHAR,)
    dll.VkKeyScanW.restype = ctypes.c_short
    dll.MapVirtualKeyW.argtypes = (wintypes.UINT, wintypes.UINT)
    dll.MapVirtualKeyW.restype = wintypes.UINT
    return dll


@cache
def shcore() -> ctypes.WinDLL:
    require_windows()
    dll = ctypes.WinDLL("shcore", use_last_error=True)
    dll.GetDpiForMonitor.argtypes = (
        wintypes.HMONITOR,
        ctypes.c_int,
        ctypes.POINTER(wintypes.UINT),
        ctypes.POINTER(wintypes.UINT),
    )
    dll.GetDpiForMonitor.restype = ctypes.c_long  # HRESULT, checked by callers
    dll.SetProcessDpiAwareness.argtypes = (ctypes.c_int,)
    dll.SetProcessDpiAwareness.restype = ctypes.c_long
    return dll


def cursor_position() -> tuple[int, int]:
    point = wintypes.POINT()
    if not user32().GetCursorPos(ctypes.byref(point)):
        raise ctypes.WinError(ctypes.get_last_error())
    return point.x, point.y
