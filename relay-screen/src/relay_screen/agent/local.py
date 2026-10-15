"""The local operator: a vision model on the owner's GPU plans and runs UI tasks (D-067).

Relay's local model drives the mouse, keyboard and apps by itself when Claude is not
available (signed out, offline, out of quota). Each step it sees a fresh screenshot,
answers with one JSON action, and Relay runs that action through the same `Computer`
Claude uses, so the kill switch, approval prompt and audit log all apply.
"""

from __future__ import annotations

import base64
import io
import json
import re
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from typing import Any, Literal, Protocol

from PIL import Image

from relay_screen.actions import ActionResult
from relay_screen.errors import ActionError
from relay_screen.runtime.catalog import Coords
from relay_screen.safety.approval import ApprovalDeniedError, PromptUnavailableError
from relay_screen.safety.killswitch import KillSwitchError

MAX_STEPS = 25
HISTORY = 10  # earlier steps repeated to the model as text
SEND_EDGE = 1280  # px, long edge of the screenshot sent to the model
MAX_BAD_REPLIES = 3
STUCK_REPEATS = 3  # the same action with no visible change this many times in a row
MAX_TEXT = 2000

SYSTEM_PROMPT = """\
You are Relay, an assistant that operates a Windows PC for its owner. You see a \
screenshot of the screen and the owner's task. Work toward the task one action at a \
time. After each action you get a new screenshot.

Reply with exactly one JSON object and nothing else:
{"thought": "<one short sentence>", "action": "<name>", ...parameters}

Actions:
- {"action": "click", "x": X, "y": Y}  (also "double_click", "right_click")
- {"action": "type", "text": "..."}  types into the focused field
- {"action": "key", "keys": "ctrl+s"}  key names like Return, Tab, Escape, BackSpace, \
ctrl+l, alt+F4, super
- {"action": "scroll", "x": X, "y": Y, "direction": "down", "amount": 3}
- {"action": "launch_app", "name": "Notepad", "args": []}  opens an installed app; \
name "browser" opens the owner's web browser, and a URL in args opens that page
- {"action": "wait", "seconds": 2}
- {"action": "done", "summary": "..."}  the task is finished; say what you did
- {"action": "fail", "reason": "..."}  you cannot finish; say why

X and Y are positions on the screenshot from 0 to 1000: 0,0 is the top left corner \
and 1000,1000 the bottom right.

Rules:
- Click a field before typing into it. Prefer keyboard shortcuts when they are certain.
- Never type a password and never sign in for the owner. If a sign-in page blocks the \
task, reply with fail and the reason "sign-in needed".
- Only buy, pay, send, submit or delete when the task clearly asks for it. Relay asks \
the owner before such steps either way.
- If an action did not change the screen, try something different.
"""

Status = Literal["done", "failed", "stopped"]


class Model(Protocol):
    def __call__(self, messages: list[dict[str, Any]]) -> str: ...


class Hands(Protocol):
    """The part of `actions.Computer` the agent uses."""

    def run(self, name: str, params: Mapping[str, Any] | None = None) -> ActionResult: ...


@dataclass
class Step:
    n: int
    thought: str
    action: dict[str, Any]
    result: str
    error: bool = False

    def line(self) -> str:
        return f"Step {self.n}: {describe(self.action)} -> {self.result}"


@dataclass
class Outcome:
    status: Status
    message: str
    steps: list[Step] = field(default_factory=list)


class ReplyError(ValueError):
    """The model's reply was not one usable action."""


def parse_reply(text: str) -> dict[str, Any]:
    """The action object in a model reply, tolerating code fences and stray prose."""
    text = re.sub(r"```(?:json)?", "", text)
    start = text.find("{")
    if start < 0:
        raise ReplyError("no JSON object in the reply")
    try:
        value, _ = json.JSONDecoder().raw_decode(text[start:])
    except ValueError as exc:
        raise ReplyError(f"the JSON did not parse: {exc}") from exc
    if not isinstance(value, dict) or not isinstance(value.get("action"), str):
        raise ReplyError('the reply needs an "action" name')
    if "coordinate" in value and "x" not in value:
        x, y = _pair(value.pop("coordinate"))
        value.update(x=x, y=y)
    value["action"] = value["action"].strip().lower()
    return value


