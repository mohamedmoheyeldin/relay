"""UI Automation: read the accessibility tree and act on controls without vision (D-008).

Elements get short ids (``e12``) that later calls (``uia_invoke``, ``uia_set_value``)
refer to. Bounds are reported in screenshot pixels of the active display, like every
other coordinate Relay gives Claude. Output is capped so a large window stays well
under the MCP output limit.

The Windows side uses the ``uiautomation`` package (D-013); everything else here is
pure and tested with fake elements.
"""

from __future__ import annotations

import threading
import time
from collections import OrderedDict
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass
from typing import Protocol

from relay_screen.displays import Display
from relay_screen.errors import ActionError
from relay_screen.scaling import Rect, ScreenshotGeometry

MAX_NODES = 400  # elements per tree listing
MAX_CHARS = 24_000  # ~6-8k tokens
MAX_SEARCH_NODES = 4_000
SEARCH_SECONDS = 6.0
MAX_MATCHES = 10
MAX_IDS = 5_000  # remembered element ids
NAME_LIMIT = 80
ACTIONS = frozenset({"invoke", "toggle", "expand", "select", "value"})
LAYOUT_ROLES = frozenset({"Pane", "Group", "Custom", "Text"})
# What the operator can act on, for interactive listings of big trees like web pages.
INPUT_ROLES = frozenset(
    {"Edit", "ComboBox", "CheckBox", "RadioButton", "Button", "Hyperlink", "Link", "MenuItem",
     "TabItem", "ListItem", "Slider", "Spinner", "SplitButton", "DataItem", "TreeItem"}
)  # fmt: skip


class UiaElement(Protocol):
    @property
    def role(self) -> str: ...  # "Button", "Edit", "MenuItem" ...
    @property
    def name(self) -> str: ...
    @property
    def automation_id(self) -> str: ...
    @property
    def enabled(self) -> bool: ...
    @property
    def rect(self) -> Rect | None: ...  # physical virtual-desktop px; None if no bounds
    def children(self) -> Sequence[UiaElement]: ...
    def patterns(self) -> frozenset[str]: ...  # subset of {"invoke", "value", ...}
    def value(self) -> str | None: ...
    def invoke(self) -> str: ...  # performs the default action, returns what it did
    def set_value(self, value: str) -> None: ...


class UiaBackend(Protocol):
    def windows(self, title: str | None) -> Sequence[UiaElement]:
        """Top-level windows to search: the foreground window and its popups, or the
        windows whose title contains ``title``."""
        ...

    def element_at(self, x: int, y: int) -> UiaElement | None:
        """The control at physical virtual-desktop pixel (x, y), or the nearest
        ancestor that can be pressed (so the text of a button gives the button)."""
        ...

    def run(self, work: Callable[[], object]) -> object:
        """Run ``work`` with COM initialized for the calling thread."""
        ...


class Snapshots(Protocol):
    def snapshot(self, roots: Sequence[UiaElement]) -> Sequence[UiaElement]:
        """The same subtrees read in one cached call each (UIA CacheRequest), so a
        listing doesn't cross into the app once per property (D-089)."""
        ...


def _interactive(element: UiaElement) -> bool:
    return element.role in INPUT_ROLES or bool(element.patterns() & ACTIONS)


@dataclass
class Entry:
    id: str
    depth: int
    element: UiaElement


def _clip(text: str, limit: int = NAME_LIMIT) -> str:
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 1] + "…"


def _matches(element: UiaElement, query: str) -> bool:
    q = query.casefold()
    return (
        q in element.name.casefold()
        or q == element.role.casefold()
        or (bool(element.automation_id) and q in element.automation_id.casefold())
    )


def _noise(element: UiaElement) -> bool:
    """Unnamed elements a listing can skip: hidden ones (no bounds) and bare layout
    containers. Their children are still listed."""
    if element.name.strip() or element.automation_id:
        return False
    if element.rect is None:
        return True
    return element.role in LAYOUT_ROLES and not element.patterns() & ACTIONS


