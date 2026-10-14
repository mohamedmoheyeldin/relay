"""Windows UI Automation backend for uia.py, on the ``uiautomation`` package (D-013).

All COM work runs on one dedicated thread: UIA element pointers belong to the thread
that created them, and MCP tool calls arrive on varying worker threads.
"""

from __future__ import annotations

import atexit
import ctypes
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from ctypes import wintypes
from typing import Any

from relay_screen import win32
from relay_screen.scaling import Rect

SPI_GETSCREENREADER = 0x0046
SPI_SETSCREENREADER = 0x0047
SPIF_SENDCHANGE = 0x0002


def enable_screen_reader_flag() -> None:
    """Tell apps an assistive tool is running so Chromium/Electron build full UIA trees
    (D-017). Restores the previous value when the server exits."""
    user32 = win32.user32()
    current = wintypes.BOOL()
    user32.SystemParametersInfoW(SPI_GETSCREENREADER, 0, ctypes.byref(current), 0)
    if current.value:
        return
    user32.SystemParametersInfoW(SPI_SETSCREENREADER, 1, None, SPIF_SENDCHANGE)
    atexit.register(user32.SystemParametersInfoW, SPI_SETSCREENREADER, 0, None, SPIF_SENDCHANGE)


class WinElement:
    def __init__(self, control: Any, auto: Any) -> None:
        self._c = control
        self._auto = auto
        self._patterns: frozenset[str] | None = None

    @property
    def role(self) -> str:
        name = self._c.ControlTypeName or "Unknown"
        return name.removesuffix("Control") or name

    @property
    def name(self) -> str:
        return self._c.Name or ""

    @property
    def automation_id(self) -> str:
        return self._c.AutomationId or ""

    @property
    def enabled(self) -> bool:
        return bool(self._c.IsEnabled)

    @property
    def rect(self) -> Rect | None:
        r = self._c.BoundingRectangle
        if r is None or r.width() <= 0 or r.height() <= 0:
            return None
        return Rect(r.left, r.top, r.width(), r.height())

    def children(self) -> Sequence[WinElement]:
        return [WinElement(c, self._auto) for c in self._c.GetChildren()]

    def _pattern(self, name: str) -> Any:
        return self._c.GetPattern(getattr(self._auto.PatternId, name))

    def patterns(self) -> frozenset[str]:
        if self._patterns is None:
            ids = {
                "invoke": "InvokePattern",
                "value": "ValuePattern",
                "toggle": "TogglePattern",
                "expand": "ExpandCollapsePattern",
                "select": "SelectionItemPattern",
            }
            self._patterns = frozenset(k for k, v in ids.items() if self._pattern(v))
        return self._patterns

    def value(self) -> str | None:
        pattern = self._pattern("ValuePattern")
        return pattern.Value if pattern else None

    def invoke(self) -> str:
        patterns = self.patterns()
        if "invoke" in patterns:
            self._pattern("InvokePattern").Invoke(waitTime=0)
            return "invoked"
        if "toggle" in patterns:
            self._pattern("TogglePattern").Toggle(waitTime=0)
            return "toggled"
        if "expand" in patterns:
            pattern = self._pattern("ExpandCollapsePattern")
            if pattern.ExpandCollapseState == self._auto.ExpandCollapseState.Expanded:
                pattern.Collapse(waitTime=0)
                return "collapsed"
            pattern.Expand(waitTime=0)
            return "expanded"
        self._pattern("SelectionItemPattern").Select(waitTime=0)
        return "selected"

    def set_value(self, value: str) -> None:
        self._pattern("ValuePattern").SetValue(value, waitTime=0)


class WinUiaBackend:
    def __init__(self) -> None:
        win32.require_windows()
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="relay-uia")
        self._auto: Any = self._executor.submit(self._init).result()

    @staticmethod
    def _init() -> Any:
        # Importing comtypes initializes COM for this (the UIA) thread.
        import uiautomation as auto  # pyright: ignore[reportMissingImports]

        enable_screen_reader_flag()
        return auto

    def run(self, work: Callable[[], object]) -> object:
        return self._executor.submit(work).result()

    def windows(self, title: str | None) -> list[WinElement]:
        auto = self._auto
        tops = auto.GetRootControl().GetChildren()
        if title:
            wanted = title.casefold()
            found = [w for w in tops if wanted in (w.Name or "").casefold()]
            return [WinElement(w, auto) for w in found]
        foreground = auto.GetForegroundControl()
        if foreground is None:
            return []
        pid = foreground.ProcessId
        handle = foreground.NativeWindowHandle
        popups = [
            w
            for w in tops
            if w.ProcessId == pid and w.NativeWindowHandle != handle and not w.IsOffscreen
        ]
        return [WinElement(w, auto) for w in [foreground, *popups]]
