"""The local operator: a vision model on the owner's GPU plans and runs UI tasks (D-067).

Relay's local model drives the mouse, keyboard and apps by itself when Claude is not
available (signed out, offline, out of quota). Each step it sees a fresh screenshot,
answers with one JSON action, and Relay runs that action through the same `Computer`
Claude uses, so the kill switch, approval prompt and audit log all apply.
"""

from __future__ import annotations

import base64
import contextlib
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
MAX_UIA_CHARS = 5000  # the focused window's UI Automation list, ~1.5k tokens
UIA_DEPTH = 8
FEW_ELEMENTS = 6  # with fewer element lines than this, read the screen's text with OCR
MAX_OCR_LINES = 40
MAX_APPS_CHARS = 2500
MAX_WINDOWS_CHARS = 1500

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
- {"action": "open_url", "url": "https://www.dice.com"}  opens a web page in the \
owner's browser and brings it to the front
- {"action": "focus_window", "window": "Brave"}  brings an open window to the front \
(restoring it if minimized) by its title or program name from the open windows list
- {"action": "wait", "seconds": 2}
- {"action": "invoke", "element": "e12"}  presses a button, menu item, link, checkbox or \
list item from the element list
- {"action": "set_value", "element": "e5", "value": "..."}  fills a text field from the \
element list in one go
- {"action": "click_text", "text": "Save"}  clicks text that is visible on the screen \
(a button, link, tab or menu item), found by Windows OCR. Use it when the element list \
does not have what you need
- {"action": "done", "summary": "..."}  the task is finished; say what you did
- {"action": "fail", "reason": "..."}  you cannot finish; say why

X and Y are positions on the screenshot from 0 to 1000: 0,0 is the top left corner \
and 1000,1000 the bottom right.

When an element list of the focused window is given, prefer invoke and set_value \
with its ids over clicking and typing: they are faster and do not miss. Each line is \
[id] Role "name" ... {what it can do}. Next best is click_text with words you can \
read on the screen. Click by position only when neither works.

The open windows are listed, topmost first. When the program the task needs is \
already open, focus_window it; open it with launch_app only when no window of it is \
open. The installed apps are listed; open them by those names.

Rules:
- Click a field before typing into it. Prefer keyboard shortcuts when they are certain.
- Never type a password and never sign in for the owner. If a sign-in page blocks the \
task, reply with fail and the reason "sign-in needed".
- Only buy, pay, send, submit or delete when the task clearly asks for it. Relay asks \
the owner before such steps either way.
- If an action did not change the screen, try something different.
- To go to a website, use open_url instead of typing in the address bar.
- Never click taskbar buttons to switch programs (that minimizes them); use \
focus_window. Never act on Relay's own window; it is marked in the windows list.
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
    if "element" in action and "value" in action:
        return f"{name} {action['element']} = {json.dumps(action['value'])[:60]}"
    for key in ("element", "text", "keys", "name", "seconds", "summary", "reason"):
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
    if name in ("open_url", "go_to", "navigate", "open_website"):
        url = action.get("url") or action.get("address")
        if not isinstance(url, str) or not url.strip():
            raise ReplyError("open_url needs a url")
        return "open_url", {"url": url.strip()[:MAX_TEXT]}
    if name in ("focus_window", "focus", "switch_to"):
        window = action.get("window") or action.get("name") or action.get("title")
        if not isinstance(window, str) or not window.strip():
            raise ReplyError("focus_window needs a window title or program name")
        return "focus_window", {"window": window.strip()[:200]}
    if name in ("invoke", "set_value"):
        element = action.get("element") or action.get("id")
        if not isinstance(element, str) or not re.fullmatch(r"e\d+", element):
            raise ReplyError(f"{name} needs an element id like e12 from the element list")
        if name == "invoke":
            return "uia_invoke", {"element": element}
        value = action.get("value", action.get("text"))
        if not isinstance(value, str):
            raise ReplyError("set_value needs a value")
        return "uia_set_value", {"element": element, "value": value[:MAX_TEXT]}
    if name == "click_text":
        text = action.get("text")
        if not isinstance(text, str) or not text.strip():
            raise ReplyError("click_text needs the text to click")
        return "click_text", {"text": text.strip()[:200]}
    if name == "wait":
        return "wait", {"duration": min(float(action.get("seconds", 1)), 10.0)}
    raise ReplyError(f"unknown action {name!r}")


Event = Callable[[dict[str, Any]], None]


@dataclass(frozen=True)
class Context:
    """What the model gets besides the screenshot: UIA elements, OCR text, apps."""

    elements: str = ""
    screen_text: str = ""
    apps: str = ""
    windows: str = ""


URL_IN_TASK = re.compile(
    r"(?:https?://|www\.)[^\s\"'<>]+"
    r"|\b(?:go to|open|visit|navigate to)\s+([a-z0-9-]+(?:\.[a-z0-9-]+)*"
    r"\.(?:com|org|net|io|ai|dev|app|co|edu|gov|jobs|info|me|us|uk|ca|de|eg)"
    r"(?:/\S*)?)\b",
    re.IGNORECASE,
)


