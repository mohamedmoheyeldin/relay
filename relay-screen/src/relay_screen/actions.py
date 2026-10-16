"""computer_toolset_20260801 member semantics on top of capture and input.

Each member takes the same parameters as the API toolset; coordinates are screenshot
pixels of the active display (D-009). ``run_batch`` implements the documented batch
rule: run in order, stop at the first failure, and answer every later action with the
exact halt text.
"""

from __future__ import annotations

import json
import re
import time
import urllib.parse
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import PureWindowsPath
from typing import Any, Literal

from PIL import Image, ImageChops, ImageDraw

from relay_screen import apps as app_launch
from relay_screen import capture
from relay_screen.apps import MAX_ARGS, AppCatalog
from relay_screen.capture import Grabber
from relay_screen.displays import display_at
from relay_screen.errors import ActionError
from relay_screen.input.driver import MAX_DURATION, InputDriver
from relay_screen.input.events import Button
from relay_screen.input.keys import parse_modifiers
from relay_screen.ocr import OcrService
from relay_screen.safety.approval import CLICKS, ActRequest, describe
from relay_screen.safety.audit import AuditLog, elapsed_ms
from relay_screen.scaling import Rect, ScreenshotGeometry
from relay_screen.state import ScreenState
from relay_screen.uia import UiaService
from relay_screen.uia import center as uia_center

HALT_TEXT = "Not executed: an earlier computer action in this turn failed."

Kind = Literal["read", "act"]

TOOLSET_MEMBERS: dict[str, Kind] = {
    "screenshot": "read",
    "zoom": "read",
    "cursor_position": "read",
    "wait": "read",
    "mouse_move": "act",
    "left_click": "act",
    "right_click": "act",
    "middle_click": "act",
    "double_click": "act",
    "triple_click": "act",
    "left_click_drag": "act",
    "left_mouse_down": "act",
    "left_mouse_up": "act",
    "scroll": "act",
    "type": "act",
    "key": "act",
    "hold_key": "act",
}

# Relay-only members (architecture §4.1); they get the same safety layer and audit log.
RELAY_MEMBERS: dict[str, Kind] = {
    "uia_tree": "read",
    "uia_find": "read",
    "uia_invoke": "act",
    "uia_set_value": "act",
    "find_text": "read",
    "list_apps": "read",
    "launch_app": "act",
    "list_windows": "read",
    "focus_window": "act",
    "open_url": "act",
}

MEMBERS: dict[str, Kind] = {**TOOLSET_MEMBERS, **RELAY_MEMBERS}

_CLICKS: dict[str, tuple[Button, int]] = {
    "left_click": ("left", 1),
    "right_click": ("right", 1),
    "middle_click": ("middle", 1),
    "double_click": ("left", 2),
    "triple_click": ("left", 3),
}

_ALLOWED_PARAMS: dict[str, frozenset[str]] = {
    "screenshot": frozenset(),
    "zoom": frozenset({"region"}),
    "cursor_position": frozenset(),
    "wait": frozenset({"duration"}),
    "mouse_move": frozenset({"coordinate"}),
    "left_click_drag": frozenset({"start_coordinate", "coordinate", "text"}),
    "left_mouse_down": frozenset(),
    "left_mouse_up": frozenset(),
    "scroll": frozenset({"scroll_direction", "scroll_amount", "coordinate", "text"}),
    "type": frozenset({"text"}),
    "key": frozenset({"text", "repeat"}),
    "hold_key": frozenset({"text", "duration"}),
    **{name: frozenset({"coordinate", "text"}) for name in _CLICKS},
    "uia_tree": frozenset({"window", "depth", "filter", "root", "interactive"}),
    "uia_find": frozenset({"query", "role", "window"}),
    "uia_invoke": frozenset({"element"}),
    "uia_set_value": frozenset({"element", "value"}),
    "find_text": frozenset({"text", "region", "fuzzy"}),
    "list_apps": frozenset({"query", "refresh"}),
    "launch_app": frozenset({"name", "args"}),
    "list_windows": frozenset({"query"}),
    "focus_window": frozenset({"window"}),
    "open_url": frozenset({"url"}),
}

TAB_SETTLE = 0.3  # s for a new tab to take the address bar
PREVIEW_EDGE = 720  # px, long edge of the screenshot shown in the approval prompt
MAX_URL = 2000

