"""computer_toolset_20260801 member semantics on top of capture and input.

Each member takes the same parameters as the API toolset; coordinates are screenshot
pixels of the active display (D-009). ``run_batch`` implements the documented batch
rule: run in order, stop at the first failure, and answer every later action with the
exact halt text.
"""

from __future__ import annotations

import json
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any, Literal

from PIL import Image, ImageChops, ImageDraw

from relay_screen import capture
from relay_screen.capture import Grabber
from relay_screen.errors import ActionError
from relay_screen.input.driver import MAX_DURATION, InputDriver
from relay_screen.input.events import Button
from relay_screen.input.keys import parse_modifiers
from relay_screen.safety.approval import ActRequest
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
    "uia_tree": frozenset({"window", "depth", "filter"}),
    "uia_find": frozenset({"query", "role", "window"}),
    "uia_invoke": frozenset({"element"}),
    "uia_set_value": frozenset({"element", "value"}),
}

PREVIEW_EDGE = 720  # px, long edge of the screenshot shown in the approval prompt

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
        if name not in ("uia_invoke", "uia_set_value"):
            return None
        label = self._uia().label(str(params.get("element", "")))
        if name == "uia_invoke":
            return f"Activate {label}"
        value = str(params.get("value", ""))
        shown = value if len(value) <= 300 else value[:300] + "…"
        return f"Set {label} to {json.dumps(shown, ensure_ascii=False)}"

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
        self, window: str | None = None, depth: int = 6, filter: str | None = None
    ) -> ActionResult:
        if not isinstance(depth, int) or isinstance(depth, bool):
            raise ActionError("depth must be an integer")
        return ActionResult(self._uia().tree(self._geometry(), window, depth, filter))

    def _do_uia_find(
        self, query: str, role: str | None = None, window: str | None = None
    ) -> ActionResult:
        if not isinstance(query, str):
            raise ActionError("query must be text")
        return ActionResult(self._uia().find(self._geometry(), query, role, window))

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


def _duration(value: Any) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ActionError("duration must be a number of seconds")
    if not 0 < value <= MAX_DURATION:
        raise ActionError(f"duration must be between 0 and {MAX_DURATION:g} seconds")
    return float(value)