def _task_url(task: str) -> str | None:
    """The web address a task names ("go to dice.com and ..."), if any."""
    found = URL_IN_TASK.search(task)
    if found is None:
        return None
    return (found.group(1) or found.group(0)).rstrip(".,;:!?)")


def _few(elements: str) -> bool:
    return not elements.startswith("Elements") or elements.count("\n") < FEW_ELEMENTS


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
        apps = self._apps()
        self._focus_target(task)
        for n in range(1, self.max_steps + 1):
            windows = self._windows()
            frame = Frame.from_png(self._screenshot())
            elements = self._elements()
            screen_text = self._screen_text() if _few(elements) else ""
            context = Context(elements, screen_text, apps, windows)
            reply = self.model(self._messages(task, steps, frame, note, context))
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

    def _elements(self) -> str:
        """The focused window's UI Automation elements, or "" when UIA can't read it."""
        try:
            listing = self.hands.run("uia_tree", {"depth": UIA_DEPTH}).text or ""
        except Exception:
            return ""
        if len(listing) > MAX_UIA_CHARS:
            listing = listing[:MAX_UIA_CHARS].rsplit("\n", 1)[0] + "\n… (list cut)"
        return listing

    def _screen_text(self) -> str:
        """Lines of text on the screen (Windows OCR), for windows UIA can't read."""
        try:
            found = self.hands.run("find_text", {}).text or ""
        except Exception:
            return ""
        lines = [m.group(1) for m in re.finditer(r'^"(.*)" center', found, re.MULTILINE)]
        return "\n".join(lines[:MAX_OCR_LINES])

    def _apps(self) -> str:
        """Names of the installed apps, once per run."""
        try:
            listing = self.hands.run("list_apps", {}).text or ""
        except Exception:
            return ""
        names = listing.splitlines()[1:]
        text = ", ".join(n for n in names if not n.startswith("..."))
        return text[:MAX_APPS_CHARS]

    def _windows(self) -> str:
        """The open windows, topmost first, so the model knows where each program is."""
        try:
            listing = self.hands.run("list_windows", {}).text or ""
        except Exception:
            return ""
        head, _, rest = listing.partition("\n")
        if "open window(s)" not in head:
            return ""
        return rest[:MAX_WINDOWS_CHARS]

    def _focus_target(self, task: str) -> None:
        """Before the first step, bring the task's page or program to the front (D-078, D-079).

        Relay's own chat window is usually in front when a task starts, which left the
        model screenshotting Relay and clicking the taskbar. When an open window's
        program is named in the task ("in Brave, ..."), it goes in front first;
        otherwise the model sees the windows list and can focus_window itself.
        """
        url = _task_url(task)
        if url:  # "go to https://www.dice.com/ and ...": open it before the first look
            with contextlib.suppress(ActionError, ApprovalDeniedError, PromptUnavailableError):
                self.hands.run("open_url", {"url": url})
            return
        words = set(re.findall(r"[a-z0-9]+", task.casefold()))
        for line in self._windows().splitlines():
            parts = line.split(" · ")
            if len(parts) < 2 or any(p.startswith("Relay itself") for p in parts):
                continue
            stem = parts[1].casefold().removesuffix(".exe")
            if stem in words:
                if "in front" not in parts:
                    with contextlib.suppress(
                        ActionError, ApprovalDeniedError, PromptUnavailableError
                    ):
                        self.hands.run("focus_window", {"window": stem})
                return

    def _messages(
        self,
        task: str,
        steps: list[Step],
        frame: Frame,
        note: str,
        context: Context | None = None,
    ) -> list[dict[str, Any]]:
        context = context or Context()
        lines = [f"Task: {task}"]
        if context.apps:
            lines.append(f"Installed apps: {context.apps}")
        if context.windows:
            lines.append("Open windows (topmost first):")
            lines.append(context.windows)
        if steps:
            lines.append("What you did so far:")
            lines.extend(s.line() for s in steps[-HISTORY:])
        if note:
            lines.append(note)
        if context.elements.startswith("Elements"):
            lines.append("Element list of the focused window:")
            lines.append(context.elements)
        if context.screen_text:
            lines.append("Text on the screen (OCR), usable with click_text:")
            lines.append(context.screen_text)
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
            if member == "click_text":
                result = self._click_text(str(params["text"]))
            else:
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

    def _click_text(self, text: str) -> ActionResult:
        found = self.hands.run("find_text", {"text": text}).text or ""
        match = re.search(r"center \((\d+), (\d+)\)", found)
        if match is None:
            raise ActionError(f"no text like {text!r} on the screen")
        point = [int(match.group(1)), int(match.group(2))]
        return self.hands.run("left_click", {"coordinate": point})

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