# Region checked around a coordinate target for the "what changed" verdict (D-018).
CHANGE_RADIUS = 120  # physical px
CHANGE_SETTLE = 0.15  # s to let the UI react before the first comparison
CHANGE_POLL = 0.1  # s between further comparisons while nothing has changed yet
CHANGE_PATIENCE = 0.75  # s after the act before settling on "no visible change"
CHANGE_THRESHOLD = 24  # per-channel difference that counts as changed
CHANGE_MIN_FRACTION = 0.002  # share of pixels that must change


@dataclass
class ActionResult:
    text: str | None = None
    image: bytes | None = None  # PNG
    is_error: bool = False


@dataclass
class Computer:
    state: ScreenState
    grabber: Grabber
    driver: InputDriver | None
    report_changes: bool = True
    # Run before every act: the kill switch and approval gate (safety/). A hook raises
    # to stop the act, or returns a label for the audit log (e.g. "approved").
    before_act: list[Callable[[ActRequest], str | None]] = field(default_factory=list)
    sleep: Callable[[float], None] = time.sleep
    audit: AuditLog | None = None
    uia: UiaService | None = None
    ocr: OcrService | None = None
    apps: AppCatalog | None = None

    # -- public ---------------------------------------------------------------

    def run(
        self,
        name: str,
        params: Mapping[str, Any] | None = None,
        step: int | None = None,
        total: int | None = None,
    ) -> ActionResult:
        params = dict(params or {})
        if name not in MEMBERS:
            raise ActionError(f"unknown action {name!r}")
        unknown = set(params) - _ALLOWED_PARAMS[name]
        if unknown:
            raise ActionError(f"{name} does not take {', '.join(sorted(unknown))}")
        is_act = MEMBERS[name] == "act"
        start = time.monotonic()
        entry: dict[str, Any] = {"action": name, "input": params}
        if step is not None:
            entry["batch_step"] = f"{step}/{total}"
        try:
            if is_act:
                targets = self._targets(name, params)
                request = ActRequest(
                    name,
                    params,
                    step,
                    total,
                    preview=lambda: self._preview(targets),
                    summary=self._summary(name, params),
                )
                labels = [label for hook in self.before_act if (label := hook(request))]
                entry["approval"] = "; ".join(labels) or None
                entry["before"] = self._audit_thumbnail("before")
            result = self._dispatch(name, params)
        except Exception as exc:
            entry.update(error=str(exc) or type(exc).__name__, duration_ms=elapsed_ms(start))
            self._audit(entry)
            raise
        entry.update(result=result.text, duration_ms=elapsed_ms(start))
        if is_act:
            entry["after"] = self._audit_thumbnail("after")
        self._audit(entry)
        return result

    def run_batch(self, actions: Sequence[Any]) -> list[ActionResult]:
        results: list[ActionResult] = []
        failed = False
        total = len(actions)
        for step, action in enumerate(actions, start=1):
            if failed:
                results.append(ActionResult(HALT_TEXT, is_error=True))
                continue
            try:
                name = action.get("name") if isinstance(action, Mapping) else None
                params = (action.get("input") or {}) if isinstance(action, Mapping) else None
                if not isinstance(name, str) or not isinstance(params, Mapping):
                    raise ActionError("each action needs a string 'name' and an object 'input'")
                results.append(self.run(name, params, step, total))
            except Exception as exc:  # any failure stops the batch
                failed = True
                message = str(exc)
                if not isinstance(exc, ActionError):
                    message = f"{type(exc).__name__}: {exc}"
                results.append(ActionResult(message, is_error=True))
        return results

    def _dispatch(self, name: str, params: dict[str, Any]) -> ActionResult:
        try:
            return getattr(self, f"_do_{name}")(**params)
        except ActionError:
            raise
        except (ValueError, TypeError) as exc:
            raise ActionError(f"{name}: {exc}") from exc

    # -- safety helpers -------------------------------------------------------

    def _audit(self, entry: dict[str, Any]) -> None:
        if self.audit is not None:
            entry = {"seq": self.audit.next_seq(), **entry}
            self.audit.record({k: v for k, v in entry.items() if v is not None})

    def _audit_thumbnail(self, label: str) -> str | None:
        if self.audit is None:
            return None
        try:
            image = self.grabber.grab(self._geometry().display)
            return self.audit.save_thumbnail(self.audit.next_seq(), label, image)
        except Exception:  # the log must never stop an action
            return None

    def _targets(self, name: str, params: Mapping[str, Any]) -> list[tuple[int, int]]:
        """Screenshot-pixel points an act aims at, for the approval preview."""
        if name.startswith("uia_"):
            element = params.get("element")
            if self.uia is None or not isinstance(element, str):
                return []
            point = uia_center(self._geometry(), self.uia.rect(element))
            return [point] if point else []
        points = []
        for key in ("start_coordinate", "coordinate"):
            point = params.get(key)
            if (
                isinstance(point, (list, tuple))
                and len(point) == 2
                and all(isinstance(v, int) for v in point)
            ):
                points.append((point[0], point[1]))
        return points

    def _summary(self, name: str, params: Mapping[str, Any]) -> str | None:
        """Plain words for acts the approval gate cannot describe from params alone."""
        if name == "launch_app":
            app = self._apps().resolve(str(params.get("name", "")))
            args = _args(params.get("args"))
            shown = " ".join(json.dumps(a, ensure_ascii=False) for a in args)
            return f"Open {app.name}" + (f" with {shown[:300]}" if args else "")
        if name == "open_url":
            browser = self._apps().resolve("browser")
            return f"Open {_web_url(params.get('url'))} in {browser.name}"
        if name == "focus_window":
            catalog = self._apps()
            window = app_launch.find_window(
                catalog.backend.windows(), str(params.get("window", "")), catalog
            )
            return f"Bring {json.dumps(window.title, ensure_ascii=False)} to the front"
        if name in CLICKS:
            return self._click_summary(name, params)
        if name not in ("uia_invoke", "uia_set_value"):
            return None
        label = self._uia().label(str(params.get("element", "")))
        if name == "uia_invoke":
            return f"Activate {label}"
        value = str(params.get("value", ""))
        shown = value if len(value) <= 300 else value[:300] + "…"
        return f"Set {label} to {json.dumps(shown, ensure_ascii=False)}"

    def _click_summary(self, name: str, params: Mapping[str, Any]) -> str | None:
        """'Left click at (x, y) on Button "Send"', so the gate can tell risky clicks."""
        point = params.get("coordinate")
        if self.uia is None or not (
            isinstance(point, (list, tuple))
            and len(point) == 2
            and all(isinstance(v, int) for v in point)
        ):
            return None
        try:
            x, y = self._geometry().to_physical(point[0], point[1])
            label = self.uia.label_at(x, y)
        except Exception:  # the summary is a nicety; never block the click on it
            return None
        return f"{describe(name, params)} on {label}" if label else None

    def _preview(self, points: list[tuple[int, int]]) -> bytes | None:
        """The active display, small, with the target point(s) circled in red."""
        try:
            geometry = self._geometry()
            image = capture.screenshot(self.grabber, geometry).convert("RGB")
            draw = ImageDraw.Draw(image)
            for x, y in points:
                draw.ellipse((x - 14, y - 14, x + 14, y + 14), outline=(230, 0, 0), width=4)
            image.thumbnail((PREVIEW_EDGE, PREVIEW_EDGE))
            return capture.encode_png(image)
        except Exception:
            return None

    # -- helpers --------------------------------------------------------------

    def _input(self) -> InputDriver:
        if self.driver is None:
            raise ActionError("input is not available in this server")
        return self.driver

    def _geometry(self) -> ScreenshotGeometry:
        return self.state.geometry()

    def _point(self, geometry: ScreenshotGeometry, coordinate: Any, label: str) -> tuple[int, int]:
        if (
            not isinstance(coordinate, (list, tuple))
            or len(coordinate) != 2
            or not all(isinstance(v, int) and not isinstance(v, bool) for v in coordinate)
        ):
            raise ActionError(f"{label} must be [x, y] integers in screenshot pixels")
        x, y = coordinate
        try:
            geometry.check_point(x, y)
        except ValueError as exc:
            raise ActionError(f"{label}: {exc}") from exc
        return geometry.to_physical(x, y)

    def _observe(self, area: Rect | None) -> Image.Image | None:
        if not self.report_changes:
            return None
        geometry = self._geometry()
        rect = geometry.display if area is None else (area.intersect(geometry.display) or area)
        image = self.grabber.grab(rect)
        if area is None:  # whole display: compare a thumbnail
            image = image.resize((max(1, rect.width // 8), max(1, rect.height // 8)))
        return image

    def _verdict(self, before: Image.Image | None, area: Rect | None) -> str:
        if before is None:
            return "OK"
        # Shell surfaces animate (Start menu, flyouts), so one early look can miss a change.
        waited = CHANGE_SETTLE
        self.sleep(CHANGE_SETTLE)
        while True:
            after = self._observe(area)
            if after is None or after.size != before.size:
                return "OK"
            diff = ImageChops.difference(before.convert("RGB"), after.convert("RGB")).convert("L")
            changed = sum(diff.histogram()[CHANGE_THRESHOLD:])
            if changed / (before.size[0] * before.size[1]) >= CHANGE_MIN_FRACTION:
                return "OK · target changed" if area is not None else "OK · screen changed"
            if waited + CHANGE_POLL > CHANGE_PATIENCE + 1e-9:
                return "OK · no visible change"
            self.sleep(CHANGE_POLL)
            waited += CHANGE_POLL

    @staticmethod
    def _around(point: tuple[int, int]) -> Rect:
        x, y = point
        return Rect(x - CHANGE_RADIUS, y - CHANGE_RADIUS, 2 * CHANGE_RADIUS, 2 * CHANGE_RADIUS)

    def _act(self, area: Rect | None, action: Callable[[], object]) -> ActionResult:
        before = self._observe(area)
        action()
        return ActionResult(self._verdict(before, area))

    # -- members --------------------------------------------------------------

    def _do_screenshot(self) -> ActionResult:
        image = capture.screenshot(self.grabber, self._geometry())
        return ActionResult(image=capture.encode_png(image))

    def _do_zoom(self, region: Any) -> ActionResult:
        if (
            not isinstance(region, (list, tuple))
            or len(region) != 4
            or not all(isinstance(v, int) and not isinstance(v, bool) for v in region)
        ):
            raise ActionError("region must be [x0, y0, x1, y1] integers in screenshot pixels")
        x0, y0, x1, y1 = region
        image = capture.zoom(self.grabber, self._geometry(), (x0, y0, x1, y1))
        return ActionResult(image=capture.encode_png(image))

    def _do_cursor_position(self) -> ActionResult:
        geometry = self._geometry()
        x, y = self._input().backend.cursor()
        if not geometry.display.contains(x, y):
            return ActionResult(f"cursor is on another display at physical ({x}, {y})")
        sx, sy = geometry.to_screenshot(x, y)
        return ActionResult(f"X={sx}, Y={sy}")

    def _do_wait(self, duration: Any) -> ActionResult:
        seconds = _duration(duration)
        self.sleep(seconds)
        return ActionResult("OK")

    def _do_mouse_move(self, coordinate: Any) -> ActionResult:
        point = self._point(self._geometry(), coordinate, "coordinate")
        self._input().move(*point)
        return ActionResult("OK")

    def _click(self, name: str, coordinate: Any = None, text: str | None = None) -> ActionResult:
        button, count = _CLICKS[name]
        modifiers = parse_modifiers(text)
        geometry = self._geometry()
        if coordinate is None:
            point = None
            cursor = self._input().backend.cursor()
            area = self._around(cursor) if geometry.display.contains(*cursor) else None
        else:
            point = self._point(geometry, coordinate, "coordinate")
            area = self._around(point)
        return self._act(area, lambda: self._input().click(point, button, count, modifiers))

    def _do_left_click(self, coordinate: Any = None, text: str | None = None) -> ActionResult:
        return self._click("left_click", coordinate, text)

    def _do_right_click(self, coordinate: Any = None, text: str | None = None) -> ActionResult:
        return self._click("right_click", coordinate, text)

    def _do_middle_click(self, coordinate: Any = None, text: str | None = None) -> ActionResult:
        return self._click("middle_click", coordinate, text)

    def _do_double_click(self, coordinate: Any = None, text: str | None = None) -> ActionResult:
        return self._click("double_click", coordinate, text)

    def _do_triple_click(self, coordinate: Any = None, text: str | None = None) -> ActionResult:
        return self._click("triple_click", coordinate, text)

    def _do_left_click_drag(
        self, start_coordinate: Any, coordinate: Any, text: str | None = None
    ) -> ActionResult:
        geometry = self._geometry()
        start = self._point(geometry, start_coordinate, "start_coordinate")
        end = self._point(geometry, coordinate, "coordinate")
        modifiers = parse_modifiers(text)
        return self._act(None, lambda: self._input().drag(start, end, modifiers))

    def _do_left_mouse_down(self) -> ActionResult:
        self._input().mouse_down("left")
        return ActionResult("OK")

    def _do_left_mouse_up(self) -> ActionResult:
        return self._act(None, lambda: self._input().mouse_up("left"))

    def _do_scroll(
        self,
        scroll_direction: Any,
        scroll_amount: Any,
        coordinate: Any = None,
        text: str | None = None,
    ) -> ActionResult:
        if not isinstance(scroll_amount, int) or isinstance(scroll_amount, bool):
            raise ActionError("scroll_amount must be an integer")
        geometry = self._geometry()
        point = None if coordinate is None else self._point(geometry, coordinate, "coordinate")
        modifiers = parse_modifiers(text)
        area = self._around(point) if point is not None else None
        return self._act(
            area,
            lambda: self._input().scroll(str(scroll_direction), scroll_amount, point, modifiers),
        )

    def _do_type(self, text: Any) -> ActionResult:
        if not isinstance(text, str) or not text:
            raise ActionError("text must be a non-empty string")
        return self._act(None, lambda: self._input().type_text(text))

    def _do_key(self, text: Any, repeat: Any = 1) -> ActionResult:
        if not isinstance(text, str):
            raise ActionError("text must be a key name such as 'Return' or 'ctrl+s'")
        if not isinstance(repeat, int) or isinstance(repeat, bool):
            raise ActionError("repeat must be an integer")
        return self._act(None, lambda: self._input().key(text, repeat))

    def _do_hold_key(self, text: Any, duration: Any) -> ActionResult:
        if not isinstance(text, str):
            raise ActionError("text must be a key name such as 'shift'")
        seconds = _duration(duration)
        return self._act(None, lambda: self._input().hold_key(text, seconds))

    # -- UI Automation (Relay-only) -------------------------------------------

    def _uia(self) -> UiaService:
        if self.uia is None:
            raise ActionError("UI Automation is not available in this server")
        return self.uia

    def _do_uia_tree(
        self,
        window: str | None = None,
        depth: int = 6,
        filter: str | None = None,
        root: str | None = None,
        interactive: bool = False,
    ) -> ActionResult:
        if not isinstance(depth, int) or isinstance(depth, bool):
            raise ActionError("depth must be an integer")
        displays = self.state.displays()
        listing = self._uia().tree(
            self._geometry(), window, depth, filter, displays, root, bool(interactive)
        )
        return ActionResult(listing)

    def _do_uia_find(
        self, query: str, role: str | None = None, window: str | None = None
    ) -> ActionResult:
        if not isinstance(query, str):
            raise ActionError("query must be text")
        displays = self.state.displays()
        return ActionResult(self._uia().find(self._geometry(), query, role, window, displays))

    def _element_area(self, element: str) -> Rect | None:
        rect = self._uia().rect(element)
        if rect is None:
            return None
        return self._around((rect.left + rect.width // 2, rect.top + rect.height // 2))

    def _do_uia_invoke(self, element: str) -> ActionResult:
        uia = self._uia()
        area = self._element_area(element)
        done: list[str] = []
        result = self._act(area, lambda: done.append(uia.invoke(element)))
        return ActionResult(f"{done[0].capitalize()} · {result.text}" if done else result.text)

    def _do_uia_set_value(self, element: str, value: str) -> ActionResult:
        if not isinstance(value, str):
            raise ActionError("value must be text")
        uia = self._uia()
        area = self._element_area(element)
        return self._act(area, lambda: uia.set_value(element, value))

    # -- OCR (Relay-only) -----------------------------------------------------

    def _do_find_text(
        self, text: str | None = None, region: Any = None, fuzzy: bool = True
    ) -> ActionResult:
        if self.ocr is None:
            raise ActionError("OCR is not available in this server")
        if text is not None and not isinstance(text, str):
            raise ActionError("text must be text")
        box = None
        if region is not None:
            if (
                not isinstance(region, (list, tuple))
                or len(region) != 4
                or not all(isinstance(v, int) and not isinstance(v, bool) for v in region)
            ):
                raise ActionError("region must be [x0, y0, x1, y1] integers in screenshot pixels")
            box = (region[0], region[1], region[2], region[3])
        return ActionResult(self.ocr.find_text(self._geometry(), text, box, bool(fuzzy)))

    # -- apps (Relay-only, D-030) ---------------------------------------------

    def _apps(self) -> AppCatalog:
        if self.apps is None:
            raise ActionError("app discovery is not available in this server")
        return self.apps

    def _do_list_apps(self, query: str | None = None, refresh: bool = False) -> ActionResult:
        if query is not None and not isinstance(query, str):
            raise ActionError("query must be text")
        return ActionResult(self._apps().listing(query, bool(refresh)))

    def _do_launch_app(self, name: str, args: Any = None) -> ActionResult:
        if not isinstance(name, str) or not name.strip():
            raise ActionError("name must be an app name from list_apps")
        catalog = self._apps()
        app = catalog.resolve(name)
        try:
            window, in_front = app_launch.launch(catalog.backend, app, _args(args), self.sleep)
        except OSError as exc:
            raise ActionError(f"could not open {app.name}: {exc}") from exc
        if window is None:
            return ActionResult(
                f"Started {app.name}, but no window of it appeared within "
                f"{app_launch.WINDOW_PATIENCE:g} s. Check with screenshot."
            )
        text = f"Opened {app.name} · window {json.dumps(window.title, ensure_ascii=False)}"
        text += " in front" if in_front else " (behind other windows)"
        return ActionResult(text + self._display_note(window))

    def _do_open_url(self, url: str) -> ActionResult:
        """Open a web page on screen in the owner's own browser (D-079).

        A browser that is already open gets a new tab, or the same tab when it already
        shows that site; the browser is started only when none is open (D-095).
        """
        address = _web_url(url)
        catalog = self._apps()
        browser = catalog.resolve("browser")
        opened = self._open_in_open_browser(catalog, browser, address)
        if opened is not None:
            return opened
        try:
            window, in_front = app_launch.launch(catalog.backend, browser, [address], self.sleep)
        except OSError as exc:
            raise ActionError(f"could not open {browser.name}: {exc}") from exc
        if window is None:
            return ActionResult(
                f"Asked {browser.name} to open {address}, but no window of it changed "
                f"within {app_launch.WINDOW_PATIENCE:g} s. Check with screenshot."
            )
        text = f"Opened {address} in {browser.name} · window "
        text += json.dumps(window.title, ensure_ascii=False)
        text += " in front" if in_front else " (behind other windows; focus_window it)"
        return ActionResult(text + self._display_note(window))

    def _open_in_open_browser(
        self, catalog: AppCatalog, browser: app_launch.App, address: str
    ) -> ActionResult | None:
        backend = catalog.backend
        open_now = [
            w
            for w in backend.windows()
            if not app_launch.is_relay(w)
            and not app_launch.minimized(w)
            and (app_launch.window_stem(w) or "") in app_launch.BROWSERS
        ]
        if not open_now:
            return None
        same = [w for w in open_now if app_launch.window_stem(w) == browser.stem]
        target = (same or open_now)[0]  # the owner's browser, topmost window
        if backend.foreground() != target.handle and not backend.bring_to_front(target.handle):
            return None  # Windows kept it behind; starting the browser still works
        self.sleep(app_launch.WINDOW_SETTLE)
        keys = self._input()
        shown = self._address_bar(target.handle)
        if shown is None or _site(shown[1]) != _site(address):
            keys.key("ctrl+t", 1)  # a new tab, never a new window
            self.sleep(TAB_SETTLE)
            shown = self._address_bar(target.handle)
            how = "in a new tab"
        else:
            how = "in the tab that already showed that site"
        keys.key("ctrl+l", 1)  # the address bar has focus for Enter either way
        set_by_uia = False
        if shown is not None:
            try:
                self._uia().set_value(shown[0], address)
                set_by_uia = True
            except ActionError:
                set_by_uia = False
        if not set_by_uia:
            keys.type_text(address)
        keys.key("Return", 1)
        window = next((w for w in backend.windows() if w.handle == target.handle), target)
        name = PureWindowsPath(target.exe).stem.capitalize() if target.exe else "the browser"
        text = f"Opened {address} {how} of {name} · window "
        text += json.dumps(window.title, ensure_ascii=False) + " in front"
        return ActionResult(text + self._display_note(window))

    def _address_bar(self, handle: int) -> tuple[str, str] | None:
        """The browser window's address bar: its element id and the address it shows."""
        if self.uia is None:
            return None
        window = next((w for w in self._apps().backend.windows() if w.handle == handle), None)
        if window is None:
            return None
        try:
            found = self._uia().find(
                self._geometry(),
                app_launch.ADDRESS_BAR,
                "Edit",
                window.title,
                self.state.displays(),
            )
        except ActionError:
            return None
        match = re.search(r"^\[(e\d+)\] Edit[^\n]*?value='([^']*)'", found, re.MULTILINE)
        if match is None:
            match = re.search(r"^\[(e\d+)\] Edit", found, re.MULTILINE)
            return (match.group(1), "") if match else None
        return match.group(1), match.group(2)

    def _display_note(self, window: app_launch.Window) -> str:
        cx = window.rect.left + window.rect.width // 2
        cy = window.rect.top + window.rect.height // 2
        display = display_at(self.state.displays(), cx, cy)
        if display is not None and display.id != self.state.active().id:
            # Follow the window: screenshots and coordinates now use its display (D-093).
            self.state.select(display.id)
            return f" on display {display.id}, now the active display"
        return ""

    def _do_list_windows(self, query: str | None = None) -> ActionResult:
        if query is not None and not isinstance(query, str):
            raise ActionError("query must be text")
        backend = self._apps().backend
        return ActionResult(
            app_launch.window_listing(backend.windows(), backend.foreground(), query)
        )

    def _do_focus_window(self, window: str) -> ActionResult:
        """Find an open window by title or program and bring it in front (D-078)."""
        if not isinstance(window, str) or not window.strip():
            raise ActionError("window must be a window title or program name from list_windows")
        catalog = self._apps()
        backend = catalog.backend
        target = app_launch.find_window(backend.windows(), window, catalog)
        in_front = backend.foreground() == target.handle or backend.bring_to_front(target.handle)
        if in_front:
            self.sleep(app_launch.WINDOW_SETTLE)  # let it restore and repaint
        title = json.dumps(target.title, ensure_ascii=False)
        if not in_front:
            return ActionResult(
                f"Found {title} but Windows kept it behind other windows. "
                "Take a screenshot; click its title bar if it is visible."
            )
        return ActionResult(f"{title} is in front" + self._display_note(target))


def _site(address: str) -> str:
    """'dice.com' for 'https://www.dice.com/jobs' or 'dice.com/jobs'; '' for no address."""
    text = address.strip()
    if not text:
        return ""
    if "://" not in text:
        text = "https://" + text
    host = urllib.parse.urlparse(text).hostname or ""
    return host.removeprefix("www.")


def _web_url(value: Any) -> str:
    """An http(s) address; a bare "dice.com" gets https://."""
    if not isinstance(value, str) or not value.strip() or len(value) > MAX_URL:
        raise ActionError("url must be a web address like https://www.dice.com")
    address = value.strip()
    if "://" not in address:
        if re.match(r"[a-z][a-z0-9+.-]*:(?!\d)", address, re.IGNORECASE):
            raise ActionError("url must be an http or https web address")  # mailto:, javascript:
        address = "https://" + address
    scheme, _, rest = address.partition("://")
    if scheme.casefold() not in ("http", "https") or not rest or any(c.isspace() for c in rest):
        raise ActionError("url must be an http or https web address")
    return address


def _args(value: Any) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, (list, tuple)) or not all(isinstance(a, str) for a in value):
        raise ActionError("args must be a list of strings")
    if len(value) > MAX_ARGS:
        raise ActionError(f"args takes at most {MAX_ARGS} items")
    return list(value)


def _duration(value: Any) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ActionError("duration must be a number of seconds")
    if not 0 < value <= MAX_DURATION:
        raise ActionError(f"duration must be between 0 and {MAX_DURATION:g} seconds")
    return float(value)
