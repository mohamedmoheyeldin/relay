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


CHOSEN_BY_SELECT = frozenset({"RadioButton", "TabItem"})
PRESSABLE_DEPTH = 4  # ancestors to look through for the button a click lands on


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

    def toggled(self) -> bool | None:
        """Whether a checkbox or switch is on; None when it isn't one (D-100)."""
        pattern = self._pattern("TogglePattern") if "toggle" in self.patterns() else None
        return {0: False, 1: True}.get(pattern.ToggleState) if pattern else None

    def selected(self) -> bool | None:
        """Whether a radio button is the chosen one; None when it isn't one (D-104)."""
        if self.role != "RadioButton" or "select" not in self.patterns():
            return None
        return bool(self._pattern("SelectionItemPattern").IsSelected)

    def invoke(self) -> str:
        patterns = self.patterns()
        if self.role in CHOSEN_BY_SELECT and "select" in patterns:
            self._pattern("SelectionItemPattern").Select(waitTime=0)
            return "selected"
        if "invoke" in patterns:
            self._pattern("InvokePattern").Invoke(waitTime=0)
            if self.role == "ListItem" and "select" in patterns and self._unpicked():
                # A web page's <select> option says Invoked but stays unpicked; Select
                # picks it and fires the page's change event (D-106).
                self._pattern("SelectionItemPattern").Select(waitTime=0)
                return "selected"
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

    def _unpicked(self) -> bool:
        """Still not selected after Invoke; False when the element went away (it opened)."""
        try:
            return not self._pattern("SelectionItemPattern").IsSelected
        except Exception:
            return False

    def set_value(self, value: str) -> None:
        self._pattern("ValuePattern").SetValue(value, waitTime=0)


# UIA property ids (UIAutomationClient.h) cached in one call per listing (D-089).
NAME, CONTROL_TYPE, AUTOMATION_ID, IS_ENABLED, BOUNDS = 30005, 30003, 30011, 30010, 30001
VALUE = 30045
TOGGLE_STATE = 30086  # UIA_ToggleToggleStatePropertyId: 0 off, 1 on, 2 mixed
IS_SELECTED = 30079  # UIA_SelectionItemIsSelectedPropertyId
PATTERN_AVAILABLE = {
    "invoke": 30031,
    "value": 30043,
    "toggle": 30041,
    "expand": 30028,
    "select": 30036,
}
TREE_SCOPE_SUBTREE = 7
MAX_CACHED_DEPTH = 60


class CachedElement:
    """An element whose properties and children came from one CacheRequest (D-089).

    Reads cost nothing; invoke and set_value go to the live element.
    """

    def __init__(self, element: Any, auto: Any, depth: int = 0) -> None:
        self._e = element
        self._auto = auto
        self._depth = depth
        self._patterns: frozenset[str] | None = None

    def _prop(self, prop: int) -> Any:
        return self._e.GetCachedPropertyValue(prop)

    @property
    def role(self) -> str:
        names = getattr(self._auto, "ControlTypeNames", {})
        name = names.get(self._e.CachedControlType, "Unknown")
        return name.removesuffix("Control") or name

    @property
    def name(self) -> str:
        return self._e.CachedName or ""

    @property
    def automation_id(self) -> str:
        return self._e.CachedAutomationId or ""

    @property
    def enabled(self) -> bool:
        return bool(self._e.CachedIsEnabled)

    @property
    def rect(self) -> Rect | None:
        r = self._e.CachedBoundingRectangle
        width, height = r.right - r.left, r.bottom - r.top
        if width <= 0 or height <= 0:
            return None
        return Rect(r.left, r.top, width, height)

    def children(self) -> Sequence[CachedElement]:
        if self._depth >= MAX_CACHED_DEPTH:
            return []
        kids = self._e.GetCachedChildren()
        if not kids:
            return []
        return [
            CachedElement(kids.GetElement(i), self._auto, self._depth + 1)
            for i in range(kids.Length)
        ]

    def patterns(self) -> frozenset[str]:
        if self._patterns is None:
            self._patterns = frozenset(
                k for k, prop in PATTERN_AVAILABLE.items() if self._prop(prop)
            )
        return self._patterns

    def value(self) -> str | None:
        value = self._prop(VALUE)
        return value if isinstance(value, str) else None

    def toggled(self) -> bool | None:
        if "toggle" not in self.patterns():
            return None
        return {0: False, 1: True}.get(self._prop(TOGGLE_STATE))

    def selected(self) -> bool | None:
        if self.role != "RadioButton" or "select" not in self.patterns():
            return None
        return bool(self._prop(IS_SELECTED))

    def _live(self) -> WinElement:
        control = self._auto.Control.CreateControlFromElement(self._e)
        return WinElement(control, self._auto)

    def invoke(self) -> str:
        return self._live().invoke()

    def set_value(self, value: str) -> None:
        self._live().set_value(value)


class WinUiaBackend:
    def __init__(self) -> None:
        win32.require_windows()
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="relay-uia")
        self._auto: Any = self._executor.submit(self._init).result()
        self._request: Any = None

    @staticmethod
    def _init() -> Any:
        # Importing comtypes initializes COM for this (the UIA) thread.
        import uiautomation as auto  # pyright: ignore[reportMissingImports]

        enable_screen_reader_flag()
        return auto

    def run(self, work: Callable[[], object]) -> object:
        return self._executor.submit(work).result()

    def _cache_request(self) -> Any:
        if self._request is None:
            # The package's star import leaves out underscore names, so import it directly.
            from uiautomation.uiautomation import (  # pyright: ignore[reportMissingImports]
                _AutomationClient,
            )

            client = _AutomationClient.instance().IUIAutomation
            request = client.CreateCacheRequest()
            for prop in (
                NAME,
                CONTROL_TYPE,
                AUTOMATION_ID,
                IS_ENABLED,
                BOUNDS,
                VALUE,
                TOGGLE_STATE,
                IS_SELECTED,
            ):
                request.AddProperty(prop)
            for prop in PATTERN_AVAILABLE.values():
                request.AddProperty(prop)
            request.TreeScope = TREE_SCOPE_SUBTREE
            request.TreeFilter = client.ControlViewCondition
            self._request = request
        return self._request

    def snapshot(self, roots: Sequence[Any]) -> list[CachedElement]:
        """Each root's whole subtree in one cross-process call (runs on the UIA thread)."""
        request = self._cache_request()
        out: list[CachedElement] = []
        for root in roots:
            element = root._e if isinstance(root, CachedElement) else root._c.Element
            out.append(CachedElement(element.BuildUpdatedCache(request), self._auto))
        return out

    def element_at(self, x: int, y: int) -> WinElement | None:
        control = self._auto.ControlFromPoint(x, y)
        if control is None:
            return None
        found = WinElement(control, self._auto)
        current = control
        for _ in range(PRESSABLE_DEPTH):
            element = WinElement(current, self._auto)
            if element.patterns() & {"invoke", "toggle", "select", "expand"}:
                return element
            current = current.GetParentControl()
            if current is None:
                break
        return found

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