def describe(action: Mapping[str, Any]) -> str:
    name = action.get("action", "?")
    if "x" in action and "y" in action:
        return f"{name} at ({action['x']}, {action['y']})"
    for key in ("text", "keys", "name", "seconds", "summary", "reason"):
        if key in action:
            return f"{name} {json.dumps(action[key])[:80]}"
    return str(name)


def _pair(value: Any) -> tuple[float, float]:
    if isinstance(value, list | tuple) and len(value) == 2:
        return float(value[0]), float(value[1])
    raise ReplyError("a coordinate needs two numbers")


@dataclass(frozen=True)
class Frame:
    """One screenshot: the PNG Relay captured and the smaller image sent to the model."""

    width: int
    height: int
    sent: bytes
    sent_size: tuple[int, int]

    @classmethod
    def from_png(cls, png: bytes, edge: int = SEND_EDGE) -> Frame:
        image = Image.open(io.BytesIO(png)).convert("RGB")
        width, height = image.size
        scale = min(1.0, edge / max(width, height))
        if scale < 1.0:
            image = image.resize(
                (round(width * scale), round(height * scale)), Image.Resampling.LANCZOS
            )
        out = io.BytesIO()
        image.save(out, "JPEG", quality=85)
        return cls(width, height, out.getvalue(), image.size)

    def data_url(self) -> str:
        return "data:image/jpeg;base64," + base64.b64encode(self.sent).decode("ascii")

    def to_screen(self, x: Any, y: Any, coords: Coords) -> list[int]:
        """Model coordinates -> screenshot pixels of the active display (D-009)."""
        try:
            fx, fy = float(x), float(y)
        except (TypeError, ValueError) as exc:
            raise ReplyError("x and y must be numbers") from exc
        if coords == "rel1000":
            px, py = fx / 1000 * self.width, fy / 1000 * self.height
        else:
            px = fx * self.width / self.sent_size[0]
            py = fy * self.height / self.sent_size[1]
        return [
            min(max(round(px), 0), self.width - 1),
            min(max(round(py), 0), self.height - 1),
        ]


def to_member(action: Mapping[str, Any], frame: Frame, coords: Coords) -> tuple[str, dict]:
    """Map a model action onto a computer toolset member and its input."""
    name = action["action"]
    if name in ("click", "left_click", "double_click", "right_click"):
        member = "left_click" if name in ("click", "left_click") else name
        return member, {"coordinate": frame.to_screen(action.get("x"), action.get("y"), coords)}
    if name == "type":
        text = action.get("text")
        if not isinstance(text, str) or not text:
            raise ReplyError("type needs text")
        return "type", {"text": text[:MAX_TEXT]}
    if name in ("key", "hotkey", "press"):
        keys = action.get("keys") or action.get("key") or action.get("text")
        if not isinstance(keys, str) or not keys:
            raise ReplyError("key needs keys, for example ctrl+s")
        return "key", {"text": keys}
    if name == "scroll":
        direction = str(action.get("direction", "down")).lower()
        if direction not in ("up", "down", "left", "right"):
            raise ReplyError("scroll direction must be up, down, left or right")
        params: dict[str, Any] = {
            "scroll_direction": direction,
            "scroll_amount": int(action.get("amount", 3)),
        }
        if "x" in action and "y" in action:
            params["coordinate"] = frame.to_screen(action["x"], action["y"], coords)
        return "scroll", params
    if name in ("launch_app", "open_app", "open"):
        app = action.get("name") or action.get("app")
        if not isinstance(app, str) or not app:
            raise ReplyError("launch_app needs a name")
        args = action.get("args") or []
        if isinstance(args, str):
            args = [args]
        return "launch_app", {"name": app, "args": [str(a) for a in args]}
    if name == "wait":
        return "wait", {"duration": min(float(action.get("seconds", 1)), 10.0)}
    raise ReplyError(f"unknown action {name!r}")