def screenshot_box(
    geometry: ScreenshotGeometry, rect: Rect | None, displays: Sequence[Display] = ()
) -> str:
    """Bounds as screenshot px of the active display, or why there are none.

    An element on another monitor names that display, so Claude can select_display it.
    """
    if rect is None or rect.width <= 0 or rect.height <= 0:
        return "no bounds"
    cx, cy = rect.left + rect.width // 2, rect.top + rect.height // 2
    if not geometry.display.contains(cx, cy):
        other = next((d for d in displays if d.rect.contains(cx, cy)), None)
        if other is not None:
            return f"on display {other.id}, not the active one (select_display {other.id})"
        return "not on the active display"
    x0, y0 = geometry.to_screenshot(rect.left, rect.top)
    x1, y1 = geometry.to_screenshot(
        min(rect.right, geometry.display.right) - 1, min(rect.bottom, geometry.display.bottom) - 1
    )
    sx, sy = geometry.to_screenshot(cx, cy)
    return f"center ({sx}, {sy}) box [{x0}, {y0}, {x1}, {y1}]"


def center(geometry: ScreenshotGeometry, rect: Rect | None) -> tuple[int, int] | None:
    if rect is None or rect.width <= 0 or rect.height <= 0:
        return None
    cx, cy = rect.left + rect.width // 2, rect.top + rect.height // 2
    if not geometry.display.contains(cx, cy):
        return None
    return geometry.to_screenshot(cx, cy)