Event = Callable[[dict[str, Any]], None]


class LocalAgent:
    def __init__(
        self,
        hands: Hands,
        model: Model,
        *,
        coords: Coords = "rel1000",
        max_steps: int = MAX_STEPS,
        on_event: Event | None = None,
    ) -> None:
        self.hands = hands
        self.model = model
        self.coords: Coords = coords
        self.max_steps = max_steps
        self.emit = on_event or (lambda event: None)

    def run(self, task: str) -> Outcome:
        steps: list[Step] = []
        bad_replies = 0
        note = ""
        for n in range(1, self.max_steps + 1):
            frame = Frame.from_png(self._screenshot())
            reply = self.model(self._messages(task, steps, frame, note))
            note = ""
            try:
                action = parse_reply(reply)
                thought = str(action.pop("thought", ""))
                if action["action"] == "done":
                    return self._finish("done", str(action.get("summary") or "Done."), steps)
                if action["action"] == "fail":
                    reason = str(action.get("reason") or "The local model gave up.")
                    return self._finish("failed", reason, steps)
                member, params = to_member(action, frame, self.coords)
            except (ReplyError, ValueError) as exc:
                bad_replies += 1
                self.emit({"event": "bad_reply", "step": n, "error": str(exc)})
                if bad_replies >= MAX_BAD_REPLIES:
                    message = f"The local model's replies could not be used ({exc})."
                    return self._finish("failed", message, steps)
                note = f"Your last reply could not be used: {exc}. Reply with one JSON object."
                continue
            bad_replies = 0
            step = self._act(n, thought, action, member, params)
            steps.append(step)
            if step.error and step.result.startswith("STOP:"):
                return self._finish("stopped", step.result.removeprefix("STOP: "), steps)
            if self._stuck(steps):
                message = "The same action kept changing nothing, so Relay stopped."
                return self._finish("failed", message, steps)
        return self._finish("failed", f"Not finished after {self.max_steps} steps.", steps)

    def _screenshot(self) -> bytes:
        shot = self.hands.run("screenshot")
        if shot.image is None:
            raise ActionError("the screenshot came back empty")
        return shot.image

    def _messages(
        self, task: str, steps: list[Step], frame: Frame, note: str
    ) -> list[dict[str, Any]]:
        lines = [f"Task: {task}"]
        if steps:
            lines.append("What you did so far:")
            lines.extend(s.line() for s in steps[-HISTORY:])
        if note:
            lines.append(note)
        lines.append("Here is the screen now. What is the next action?")
        return [
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "\n".join(lines)},
                    {"type": "image_url", "image_url": {"url": frame.data_url()}},
                ],
            },
        ]

    def _act(self, n: int, thought: str, action: dict[str, Any], member: str, params: dict) -> Step:
        try:
            result = self.hands.run(member, params)
            text, error = result.text or "done", result.is_error
        except (KillSwitchError, ApprovalDeniedError, PromptUnavailableError) as exc:
            text, error = f"STOP: {exc}", True
        except ActionError as exc:
            text, error = f"failed: {exc}", True
        step = Step(n, thought, action, text, error)
        self.emit(
            {
                "event": "step",
                "step": n,
                "thought": thought,
                "action": describe(action),
                "result": text,
            }
        )
        return step

    @staticmethod
    def _stuck(steps: list[Step]) -> bool:
        last = steps[-STUCK_REPEATS:]
        if len(last) < STUCK_REPEATS:
            return False
        same = all(s.action == last[0].action for s in last)
        return same and all("no visible change" in s.result for s in last)

    def _finish(self, status: Status, message: str, steps: list[Step]) -> Outcome:
        self.emit({"event": status, "message": message, "steps": len(steps)})
        return Outcome(status, message, steps)