class UiaService:
    def __init__(self, backend: UiaBackend, clock: Callable[[], float] = time.monotonic) -> None:
        self.backend = backend
        self.clock = clock
        self._ids: OrderedDict[str, UiaElement] = OrderedDict()
        self._next = 1
        self._lock = threading.Lock()

    # -- ids ------------------------------------------------------------------

    def _remember(self, element: UiaElement) -> str:
        with self._lock:
            element_id = f"e{self._next}"
            self._next += 1
            self._ids[element_id] = element
            while len(self._ids) > MAX_IDS:
                self._ids.popitem(last=False)
            return element_id

    def element(self, element_id: str) -> UiaElement:
        with self._lock:
            found = self._ids.get(element_id.strip())
        if found is None:
            raise ActionError(
                f"unknown element {element_id!r}; get fresh ids from uia_tree or uia_find"
            )
        return found

    # -- formatting -------------------------------------------------------------

    def describe(self, element: UiaElement) -> str:
        """Short label for prompts and logs, e.g. 'Button "Save"'."""
        name = f' "{_clip(element.name)}"' if element.name else ""
        return f"{element.role}{name}"

    def _line(
        self, entry: Entry, geometry: ScreenshotGeometry, displays: Sequence[Display] = ()
    ) -> str:
        e = entry.element
        parts = [f"{'  ' * entry.depth}[{entry.id}] {e.role}"]
        if e.name:
            parts.append(f'"{_clip(e.name)}"')
        if e.automation_id:
            parts.append(f"#{_clip(e.automation_id, 40)}")
        value = e.value() if "value" in e.patterns() else None
        if value:
            parts.append(f"value={_clip(value, 60)!r}")
        if not e.enabled:
            parts.append("disabled")
        actions = sorted(e.patterns() & ACTIONS)
        if actions:
            parts.append("{" + ",".join(actions) + "}")
        parts.append(screenshot_box(geometry, e.rect, displays))
        return " ".join(parts)

    # -- reads ----------------------------------------------------------------

    def _walk(
        self,
        roots: Sequence[UiaElement],
        depth: int,
        limit: int,
        deadline: float,
        hide: Callable[[UiaElement], bool] | None = None,
    ) -> Iterator[tuple[int, UiaElement]]:
        """Depth-first walk yielding (indent, element). Elements ``hide`` rejects are
        not yielded; their children take their indent. ``depth`` counts real levels."""
        stack: list[tuple[int, int, UiaElement]] = [(0, 0, r) for r in reversed(roots)]
        seen = 0
        while stack and seen < limit and self.clock() < deadline:
            level, indent, element = stack.pop()
            seen += 1
            shown = level == 0 or hide is None or not hide(element)
            if shown:
                yield indent, element
            if level >= depth:
                continue
            try:
                kids = element.children()
            except Exception:
                continue  # element vanished while walking
            child_indent = indent + 1 if shown else indent
            stack.extend((level + 1, child_indent, k) for k in reversed(kids))

    def _snapshot(self, roots: Sequence[UiaElement]) -> Sequence[UiaElement]:
        snap = getattr(self.backend, "snapshot", None)
        if snap is None:
            return roots
        try:
            return snap(roots)
        except Exception:  # an app that refuses the cache is read the slow way
            return roots

    def tree(
        self,
        geometry: ScreenshotGeometry,
        window: str | None = None,
        depth: int = 6,
        filter: str | None = None,
        displays: Sequence[Display] = (),
        root: str | None = None,
        interactive: bool = False,
    ) -> str:
        """`root` lists from an element id (a web page's document); `interactive` keeps
        only controls that can be acted on, plus their named containers' children."""
        if not 1 <= depth <= 60:
            raise ActionError("depth must be between 1 and 60")
        start = self.element(root) if root else None

        def work() -> str:
            roots = [start] if start is not None else self.backend.windows(window)
            if not roots:
                raise ActionError(_no_window(window))
            roots = self._snapshot(roots)
            deadline = self.clock() + SEARCH_SECONDS
            lines: list[str] = []
            size = 0
            truncated = ""
            listed = 0
            hide = None if filter else _noise
            if interactive:
                hide = lambda element: not _interactive(element)  # noqa: E731
            for level, element in self._walk(roots, depth, MAX_SEARCH_NODES, deadline, hide):
                if filter and not _matches(element, filter):
                    continue
                entry = Entry(self._remember(element), level, element)
                line = self._line(entry, geometry, displays)
                if listed >= MAX_NODES or size + len(line) > MAX_CHARS:
                    truncated = (
                        f"… output cut at {listed} elements. Narrow it with depth, filter, "
                        "window or uia_find."
                    )
                    break
                lines.append(line)
                size += len(line) + 1
                listed += 1
            if self.clock() >= deadline and not truncated:
                truncated = "… stopped after the time limit; the tree may be incomplete."
            if not lines:
                return f"No elements match {filter!r}." if filter else "The window is empty."
            header = "Elements (ids stay valid for later uia_invoke / uia_set_value):"
            return "\n".join([header, *lines, *([truncated] if truncated else [])])

        return str(self.backend.run(work))

    def find(
        self,
        geometry: ScreenshotGeometry,
        query: str,
        role: str | None = None,
        window: str | None = None,
        displays: Sequence[Display] = (),
    ) -> str:
        if not query.strip():
            raise ActionError("query must not be empty")

        def work() -> str:
            roots = self.backend.windows(window)
            if not roots:
                raise ActionError(_no_window(window))
            roots = self._snapshot(roots)
            deadline = self.clock() + SEARCH_SECONDS
            q = query.strip().casefold()
            exact: list[UiaElement] = []
            partial: list[UiaElement] = []
            for _, element in self._walk(roots, 60, MAX_SEARCH_NODES, deadline):
                if role and element.role.casefold() != role.strip().casefold():
                    continue
                name = element.name.strip().casefold()
                if name == q or element.automation_id.casefold() == q:
                    exact.append(element)
                elif _matches(element, q):
                    partial.append(element)
                if len(exact) >= MAX_MATCHES:
                    break
            found = (exact + partial)[:MAX_MATCHES]
            if not found:
                return f"No element matches {query!r}" + (f" with role {role}." if role else ".")
            lines = [self._line(Entry(self._remember(e), 0, e), geometry, displays) for e in found]
            return "\n".join([f"{len(found)} match(es), best first:", *lines])

        return str(self.backend.run(work))

    # -- acts -----------------------------------------------------------------

    def invoke(self, element_id: str) -> str:
        element = self.element(element_id)

        def work() -> str:
            if not element.enabled:
                raise ActionError(f"{self.describe(element)} is disabled")
            if not element.patterns() & {"invoke", "toggle", "expand", "select"}:
                raise ActionError(
                    f"{self.describe(element)} has no invoke, toggle, expand or select "
                    "action; click its center instead"
                )
            return element.invoke()

        return str(self.backend.run(work))

    def set_value(self, element_id: str, value: str) -> str:
        element = self.element(element_id)

        def work() -> str:
            if "value" not in element.patterns():
                raise ActionError(
                    f"{self.describe(element)} has no value to set; click it and type"
                )
            element.set_value(value)
            return "value set"

        return str(self.backend.run(work))

    def label_at(self, x: int, y: int) -> str | None:
        """'Button "Send"' for the control at physical pixel (x, y), if UIA sees one."""

        def work() -> str | None:
            element = self.backend.element_at(x, y)
            return self.describe(element) if element is not None else None

        found = self.backend.run(work)
        return found if isinstance(found, str) else None

    def label(self, element_id: str) -> str:
        element = self.element(element_id)
        return str(self.backend.run(lambda: self.describe(element)))

    def rect(self, element_id: str) -> Rect | None:
        element = self.element(element_id)
        result = self.backend.run(lambda: element.rect)
        return result if isinstance(result, Rect) else None


def _no_window(window: str | None) -> str:
    if window:
        return f"no open window title contains {window!r}"
    return "no foreground window"
