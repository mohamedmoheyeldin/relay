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
import os
import re
import threading
import time
import urllib.parse
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal, Protocol

from PIL import Image

from relay_screen.actions import ActionResult
from relay_screen.apps import ADDRESS_BAR, BROWSERS
from relay_screen.errors import ActionError
from relay_screen.input.activity import seconds_since_input
from relay_screen.runtime.catalog import Coords
from relay_screen.safety.approval import ApprovalDeniedError, PromptUnavailableError
from relay_screen.safety.killswitch import KillSwitchError

MAX_STEPS = 25
HISTORY = 10  # earlier steps repeated to the model as text
SEND_EDGE = 1280  # px, long edge of the screenshot sent to the model
ALIGN = 32  # sent sizes are multiples of the vision patch grid, so pixel models see it as sent
# Qwen-VL models (Holo3.1) need at least this many image tokens, one per 32 px cell; with
# fewer, llama.cpp warns and grounding suffers (D-108). Frames are grown to reach it here,
# so the model sees exactly the image whose pixels it answers in.
MIN_IMAGE_TOKENS = 1024
MAX_BAD_REPLIES = 3
SEEN_CHARS = 400  # of the element list, in each step's "seen" event
FAILS_IN_A_ROW = 3  # failed steps in a row before the model is told to change approach
STUCK_REPEATS = 3  # the same action with no visible change this many times in a row
MAX_TEXT = 2000
MAX_UIA_CHARS = 5000  # the focused window's UI Automation list, ~1.5k tokens
UIA_DEPTH = 8
FEW_ELEMENTS = 6  # with fewer element lines than this, read the screen's text with OCR
MAX_OCR_LINES = 40
MAX_APPS_CHARS = 2500
MAX_WINDOWS_CHARS = 1500
MAX_BATCH = 6  # actions the model may plan in one reply (D-084)
RICH_ELEMENTS = 8  # with this many element lines, skip the screenshot unless asked
OWNER_QUIET = 0.8  # s without the owner's own input before Relay uses the real mouse/keys
OWNER_PATIENCE = 20.0  # s Relay waits for that pause before acting anyway
CANCEL_POLL = 0.05  # s between stop checks while the model, Claude or a wait runs
# Members that send real input to whatever is in front; the target goes in front again
# first, in case the owner clicked elsewhere meanwhile (D-093).
INPUT_MEMBERS = frozenset(
    {"left_click", "double_click", "right_click", "type", "key", "scroll", "click_text"}
)
REPEAT_IS_STUCK = frozenset({"focus_window", "focus", "switch_to"})
POINT_CLICKS = frozenset({"left_click", "double_click", "right_click"})
MAX_PAGE_CHARS = 8000  # a web page's controls, ~2.5k tokens
PAGE_DEPTH = 40

REL1000 = (
    "X and Y are positions on the screenshot from 0 to 1000: 0,0 is the top left corner "
    "and 1000,1000 the bottom right."
)
PIXELS = (
    "X and Y are pixels of the screenshot as you get it, counted from its top left "
    "corner; its size is given with it."
)

SYSTEM_PROMPT = """\
You are Relay, an assistant that operates a Windows PC for its owner. You get the \
owner's task and the screen: the focused window's element list from Windows UI \
Automation and, when the list doesn't cover it, a screenshot.

Reply with exactly one JSON object and nothing else, either one action:
{"thought": "<one short sentence>", "action": "<name>", ...parameters}
or, to save time when you are sure of the next few, several actions that run in order \
and stop at the first one that fails:
{"thought": "<one short sentence>", "actions": [{"action": ...}, {"action": ...}]}
For example, fill a search box and press Enter in one reply. Reply with done on its \
own, after you have seen the result.

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
- {"action": "look"}  you need to see a screenshot before deciding
- {"action": "ask_claude", "question": "..."}  ask Claude, the owner's cloud assistant, \
when the task needs judgment or writing you can't do from the screen: what to write in \
a cover letter or message, which result fits the owner best. Put everything Claude \
needs in the question; its answer comes back as the result. Not for where to click.
- {"action": "done", "summary": "..."}  the task is finished; say what you did
- {"action": "fail", "reason": "..."}  you cannot finish; say why

<COORDS>

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
- Calculator and other apps that take keys: type the whole entry at once, for \
example type 12*34= in Calculator, instead of pressing its buttons one by one.
- To go to a website, use open_url instead of typing in the address bar.
- When the owner adds a note while you work, follow it from then on.
- Never click taskbar buttons to switch programs (that minimizes them); use \
focus_window. Never act on Relay's own window; it is marked in the windows list.
- Never type into a document, note or tab that already has the owner's text unless \
the task says to change it: open a new one first (ctrl+n, or the + tab button) and \
check it is empty.
- Type each piece of text once. Read the element list or look before typing again.
- To save a file where the owner said, set_value the full path into the File name box \
of the Save dialog, then invoke its Save button.
- Follow your plan, and reply done only when its done-check holds.
- In a calculator, type the whole sum with type (for example 12*56=) instead of \
pressing its buttons, then read the result.
- Do only what the task asks: never save, close, send or delete anything it didn't \
ask for.
- Choose the radio option the task names; the element list marks each radio button \
selected or not selected, so check the right one is selected before going on.
- Tick every checkbox the task needs (an "I agree" box too); the element list marks \
each one checked or unchecked.
- If the same actions did not finish the task once, do something different.
"""

PLAN_PROMPT = """You are Relay's local AI on a Windows PC. Before doing a task on the \
screen, make a short plan. Reply with exactly one JSON object and nothing else:
{"thought": "<one short sentence>", "action": "plan", "goal": "<what the owner wants>", \
"app": "<the program or website to use>", "steps": ["<step>", ...], \
"done_when": "<what is on the screen when it is finished>", \
"question": "<one short question for the owner, or empty>", \
"default": "<what you will do if they don't answer>"}

Keep steps to at most 6 short lines. The done-check names everything that must be true \
at the end: each field filled, each box ticked, the result shown. Ask a question only \
when the task leaves out something only the owner can decide and that matters: where \
to save a file, which account, which file or person, whether to send. Never ask about \
small details you can choose well yourself (wording, a file name, which search \
engine). Plan only what the task asks: no saving, closing or sending step it doesn't \
name. Writing new text means a new document or tab, never typing over one that is \
open. Use what is already open: a program in the windows list is switched to, not \
started again, and open_url opens pages in a new tab of the browser that is open.
"""
QUESTION_WAIT = 300  # s Relay waits for the owner's answer to the plan's question

Status = Literal["done", "failed", "stopped"]

# The actions SYSTEM_PROMPT offers. llama-server turns REPLY_SCHEMA into a grammar, so
# the model can't answer with a tool it has merely read about, like select_display (D-093).
ACTION_NAMES = (
    "plan",
    "click",
    "double_click",
    "right_click",
    "type",
    "key",
    "scroll",
    "launch_app",
    "open_url",
    "focus_window",
    "wait",
    "invoke",
    "set_value",
    "click_text",
    "look",
    "ask_claude",
    "done",
    "fail",
)
LOOK_ALIASES = frozenset({"select_display", "screenshot", "list_windows", "list_displays"})
_ACTION = {
    "type": "object",
    "properties": {"action": {"enum": list(ACTION_NAMES)}},
    "required": ["action"],
    "additionalProperties": True,
}
REPLY_SCHEMA: dict[str, Any] = {
    "anyOf": [
        {
            "type": "object",
            "properties": {"thought": {"type": "string"}, "action": {"enum": list(ACTION_NAMES)}},
            "required": ["thought", "action"],
            "additionalProperties": True,
        },
        {
            "type": "object",
            "properties": {
                "thought": {"type": "string"},
                "actions": {
                    "type": "array",
                    "items": _ACTION,
                    "minItems": 1,
                    "maxItems": MAX_BATCH,
                },
            },
            "required": ["thought", "actions"],
            "additionalProperties": False,
        },
    ]
}


def system_prompt(coords: Coords) -> str:
    return SYSTEM_PROMPT.replace("<COORDS>", PIXELS if coords == "pixels" else REL1000)


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
    target: str = ""  # the listed element it acted on; ids change with every listing

    def line(self) -> str:
        return f"Step {self.n}: {describe(self.action)} -> {self.result}"


@dataclass
class Outcome:
    status: Status
    message: str
    steps: list[Step] = field(default_factory=list)


class Cancelled(Exception):
    """The owner pressed Stop while Relay waited on the model, Claude or a pause."""


class ReplyError(ValueError):
    """The model's reply was not one usable action."""


def parse_reply(text: str) -> dict[str, Any]:
    """The action object in a model reply, tolerating code fences and stray prose."""
    text = re.sub(r"<think>.*?</think>", "", text, flags=re.DOTALL)
    text = re.sub(r"```(?:json)?", "", text)
    start = text.find("{")
    if start < 0:
        raise ReplyError("no JSON object in the reply")
    try:
        value, _ = json.JSONDecoder().raw_decode(text[start:])
    except ValueError as exc:
        raise ReplyError(f"the JSON did not parse: {exc}") from exc
    if isinstance(value, dict) and isinstance(value.get("actions"), list):
        items = value["actions"][:MAX_BATCH]
        if not items:
            raise ReplyError('"actions" is empty')
        return {
            "thought": value.get("thought", ""),
            "action": "batch",
            "actions": [_one(item) for item in items],
        }
    return _one(value)


def _one(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict) or not isinstance(value.get("action"), str):
        raise ReplyError('the reply needs an "action" name')
    value = dict(value)
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
        size = (_align(width * scale), _align(height * scale))
        while (size[0] // ALIGN) * (size[1] // ALIGN) < MIN_IMAGE_TOKENS:
            scale *= 1.01
            size = (_align(width * scale), _align(height * scale))
        if size != image.size:
            image = image.resize(size, Image.Resampling.LANCZOS)
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


# RELAY_POINT_FALLBACK=0 turns off the listed-centre and raw-point fallbacks, so a run
# measures the model's own position clicks (D-106, D-108).
POINT_FALLBACK = os.environ.get("RELAY_POINT_FALLBACK", "1") != "0"
LAUNCH_NAMES = frozenset({"launch_app", "open_app", "open"})
CLICK_NAMES = frozenset({"click", "left_click", "double_click", "right_click"})
LISTED_CENTER = re.compile(r"center \((\d+), (\d+)\)")
SNAP_PX = 3


def _listed_point(elements: str, action: Mapping[str, Any]) -> list[int] | None:
    """The element-list centre a click names, within a few pixels (D-106)."""
    try:
        x, y = float(action["x"]), float(action["y"])
    except (KeyError, TypeError, ValueError):
        return None
    for found in LISTED_CENTER.finditer(elements):
        cx, cy = int(found.group(1)), int(found.group(2))
        if abs(cx - x) <= SNAP_PX and abs(cy - y) <= SNAP_PX:
            return [cx, cy]
    return None


LISTED_BOX = re.compile(r"box \[(\d+), (\d+), (\d+), (\d+)\]")


def _in_listed_box(elements: str, point: list[int]) -> bool:
    x, y = point
    for found in LISTED_BOX.finditer(elements):
        x0, y0, x1, y1 = (int(v) for v in found.groups())
        if x0 <= x <= x1 and y0 <= y <= y1:
            return True
    return False


def _likely_point(elements: str, action: Mapping[str, Any], scaled: list[int]) -> list[int]:
    """The scaled point, unless only the model's raw numbers land on a listed control.

    Holo sometimes answers in the element list's screenshot pixels rather than the
    smaller image's: Calculator's Multiply at (270, 495) was scaled to a point below
    the window twenty times in a row (D-106).
    """
    if not elements or _in_listed_box(elements, scaled):
        return scaled
    try:
        raw = [round(float(action["x"])), round(float(action["y"]))]
    except (KeyError, TypeError, ValueError):
        return scaled
    return raw if _in_listed_box(elements, raw) else scaled


def _align(length: float) -> int:
    return max(ALIGN, round(length / ALIGN) * ALIGN)


def to_member(action: Mapping[str, Any], frame: Frame | None, coords: Coords) -> tuple[str, dict]:
    """Map a model action onto a computer toolset member and its input.

    Without a frame (a text-only step, D-084) a position can't be trusted: the model
    hasn't seen the screen, so it has to look first.
    """
    name = action["action"]
    if name in ("click", "left_click", "double_click", "right_click"):
        if frame is None:
            raise ReplyError("you have not seen the screen yet; use an element id or look")
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
        if "x" in action and "y" in action and frame is not None:
            params["coordinate"] = frame.to_screen(action["x"], action["y"], coords)
        return "scroll", params
    if name in ("launch_app", "open_app", "open"):
        app = next(
            (
                action[k]
                for k in ("name", "app", "program", "application", "target", "title", "text")
                if isinstance(action.get(k), str) and action[k].strip()
            ),
            None,
        )
        if not isinstance(app, str) or not app:
            # Holo also wrote {"action": "launch_app", "app_name": "Notepad"} (D-106).
            args = action.get("args")
            if isinstance(args, list) and args and isinstance(args[0], str):
                app = args[0]
        if not isinstance(app, str) or not app:
            app = next(
                (
                    v
                    for k, v in action.items()
                    if k not in ("action", "thought", "args") and isinstance(v, str) and v.strip()
                ),
                None,
            )
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


def with_paths(text: str, home: Path | None = None) -> str:
    """'... my Downloads folder' plus its full path, for the Save dialog (D-104)."""
    home = home or Path.home()
    found = [n for n in KNOWN_FOLDERS if re.search(rf"\b{n}\b", text, re.IGNORECASE)]
    if not found:
        return text
    paths = ", ".join(f"{n} is {home / n}" for n in found)
    return f"{text} (Full path: {paths}. Put the full path and file name in the File name box.)"


# Roles the model needs first when a window's list is too long to show whole (D-104):
# the window's content, then its controls, then the navigation tree, then the rest.
ROLE_ORDER = {
    **dict.fromkeys(("ListItem", "DataItem", "Edit", "Document", "RadioButton"), 0),
    **dict.fromkeys(
        ("Button", "CheckBox", "ComboBox", "Hyperlink", "MenuItem", "TabItem", "SplitButton"), 1
    ),
    "TreeItem": 2,
}


def _prioritize(listing: str, limit: int) -> str:
    """A long element list with content and controls first, so a folder's items aren't
    cut off behind Explorer's navigation pane."""
    if len(listing) <= limit:
        return listing
    head, _, body = listing.partition("\n")
    lines = body.splitlines()

    def rank(line: str) -> int:
        found = re.match(r"\s*\[e\d+\] (\w+)", line)
        return ROLE_ORDER.get(found.group(1), 3) if found else 4

    ordered = sorted(lines, key=rank)
    return _cut("\n".join([head, *(line.strip() for line in ordered)]), limit)


def _cut(listing: str, limit: int) -> str:
    if len(listing) <= limit:
        return listing
    return listing[:limit].rsplit("\n", 1)[0] + "\n… (list cut)"


def _browser_in_front(windows: str) -> bool:
    for line in windows.splitlines():
        parts = line.split(" · ")
        if len(parts) > 2 and "in front" in parts:
            return parts[1].casefold().removesuffix(".exe") in BROWSERS
    return False


def _rich(elements: str) -> bool:
    return elements.startswith("Elements") and elements.count("\n") >= RICH_ELEMENTS


_LEAD = r"^(?:please\s+|can you\s+|could you\s+)*"
INSTANT_URL = re.compile(_LEAD + r"(?:go to|open|visit|navigate to)\s+(\S+)$", re.IGNORECASE)
INSTANT_FOCUS = re.compile(
    _LEAD + r"(?:switch to|go back to|bring up|focus(?: on)?)\s+(?:the\s+)?(.+?)(?:\s+window)?$",
    re.IGNORECASE,
)
INSTANT_OPEN = re.compile(
    _LEAD + r"(?:open|launch|start|run)\s+(?:the\s+|my\s+)?(.+?)(?:\s+app)?$", re.IGNORECASE
)
COMPOUND = re.compile(r"\b(?:and|then|in|on|with|to|for|from)\b|[,;:]", re.IGNORECASE)


def instant_plan(task: str) -> list[tuple[str, dict[str, Any]]] | None:
    """Members that finish a one-move task with no model at all (D-084, tier 0).

    "go to dice.com", "open Notepad", "switch to Outlook". Anything longer goes to
    the model; an app name that doesn't resolve falls back to it too.
    """
    text = task.strip().rstrip(".!")
    url = INSTANT_URL.match(text)
    if url and _task_url(f"go to {url.group(1)}") is not None:
        return [("open_url", {"url": _task_url(f"go to {url.group(1)}")})]
    focus = INSTANT_FOCUS.match(text)
    if focus and not COMPOUND.search(focus.group(1)):
        return [("focus_window", {"window": focus.group(1)})]
    opened = INSTANT_OPEN.match(text)
    if opened and not COMPOUND.search(opened.group(1)) and len(opened.group(1)) <= 60:
        name = opened.group(1)
        return [("focus_window", {"window": name}), ("launch_app", {"name": name})]
    return None


CALC_TASK = re.compile(
    _LEAD
    + r"(?:(?:open|launch|start|use)\s+(?:the\s+)?calculator\s*(?:,|and|then)\s*(?:then\s+)?)?"
    r"(?:in\s+(?:the\s+)?calculator\s*,?\s*)?"
    r"(?:work out|calculate|compute|what is|what's|add up|do)\s+"
    r"(.+?)(?:\s+in\s+(?:the\s+)?calculator)?\??$",
    re.IGNORECASE,
)
CALC_WORDS = (
    (r"\b(?:multiplied by|times)\b|[x\u00d7]", "*"),
    (r"\b(?:divided by|over)\b|\u00f7", "/"),
    (r"\bplus\b", "+"),
    (r"\bminus\b", "-"),
)
CALC_SUM = re.compile(r"\d+(?:\.\d+)?(?:[+*/-]\d+(?:\.\d+)?)+")


def calc_entry(task: str) -> str | None:
    """'12*34=' for "Open Calculator and work out 12 times 34": typed whole, no model
    (D-106). None unless the task names Calculator and is one plain sum."""
    text = task.strip().rstrip(".!")
    found = CALC_TASK.match(text)
    if found is None or "calculator" not in text.casefold():
        return None
    entry = found.group(1).casefold()
    for words, sign in CALC_WORDS:
        entry = re.sub(words, sign, entry)
    entry = re.sub(r"\s+|,(?=\d{3})", "", entry)
    return entry + "=" if CALC_SUM.fullmatch(entry) else None


EDITORS = frozenset({"notepad", "notepad++", "wordpad", "winword", "code", "sublime_text"})
NEW_TEXT = re.compile(
    r"\b(write|type|jot|draft|compose|create|make|start)\b.*\b(note|list|text|letter|"
    r"document|doc|file|memo|draft|story|poem|email|message|paragraph)\b",
    re.IGNORECASE,
)
EDIT_TEXT = re.compile(
    r"\b(edit|change|fix|update|append|add to|replace)\b"
    r"|\b(in the open|this|that|the current|the open|my)\s+(document|doc|file|note|tab|text|"
    r"list|page|draft)\b",
    re.IGNORECASE,
)
# "Open Notepad and type: ...": typing into an editor is new text too (D-104).
TYPE_IN_EDITOR = re.compile(
    r"\b(type|write|jot|paste|put|note)\b.*\b(notepad|wordpad|word|notepad\+\+)\b"
    r"|\b(notepad|wordpad|word|notepad\+\+)\b.*\b(type|write|jot|paste|put|note)\b",
    re.IGNORECASE,
)
# A task that saves a file but doesn't say where: Relay asks the owner (D-104).
SAVES = re.compile(r"\bsave\b", re.IGNORECASE)
UNASKED = re.compile(r"\b(save|close|exit|quit)\b", re.IGNORECASE)
PLACE = re.compile(
    r"[a-z]:\\|\\\\|/|\b(folder|desktop|documents|downloads|pictures|music|videos|"
    r"onedrive|drive|in my|to my|same place|where it was)\b",
    re.IGNORECASE,
)
KNOWN_FOLDERS = ("Downloads", "Documents", "Desktop", "Pictures", "Music", "Videos")
REPEAT_MIN = 20  # characters; short text ("Enter", a name) may well be typed twice


FILE_NAME = re.compile(
    r"(?:[a-z]:\\[^\s\"']*\\)?[\w.-]+\.(?:txt|md|csv|json|log|html?|docx?|xlsx?|rtf)\b",
    re.IGNORECASE,
)
SAVE_KEYS = re.compile(r"\bctrl\+(?:shift\+)?s\b", re.IGNORECASE)
SAVE_DIALOG_TRIES = 4
SAVE_DIALOG_WAIT = 0.5  # s between looks for the dialog


def save_target(task: str, answer: str = "", home: Path | None = None) -> str:
    """The full path a save task means: 'relay.txt' plus Downloads from the task or the
    owner's answer to "Where should I save it?" (D-106). "" when either is missing."""
    name = FILE_NAME.search(task)
    if name is None:
        return ""
    if re.match(r"[a-z]:\\", name.group(0), re.IGNORECASE):
        return name.group(0)
    for text in (task, answer):
        folder = next(
            (n for n in KNOWN_FOLDERS if re.search(rf"\b{n}\b", text, re.IGNORECASE)), None
        )
        if folder:
            return str((home or Path.home()) / folder / name.group(0))
    return ""


def _opens_save(member: str, params: Mapping[str, Any], elements: str) -> bool:
    """Ctrl+S, or a press of a Save / Save as control, which may open the Save dialog."""
    if member == "key":
        return bool(SAVE_KEYS.search(str(params.get("text", ""))))
    if member == "click_text":
        label = str(params.get("text", ""))
    elif member == "uia_invoke":
        label = _element_label(elements, str(params.get("element", ""))).strip('"')
    else:
        return False
    return label.strip().casefold() in ("save", "save as", "save as…", "save as...")


SUBMITS = re.compile(r"(next|submit|continue|apply|send|finish|save)\b", re.IGNORECASE)
EMPTY_FIELD = re.compile(r'^\s*\[e\d+\] (?:Edit|Spinner) "([^"]+)"(?![^\n]*value=)', re.M)


def empty_fields(elements: str, task: str) -> list[str]:
    """Names of empty boxes on the page that the task mentions, e.g. "Years of experience"."""
    wanted = " ".join(task.casefold().split())
    found = []
    for name in EMPTY_FIELD.findall(elements):
        key = " ".join(name.casefold().rstrip("?:* ").split())
        if len(key) >= 3 and key in wanted and name not in found:
            found.append(name)
    return found


def _typed_key(params: Mapping[str, Any]) -> str:
    return " ".join(str(params.get("text", params.get("value", ""))).split()).casefold()


def _center(elements: str, element_id: str) -> list[int] | None:
    found = re.search(
        rf"^\s*\[{re.escape(element_id)}\] .*?center \((\d+), (\d+)\)", elements, re.M
    )
    return [int(found.group(1)), int(found.group(2))] if found else None


# "Open a new tab in the web browser and search the web for ..." (D-106).
_BROWSER_LEAD = (
    r"(?:(?:open|start)\s+(?:a\s+new\s+tab|(?:the\s+|a\s+)?(?:web\s+)?browser)"
    r"(?:\s+in\s+(?:the\s+|my\s+)?(?:web\s+)?browser)?\s+and\s+(?:then\s+)?)?"
)
WEB_FIND = re.compile(
    _LEAD + _BROWSER_LEAD + r"(?:find|search(?: the web)? for|search|look up|look for|compare|book|"
    r"check (?:the )?prices? (?:of|for)|get me)\s+(?:me\s+)?(?:an?\s+|the\s+|some\s+)?(.{3,200})$",
    re.IGNORECASE,
)
ON_THE_PC = re.compile(
    r"\b(file|files|folder|document|pdf|photo|picture|email|e-mail|mail|inbox|message|"
    r"on my (?:pc|computer|desktop|laptop)|in (?:my )?downloads|setting|settings)\b",
    re.IGNORECASE,
)
SEARCH_URL = "https://www.google.com/search?q="
BROWSER_STEMS = frozenset({"brave", "chrome", "msedge", "firefox", "opera", "vivaldi"})


def web_search_url(task: str, windows: str = "") -> str | None:
    """A web search for "find a cheap flight to Cairo next month"-style tasks (D-093).

    Only when the task names no web address, nothing on the PC (files, mail, settings)
    and no open program: then the first move is plainly a search, so it needs no model.
    """
    text = task.strip().rstrip(".!")
    found = WEB_FIND.match(text)
    if found is None or _task_url(text) or ON_THE_PC.search(text):
        return None
    words = set(re.findall(r"[a-z0-9]+", text.casefold()))
    for line in windows.splitlines():
        parts = line.split(" · ")
        if len(parts) < 2 or any(p.startswith("Relay itself") for p in parts):
            continue  # "relay diagnostics 1" names Relay's own window, not a program
        stem = parts[1].casefold().removesuffix(".exe")
        if stem in words - {"find"} and stem not in BROWSER_STEMS:
            return None  # "find the invoice in Outlook": work in that program
    return SEARCH_URL + urllib.parse.quote_plus(found.group(1))


PICK_ROLES = ("ListItem", "RadioButton", "TabItem", "MenuItem", "TreeItem")


def _picked(elements: str, element_id: str) -> str:
    """'ListItem "Blue"' when the element is an option to pick; picking the same one
    again and again means it isn't taking (D-106). "" for buttons, which can repeat."""
    found = re.search(rf"\[{re.escape(element_id)}\] (\w+) \"([^\"]+)\"", elements)
    if found is None or found.group(1) not in PICK_ROLES:
        return ""
    return f'{found.group(1)} "{found.group(2)}"'


LISTING_LINE = re.compile(r'^(\s*)\[(e\d+)\] (\w+)(?: "([^"]*)")?')


def _dropdown_option(elements: str, element_id: str) -> tuple[str, list[int]] | None:
    """('Blue', centre of its ComboBox) when the element is an option inside a dropdown."""
    lines = elements.splitlines()
    for i, line in enumerate(lines):
        found = LISTING_LINE.match(line)
        if found is None or found.group(2) != element_id:
            continue
        if found.group(3) != "ListItem" or not found.group(4):
            return None
        indent = len(found.group(1))
        for above in reversed(lines[:i]):
            parent = LISTING_LINE.match(above)
            if parent is None or len(parent.group(1)) >= indent:
                continue
            if parent.group(3) == "ComboBox":
                centre = _center(elements, parent.group(2))
                return (found.group(4), centre) if centre else None
            if parent.group(3) not in ("List", "Group", "Pane"):
                return None
            indent = len(parent.group(1))
        return None
    return None


SWITCHES = 4  # focus_window steps in a row before the model is told to act


def _switching(steps: list[Step]) -> bool:
    last = steps[-SWITCHES:]
    return len(last) == SWITCHES and all(
        s.action.get("action") in ("focus_window", "focus", "switch_to") for s in last
    )


FIELD_ROLES = ("Edit", "Spinner", "ComboBox", "Document")
TARGET_ROLES = (*FIELD_ROLES, "Button", "CheckBox", "RadioButton", "Hyperlink", "ListItem")
NAMED_LINE = re.compile(r'^\s*\[(e\d+)\] (\w+) "([^"]{3,})"[^\n]*?center \((\d+), (\d+)\)', re.M)


def _named(
    elements: str, thought: str, roles: tuple[str, ...], empty: bool = False
) -> tuple[str, list[int]] | None:
    """The listed control whose name the thought says, longest name first; with
    `empty`, only boxes that have nothing in them yet."""
    said = " ".join(thought.casefold().split())
    best: tuple[int, str, list[int]] | None = None
    for found in NAMED_LINE.finditer(elements):
        element_id, role, name = found.group(1), found.group(2), found.group(3)
        key = " ".join(name.casefold().rstrip("?:* ").split())
        if empty and "value=" in found.group(0):
            continue
        good = role in roles and len(key) >= 3 and key in said
        if good and (best is None or len(key) > best[0]):
            point = [int(found.group(4)), int(found.group(5))]
            best = (len(key), element_id, point)
    return (best[1], best[2]) if best else None


def _listed_named(elements: str, text: str) -> str | None:
    """The id of a listed control named exactly `text` (ignoring case), if one is."""
    want = " ".join(text.casefold().split())
    for found in re.finditer(r'^\s*\[(e\d+)\] (\w+) "([^"]+)"', elements, re.M):
        if found.group(2) in TARGET_ROLES and " ".join(found.group(3).casefold().split()) == want:
            return found.group(1)  # even one on another display, with no centre here
    return None


def _intended_element(elements: str, thought: str) -> list[int] | None:
    found = _named(elements, thought, TARGET_ROLES)
    return found[1] if found else None


def _intended_field(elements: str, thought: str) -> str | None:
    found = _named(elements, thought, FIELD_ROLES, empty=True)
    return found[0] if found else None


def _element_label(elements: str, element_id: str) -> str:
    """'"Search jobs"' for e20 when the listing has `[e20] Edit "Search jobs"`."""
    found = re.search(rf"\[{re.escape(element_id)}\] (\w+)(?: \"([^\"]*)\")?", elements)
    if found is None:
        return "a control"
    role, name = found.group(1), found.group(2)
    return f'"{name}"' if name else f"the {role.lower()}"


def plain(action: Mapping[str, Any], result: str, error: bool, elements: str) -> str:
    """One line in plain words for the chat and the pill: 'Typed "python" into "Search"'."""
    name = action.get("action")
    a = dict(action)

    def q(value: Any, limit: int = 60) -> str:
        text = " ".join(str(value).split())
        return '"' + (text if len(text) <= limit else text[: limit - 1] + "…") + '"'

    match name:
        case "open_url":
            line = f"Opened {a.get('url')}"
        case "focus_window":
            line = f"Switched to {a.get('window')}"
        case "launch_app" | "open_app" | "open":
            line = f"Opened {a.get('name') or a.get('app')}"
        case "type":
            line = f"Typed {q(a.get('text', ''))}"
        case "key" | "hotkey" | "press":
            line = f"Pressed {a.get('keys') or a.get('key') or a.get('text')}"
        case "set_value":
            target = _element_label(elements, str(a.get("element")))
            line = f"Typed {q(a.get('value', a.get('text', '')))} into {target}"
        case "invoke":
            line = f"Pressed {_element_label(elements, str(a.get('element')))}"
        case "click_text":
            line = f"Clicked {q(a.get('text', ''))}"
        case "click" | "left_click" | "double_click" | "right_click":
            verb = {"double_click": "Double-clicked", "right_click": "Right-clicked"}
            line = verb.get(str(name), "Clicked") + " on the screen"
        case "scroll":
            line = f"Scrolled {a.get('direction', 'down')}"
        case "wait":
            line = f"Waited {a.get('seconds', 1)} s"
        case "ask_claude":
            line = f"Asked Claude: {q(a.get('question', ''), 80)}"
        case _:
            line = describe(action)
    if error:
        reason = result.removeprefix("STOP: ").removeprefix("failed: ")
        return f"{line} (didn't work: {reason[:120]})"
    return line


def run_instant(task: str, hands: Hands, emit: Event) -> Outcome | None:
    """Do a one-move task straight away, or None when it needs the model.

    For "open X" an open window of X is brought forward first and the app is launched
    only when there is none.
    """
    entry = calc_entry(task)
    plan = instant_plan("open Calculator") if entry else instant_plan(task)
    if plan is None:
        return None
    started = time.perf_counter()
    for member, params in plan:
        action = {"action": member, **params}
        try:
            result = hands.run(member, params)
        except (KillSwitchError, ApprovalDeniedError, PromptUnavailableError) as exc:
            step = Step(1, "", action, f"STOP: {exc}", True)
            emit({"event": "stopped", "message": str(exc), "steps": 1})
            return Outcome("stopped", str(exc), [step])
        except ActionError:
            continue  # try the next way, or hand the task to the model
        text = result.text or "done"
        if result.is_error or (member == "focus_window" and "is in front" not in text):
            continue
        if entry:
            return _type_entry(entry, Step(1, "", action, text), hands, emit, started)
        step = Step(1, "", action, text)
        ms = round((time.perf_counter() - started) * 1000)
        say = plain(action, text, False, "")
        emit(
            {
                "event": "step",
                "step": 1,
                "thought": "",
                "action": describe(action),
                "say": say,
                "result": text,
            }
        )
        emit({"event": "done", "message": text, "steps": 1, "instant_ms": ms})
        return Outcome("done", text, [step])
    return None


def _type_entry(
    entry: str, opened: Step, hands: Hands, emit: Event, started: float
) -> Outcome | None:
    """Calculator is in front: type the whole sum in one go (D-106)."""
    action = {"action": "type", "text": entry}
    try:
        result = hands.run("type", {"text": entry})
    except (KillSwitchError, ApprovalDeniedError, PromptUnavailableError) as exc:
        emit({"event": "stopped", "message": str(exc), "steps": 2})
        return Outcome("stopped", str(exc), [opened, Step(2, "", action, f"STOP: {exc}", True)])
    except ActionError:
        return None
    if result.is_error:
        return None
    text = result.text or "done"
    steps = [opened, Step(2, "", action, text)]
    for step in steps:
        emit(
            {
                "event": "step",
                "step": step.n,
                "thought": "",
                "action": describe(step.action),
                "say": plain(step.action, step.result, False, ""),
                "result": step.result,
            }
        )
    ms = round((time.perf_counter() - started) * 1000)
    message = f"Typed {entry} into Calculator"
    emit({"event": "done", "message": message, "steps": 2, "instant_ms": ms})
    return Outcome("done", message, steps)


class LocalAgent:
    def __init__(
        self,
        hands: Hands,
        model: Model,
        *,
        coords: Coords = "rel1000",
        max_steps: int = MAX_STEPS,
        on_event: Event | None = None,
        instant: bool = True,
        inbox: Callable[[], list[str]] | None = None,
        cancelled: Callable[[], bool] | None = None,
        ask: Callable[[str], str] | None = None,
        idle: Callable[[], float | None] | None = None,
    ) -> None:
        self.instant = instant
        self.inbox = inbox or (lambda: [])
        self.can_ask = inbox is not None  # someone can answer the plan's question
        self.cancelled = cancelled or (lambda: False)
        self.ask = ask
        self.notes: list[str] = []  # what the owner added while the task ran
        self.target = ""  # the program the task works in (its exe stem), once known
        self.target_handle = 0  # and its exact window: a second Notepad isn't it (D-106)
        self.idle = idle if idle is not None else seconds_since_input
        self.last_input = 0.0  # monotonic time Relay last sent real input
        self.plan = ""  # the plan made before the first step (D-094)
        self.done_when = ""  # the plan's done-when, checked once before finishing (D-098)
        self.done_checked = False
        self.typed: dict[str, int] = {}  # text typed so far -> the step that typed it
        self.save_to = ""  # full path for the Save dialog, once the task and owner name it
        self.task = ""
        self.last_type_step = 0  # the step that typed last, when it was the latest step
        self.plan_app = ""  # the program the plan named, for a launch_app with no name
        self.warned: set[str] = set()  # empty fields already pointed out (D-106)
        self.new_doc = False  # a writing task: editors get a new tab first
        self.fresh: set[str] = set()  # editors that already got their new tab
        self.elements = ""
        self.hands = hands
        self.model = model
        self.coords: Coords = coords
        self.max_steps = max_steps
        self.emit = on_event or (lambda event: None)

    def run(self, task: str) -> Outcome:
        if self.instant:
            done = run_instant(task, self.hands, self.emit)
            if done is not None:
                return done
        steps: list[Step] = []
        bad_replies = 0
        last_batch, repeats = "", 0
        note = ""
        look = True  # the first decision sees the screen
        apps = self._apps()
        n = 0
        self.new_doc = bool(NEW_TEXT.search(task) or TYPE_IN_EDITOR.search(task)) and not (
            EDIT_TEXT.search(task)
        )
        self.save_to = save_target(task)
        self.task = task
        try:
            self.plan = self._make_plan(task, apps)
        except Cancelled:
            return self._finish("stopped", "Stopped by the owner.", steps)
        url = _task_url(task)
        if url:  # "go to https://www.dice.com/ and ...": open it as step 1, no model (D-093)
            n = 1
            action = {"action": "open_url", "url": url}
            steps.append(self._act(n, "", action, "open_url", {"url": url}))
            if steps[-1].error and steps[-1].result.startswith("STOP:"):
                return self._finish("stopped", steps[-1].result.removeprefix("STOP: "), steps)
        elif (search := web_search_url(task, self._windows())) is not None:
            n = 1  # "find a cheap flight to Cairo": search the web first, no model (D-093)
            action = {"action": "open_url", "url": search}
            steps.append(self._act(n, "", action, "open_url", {"url": search}))
            if steps[-1].error and steps[-1].result.startswith("STOP:"):
                return self._finish("stopped", steps[-1].result.removeprefix("STOP: "), steps)
        else:
            self._focus_target(task)
        while n < self.max_steps:
            if self.cancelled():
                return self._finish("stopped", "Stopped by the owner.", steps)
            fresh = self.inbox()
            if fresh:
                self.notes.extend(fresh)
                look = True
            began = time.perf_counter()
            windows = self._windows()
            elements = self._elements(windows)
            self.elements = elements
            front = next((w for w in windows.splitlines() if "in front" in w.split(" · ")), "")
            # What the model is shown, for Diagnostics: the window in front and the top
            # of its element list (D-106).
            self.emit({"event": "seen", "front": front, "elements": elements[:SEEN_CHARS]})
            show = look or not _rich(elements)
            frame = Frame.from_png(self._screenshot()) if show else None
            screen_text = self._screen_text() if _few(elements) else ""
            context = Context(elements, screen_text, apps, windows)
            perceived = time.perf_counter()
            messages = self._messages(task, steps, frame, note, context)
            try:
                reply = self._unless_stopped(lambda m=messages: self.model(m))
            except Cancelled:
                return self._finish("stopped", "Stopped by the owner.", steps)
            decided = time.perf_counter()
            note, look = "", False
            try:
                plan = self._plan(reply, frame, elements)
            except (ReplyError, ValueError) as exc:
                bad_replies += 1
                self.emit({"event": "bad_reply", "step": n + 1, "error": str(exc)})
                if bad_replies >= MAX_BAD_REPLIES:
                    message = f"The local model's replies could not be used ({exc})."
                    return self._finish("failed", message, steps)
                note = f"Your last reply could not be used: {exc}. Reply with one JSON object."
                continue
            bad_replies = 0
            thought, actions = plan
            batch = json.dumps([[m, p] for _, m, p in actions], sort_keys=True)
            if len(actions) > 1 and batch == last_batch:
                # The Calculator run repeated one 6-action batch to the step limit (D-100).
                repeats += 1
                if repeats >= MAX_BAD_REPLIES:
                    message = "The local model kept repeating the same actions, so Relay stopped."
                    return self._finish("failed", message, steps)
                note = (
                    "You planned exactly the same actions as last time, and they did not "
                    "finish the task. Look at the screen and do something different."
                )
                look = True
                continue
            last_batch, repeats = batch, 0
            for action, member, params in actions:
                if self.cancelled():
                    return self._finish("stopped", "Stopped by the owner.", steps)
                if action["action"] == "done":
                    if self.done_when and not self.done_checked:
                        # One look against the plan's done-when before finishing: the
                        # model stopped after a search page in the owner's flight task
                        # (D-098).
                        self.done_checked = True
                        self.emit({"event": "note", "text": "Checking that it is really done…"})
                        note = (
                            f"Before you finish, look at the screen and check the plan: done "
                            f"when {self.done_when}. Reply done only if that is true now; "
                            "otherwise keep going."
                        )
                        look = True
                        break
                    return self._finish("done", str(action.get("summary") or "Done."), steps)
                if action["action"] == "fail":
                    reason = str(action.get("reason") or "The local model gave up.")
                    return self._finish("failed", reason, steps)
                if action["action"] == "look":
                    look = True
                    break
                n += 1
                step = self._act(n, thought, action, member, params)
                steps.append(step)
                if step.error and step.result.startswith("STOP:"):
                    return self._finish("stopped", step.result.removeprefix("STOP: "), steps)
                if self._stuck(steps):
                    message = "The same action kept changing nothing, so Relay stopped."
                    return self._finish("failed", message, steps)
                if _switching(steps):
                    # Brave, Notepad, Brave, Notepad... in round 10 (D-106).
                    note = (
                        f"You switched windows {SWITCHES} times in a row. The window in front "
                        "is the one you last chose: act in it now (type, invoke, set_value), "
                        "or reply done if the task is finished."
                    )
                    look = True
                    break
                if step.error:
                    look = True  # see what went wrong before planning again
                    if len(steps) >= FAILS_IN_A_ROW and all(
                        s.error for s in steps[-FAILS_IN_A_ROW:]
                    ):
                        # Calculator: fourteen position clicks in a row missed (D-106).
                        note = (
                            f"Your last {FAILS_IN_A_ROW} actions all failed. Stop clicking by "
                            "position: use invoke or set_value with an element id from the "
                            "list, type with the keyboard, or try another way."
                        )
                    break
                if n >= self.max_steps:
                    break
            self.emit(
                {
                    "event": "timing",
                    "step": n,
                    "actions": len(actions),
                    "screenshot_sent": show,
                    "perceive_ms": round((perceived - began) * 1000),
                    "think_ms": round((decided - perceived) * 1000),
                    "act_ms": round((time.perf_counter() - decided) * 1000),
                }
            )
        return self._finish("failed", f"Not finished after {self.max_steps} steps.", steps)

    def _plan(
        self, reply: str, frame: Frame | None, elements: str
    ) -> tuple[str, list[tuple[dict[str, Any], str, dict]]]:
        """The reply's actions, each mapped to a member and checked before any of them runs.

        done or fail after other actions in the same reply is dropped: the model has to
        see the result first.
        """
        parsed = parse_reply(reply)
        thought = str(parsed.pop("thought", ""))
        actions = parsed["actions"] if parsed["action"] == "batch" else [parsed]
        planned: list[tuple[dict[str, Any], str, dict]] = []
        for i, action in enumerate(actions):
            if action["action"] in LOOK_ALIASES or action["action"] == "plan":
                action = {**action, "action": "look"}  # the display follows the window
            if action["action"] in ("done", "fail", "look"):
                if i == 0:
                    planned.append((action, "", {}))
                break
            if action["action"] in ("ask_claude", "ask"):
                question = action.get("question") or action.get("text")
                if not isinstance(question, str) or not question.strip():
                    raise ReplyError("ask_claude needs a question")
                action = {**action, "action": "ask_claude", "question": question.strip()}
                planned.append((action, "ask_claude", {"question": question.strip()[:MAX_TEXT]}))
                break  # the answer changes what comes next
            hit = (
                _listed_point(elements, action)
                if action["action"] in CLICK_NAMES and POINT_FALLBACK
                else None
            )
            if hit is not None:
                # The model copied an element's centre from the list: those are already
                # screenshot pixels, not pixels of the smaller image it was sent (D-106).
                name = action["action"]
                member = "left_click" if name in ("click", "left_click") else name
                params: dict[str, Any] = {"coordinate": hit}
            else:
                if (
                    action["action"] in LAUNCH_NAMES
                    and self.plan_app
                    and not any(
                        isinstance(v, str) and v.strip()
                        for k, v in action.items()
                        if k not in ("action", "thought")
                    )
                ):
                    # A bare launch_app: the plan already named the program (D-106).
                    action = {**action, "name": self.plan_app}
                member, params = to_member(action, frame, self.coords)
                if member in POINT_CLICKS and POINT_FALLBACK:
                    params = {"coordinate": _likely_point(elements, action, params["coordinate"])}
            element = params.get("element")
            if isinstance(element, str) and f"[{element}]" not in elements:
                raise ReplyError(f"{element} is not in the element list")
            planned.append((action, member, params))
        if not planned:
            raise ReplyError("the reply had no action to run")
        return thought, planned

    def _screenshot(self) -> bytes:
        shot = self.hands.run("screenshot")
        if shot.image is None:
            raise ActionError("the screenshot came back empty")
        return shot.image

    def _elements(self, windows: str = "") -> str:
        """The focused window's UI Automation elements, or "" when UIA can't read it.

        A browser in front is read from its page: the address bar, then the controls
        of the web document, so the model works the site with no extension (D-089).
        """
        if _browser_in_front(windows):
            page = self._page()
            if page:
                return page
        try:
            listing = self.hands.run("uia_tree", {"depth": UIA_DEPTH}).text or ""
        except Exception:
            return ""
        return _prioritize(listing, MAX_UIA_CHARS)

    def _page(self) -> str:
        try:
            docs = self.hands.run("uia_find", {"query": "document", "role": "Document"}).text
            address = self.hands.run("uia_find", {"query": ADDRESS_BAR, "role": "Edit"}).text
        except Exception:
            return ""
        doc = next(
            (m.group(1) for m in re.finditer(r"^\[(e\d+)\] Document.*center \(", docs or "", re.M)),
            None,
        )
        if doc is None:
            return ""
        try:
            listing = self.hands.run(
                "uia_tree", {"root": doc, "depth": PAGE_DEPTH, "interactive": True}
            ).text
        except Exception:
            return ""
        if not listing or not listing.startswith("Elements"):
            return ""
        head, _, body = listing.partition("\n")
        bar = next((line for line in (address or "").splitlines() if line.startswith("[e")), "")
        lines = [head, *([bar] if bar else []), body]
        return _cut("\n".join(lines), MAX_PAGE_CHARS)

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
                self._remember_front()
                return

    def _messages(
        self,
        task: str,
        steps: list[Step],
        frame: Frame | None,
        note: str,
        context: Context | None = None,
    ) -> list[dict[str, Any]]:
        context = context or Context()
        lines = [f"Task: {task}"]
        if self.plan:
            lines.append(self.plan)
        if context.apps:
            lines.append(f"Installed apps: {context.apps}")
        if context.windows:
            lines.append("Open windows (topmost first):")
            lines.append(context.windows)
        if steps:
            lines.append("What you did so far:")
            lines.extend(s.line() for s in steps[-HISTORY:])
        if self.notes:
            lines.append("The owner added while you worked (follow it):")
            lines.extend(f"- {n}" for n in self.notes[-5:])
        if note:
            lines.append(note)
        if context.elements.startswith("Elements"):
            lines.append("Element list of the focused window:")
            lines.append(context.elements)
        if context.screen_text:
            lines.append("Text on the screen (OCR), usable with click_text:")
            lines.append(context.screen_text)
        content: list[dict[str, Any]] = []
        if frame is None:
            lines.append(
                "No screenshot this time: the element list covers the window. Use its ids, "
                'or reply {"action": "look"} to see the screen. What next?'
            )
        else:
            width, height = frame.sent_size
            lines.append(f"Here is the screen now ({width}x{height} pixels). What next?")
        content.append({"type": "text", "text": "\n".join(lines)})
        if frame is not None:
            content.append({"type": "image_url", "image_url": {"url": frame.data_url()}})
        return [
            {"role": "system", "content": system_prompt(self.coords)},
            {"role": "user", "content": content},
        ]

    def _act(self, n: int, thought: str, action: dict[str, Any], member: str, params: dict) -> Step:
        try:
            if member == "ask_claude":
                result = self._ask_claude(str(params["question"]))
            elif member == "wait":
                result = self._pause(float(params.get("duration", 1)))
            else:
                if member in INPUT_MEMBERS:
                    self._wait_for_owner()
                    self._bring_back()
                if member in ("type", "uia_set_value"):
                    self._not_twice(n, str(params.get("text", params.get("value", ""))))
                if member in POINT_CLICKS:
                    try:
                        self._inside_front(params)
                    except ActionError:
                        # The click missed the window, but the thought names a listed
                        # control: "click on the Full name field" (D-106).
                        meant = _intended_element(self.elements, thought)
                        if meant is None:
                            raise
                        params = {"coordinate": meant}
                if member == "type" and self._typing_again(n):
                    meant_id = _intended_field(self.elements, thought)
                    if meant_id is not None:
                        # A second type with no click between went into the field
                        # before it: the phone number landed in Email (D-106).
                        member = "uia_set_value"
                        params = {"element": meant_id, "value": params["text"]}
                if member in ("click_text", "uia_invoke"):
                    self._fields_first(params)
                if member == "click_text":
                    result = self._click_text(str(params["text"]))
                elif (
                    member == "focus_window"
                    and self._same_program(params)
                    and (self._back_to_handle())
                ):
                    result = ActionResult(f"{self._front_line().split(' · ')[0]} is in front")
                elif member == "uia_invoke":
                    result = self._invoke(str(params["element"]))
                else:
                    result = self.hands.run(member, params)
            text, error = result.text or "done", result.is_error
            if member in INPUT_MEMBERS:
                self.last_input = time.monotonic()
            if not error and member in ("focus_window", "open_url", "launch_app"):
                self._remember_front()
            if not error and member in ("type", "uia_set_value"):
                self.typed.setdefault(_typed_key(params), n)
            self.last_type_step = n if not error and member == "type" else 0
            if not error and member == "uia_set_value" and self._address_bar(params):
                # A search or address in the browser's bar does nothing until Enter,
                # and models kept pressing the search icon instead (D-097).
                self.hands.run("key", {"text": "enter"})
                text = f"{text} · Enter pressed"
        except Cancelled:
            text, error = "STOP: Stopped by the owner.", True
        except (KillSwitchError, ApprovalDeniedError, PromptUnavailableError) as exc:
            text, error = f"STOP: {exc}", True
        except ActionError as exc:
            text, error = f"failed: {exc}", True
        except Exception as exc:  # a UI Automation COMError ended a whole task (D-106)
            text, error = f"failed: {exc.__class__.__name__}: {exc}", True
        element = params.get("element")
        target = _picked(self.elements, element) if isinstance(element, str) else ""
        if not error and self.save_to and self.typed and _opens_save(member, params, self.elements):
            text = f"{text} · {self._fill_save_dialog()}"
        step = Step(n, thought, action, text, error, target)
        self.emit(
            {
                "event": "step",
                "step": n,
                "thought": thought,
                "action": describe(action),
                "say": plain(action, text, error, self.elements),
                "result": text,
            }
        )
        return step

    def _inside_front(self, params: Mapping[str, Any]) -> None:
        """Refuse a click that would land outside the window the task works in (D-106)."""
        check = getattr(self.hands, "outside_front", None)
        point = params.get("coordinate")
        if not callable(check) or not isinstance(point, (list, tuple)):
            return
        title = check(point)
        if title:
            raise ActionError(
                f"({point[0]}, {point[1]}) is outside {json.dumps(title, ensure_ascii=False)}, "
                "the window in front, so it was not clicked; use an element id from the "
                "list, or look at the screen again"
            )

    def _address_bar(self, params: Mapping[str, Any]) -> bool:
        label = _element_label(self.elements, str(params.get("element", "")))
        return label.strip('"') == ADDRESS_BAR

    def _ask_claude(self, question: str) -> ActionResult:
        """Claude's answer, or an error the model can work around: the local AI keeps going
        when Claude is signed out or not installed (D-067, D-093)."""
        if self.ask is None:
            raise ActionError("Claude can't be asked from here; decide from the screen")
        ask = self.ask
        self.emit({"event": "note", "text": "Asking Claude: " + question[:200]})
        try:
            answer = self._unless_stopped(lambda: ask(question)).strip()
        except Cancelled:
            raise
        except Exception as exc:
            self.emit({"event": "note", "text": "Claude isn't available, so the local AI decides."})
            raise ActionError(
                f"Claude isn't available ({str(exc)[:120]}); decide yourself from the screen"
            ) from exc
        if not answer:
            raise ActionError("Claude gave no answer; decide yourself from the screen")
        return ActionResult(answer[:MAX_TEXT])

    def _make_plan(self, task: str, apps: str) -> str:
        """Plan before acting, show it, and ask the owner first when only they can decide
        (D-094). Returns the plan as lines for the later prompts; "" when there is none."""
        windows = self._windows()
        lines = [f"Task: {task}"]
        if apps:
            lines.append(f"Installed apps: {apps}")
        if windows:
            lines += ["Open windows (topmost first):", windows]
        messages = [
            {"role": "system", "content": PLAN_PROMPT},
            {"role": "user", "content": [{"type": "text", "text": "\n".join(lines)}]},
        ]
        try:
            reply = self._unless_stopped(lambda: self.model(messages))
            plan = json.loads(reply[reply.find("{") : reply.rfind("}") + 1])
        except Cancelled:
            raise
        except Exception:
            return ""  # a task can still run without a plan
        if not isinstance(plan, dict):
            return ""
        steps = [str(x) for x in plan.get("steps") or [] if str(x).strip()][:6]
        # Plans kept adding "Save the file" to tasks that never said save, and the model
        # then followed the plan, not the task (D-106).
        steps = [x for x in steps if not UNASKED.search(x) or UNASKED.search(task)]
        goal, app = str(plan.get("goal") or ""), str(plan.get("app") or "")
        self.plan_app = app.strip()[:80]
        done_when = str(plan.get("done_when") or "")
        question = str(plan.get("question") or "").strip()
        default = str(plan.get("default") or "").strip()
        if not question and self.can_ask and SAVES.search(task) and not PLACE.search(task):
            # Only the owner knows where a file belongs, and models forget to ask (D-104).
            question, default = "Where should I save it?", "your Documents folder"
        if not (goal or steps or question):
            return ""
        self.emit(
            {"event": "plan", "goal": goal, "app": app, "steps": steps, "done_when": done_when}
        )
        if steps:
            numbered = " ".join(f"{i}) {x}" for i, x in enumerate(steps, 1))
            self.emit({"event": "note", "text": f"Plan: {numbered}"})
        text = [f"Your plan: {goal} in {app}." if app else f"Your plan: {goal}."]
        text += [f"{i}. {x}" for i, x in enumerate(steps, 1)]
        if done_when:
            text.append(f"Done when: {done_when}")
            self.done_when = done_when
        if question:
            answer = with_paths(self._ask_owner(question, default))
            self.save_to = self.save_to or save_target(task, answer)
            text.append(f"You asked the owner: {question} {answer}")
        return "\n".join(text)

    def _ask_owner(self, question: str, default: str) -> str:
        """Ask in the chat and wait for the owner's answer (it comes in like a note)."""
        if not self.can_ask:
            return f"Nobody can answer here; use the default: {default or 'choose sensibly'}"
        self.emit({"event": "question", "text": question, "default": default})
        self.emit({"event": "note", "text": question})
        until = time.monotonic() + QUESTION_WAIT
        while time.monotonic() < until:
            if self.cancelled():
                raise Cancelled
            fresh = self.inbox()
            if fresh:
                return "The owner answered: " + " ".join(fresh)
            time.sleep(CANCEL_POLL * 4)
        if default:
            self.emit({"event": "note", "text": f"No answer, so Relay goes with: {default}"})
            return f"No answer; use the default: {default}"
        return "No answer; choose sensibly and say what you chose."

    def _unless_stopped(self, work: Callable[[], str]) -> str:
        """Run a slow call (the model, Claude) and give up on it the moment Stop is pressed.

        The call itself finishes in the background; its answer is dropped.
        """
        box: dict[str, Any] = {}

        def run() -> None:
            try:
                box["value"] = work()
            except BaseException as exc:  # handed back to the caller below
                box["error"] = exc

        thread = threading.Thread(target=run, name="relay-operator-call", daemon=True)
        thread.start()
        while thread.is_alive():
            if self.cancelled():
                raise Cancelled
            thread.join(CANCEL_POLL)
        if "error" in box:
            raise box["error"]
        return box["value"]

    def _pause(self, seconds: float) -> ActionResult:
        until = time.monotonic() + min(max(seconds, 0.0), 10.0)
        while time.monotonic() < until:
            if self.cancelled():
                raise Cancelled
            time.sleep(min(CANCEL_POLL, max(until - time.monotonic(), 0)))
        return ActionResult(f"Waited {seconds:g} s")

    def _invoke(self, element: str) -> ActionResult:
        """Press an element; one with no press action (an address bar) gets a click on its
        center instead, so the model doesn't have to work that out (D-093)."""
        option = _dropdown_option(self.elements, element)
        if option is not None:
            return self._pick_option(*option)
        try:
            return self.hands.run("uia_invoke", {"element": element})
        except ActionError as exc:
            point = _center(self.elements, element)
            if "click its center" not in str(exc) or point is None:
                raise
        self._bring_back()
        return self.hands.run("left_click", {"coordinate": point})

    def _pick_option(self, name: str, box: list[int]) -> ActionResult:
        """Choose a dropdown option the way a person does: open the box, type the
        option's name, press Enter (D-106). Invoking or selecting a web page's option
        said OK but left "Choose..." picked, and the form went without its colour."""
        self._wait_for_owner()
        self._bring_back()
        self.hands.run("key", {"text": "Escape"})  # a popup left open from before
        self.hands.run("left_click", {"coordinate": box})
        self.hands.run("type", {"text": name})
        result = self.hands.run("key", {"text": "Return"})
        self.last_input = time.monotonic()
        return ActionResult(f"Picked {json.dumps(name, ensure_ascii=False)} · {result.text}")

    def _wait_for_owner(self) -> None:
        """Let the owner finish what they are doing before Relay moves the real mouse
        or types (D-100). UIA invoke and set_value never come here: they need neither."""
        told = False
        until = time.monotonic() + OWNER_PATIENCE
        while time.monotonic() < until:
            quiet = self.idle()
            if quiet is None:
                return
            since_relay = time.monotonic() - self.last_input
            if quiet >= OWNER_QUIET or quiet >= since_relay - 0.05:
                return  # nobody but Relay touched the input lately
            if not told:
                told = True
                self.emit(
                    {"event": "note", "text": "Waiting for you to pause before typing or clicking."}
                )
            if self.cancelled():
                raise Cancelled
            time.sleep(CANCEL_POLL * 2)

    def _front_line(self) -> str:
        for line in self._windows().splitlines():
            if "in front" in line.split(" · "):
                return line
        return ""

    def _remember_front(self) -> None:
        """The program the task now works in, so it can be brought back (D-093).

        A writing task in an editor gets a new tab or document first, so the owner's
        open files are never typed over (D-094).
        """
        parts = self._front_line().split(" · ")
        if len(parts) > 2 and not any(p.startswith("Relay itself") for p in parts):
            self.target = parts[1].casefold().removesuffix(".exe")
            front = getattr(self.hands, "front_handle", None)
            self.target_handle = front() if callable(front) else 0
            if self.new_doc and self.target in EDITORS and self.target not in self.fresh:
                self.fresh.add(self.target)
                with contextlib.suppress(ActionError):
                    self.hands.run("key", {"text": "ctrl+n"})
                    self.emit(
                        {
                            "event": "note",
                            "text": "Opened a new tab so your open files stay as they are.",
                        }
                    )

    def _not_twice(self, n: int, text: str) -> None:
        """Typing the same text again is how a list ended up in Notepad twice (D-094)."""
        first = self.typed.get(_typed_key({"text": text}))
        if first is not None and len(text.strip()) >= REPEAT_MIN:
            raise ActionError(
                f"this text was already typed in step {first}; look at the screen and "
                "go on from what is there instead of typing it again"
            )

    def _bring_back(self) -> None:
        """Put the task's program in front again before real input, in case the owner
        clicked or typed somewhere else in between (D-093)."""
        if not self.target:
            return
        if self._back_to_handle():
            return
        parts = self._front_line().split(" · ")
        stem = parts[1].casefold().removesuffix(".exe") if len(parts) > 1 else ""
        if stem == self.target and not any(p.startswith("Relay itself") for p in parts):
            return
        with contextlib.suppress(ActionError):
            self.hands.run("focus_window", {"window": self.target})

    def _typing_again(self, n: int) -> bool:
        """The step before this one also typed, so the focus is still on that field."""
        return bool(self.last_type_step) and self.last_type_step == n - 1

    def _fields_first(self, params: Mapping[str, Any]) -> None:
        """Before Next or Submit, point out fields the task names that are still empty.

        The job form went to its last page without "6 years of experience" (D-106).
        Each field is pointed out once, so a field the model can't fill never blocks.
        """
        label = str(params.get("text", ""))
        if "element" in params:
            label = _element_label(self.elements, str(params["element"])).strip('"')
        if not SUBMITS.match(label.strip()):
            return
        empty = [f for f in empty_fields(self.elements, self.task) if f not in self.warned]
        if empty:
            self.warned.update(empty)
            names = ", ".join(f'"{f}"' for f in empty)
            raise ActionError(
                f"not pressed yet: the task gives a value for {names}, still empty on "
                "this page; fill it first, then press it"
            )

    def _same_program(self, params: Mapping[str, Any]) -> bool:
        """focus_window names the program the task already works in, not a title."""
        name = str(params.get("window", "")).casefold().removesuffix(".exe").strip()
        return bool(self.target) and name in (self.target, self.target.replace("_", " "))

    def _back_to_handle(self) -> bool:
        """The exact window the task works in, in front again (D-106). With two Notepad
        windows open, focusing "Notepad" brought the owner's other document."""
        back = getattr(self.hands, "bring_back", None)
        if not self.target_handle or not callable(back):
            return False
        with contextlib.suppress(Exception):
            return bool(back(self.target_handle))
        return False

    def _click_text(self, text: str) -> ActionResult:
        found = self.hands.run("find_text", {"text": text}).text or ""
        points = [[int(x), int(y)] for x, y in re.findall(r"center \((\d+), (\d+)\)", found)]
        if not points:
            listed = _listed_named(self.elements, text)
            if listed is not None:
                # OCR reads only the active display; the Explorer window on the other
                # monitor listed "incoming" but OCR never saw it (D-106).
                return self._invoke(listed)
            raise ActionError(f"no text like {text!r} on the screen")
        check = getattr(self.hands, "outside_front", None)
        inside = [p for p in points if not (callable(check) and check(p))]
        if not inside:
            # Text in another program, like "Notepad" in a chat beside the browser (D-106).
            raise ActionError(
                f"{text!r} is only outside the window in front; open or focus the right "
                "program first (launch_app or focus_window)"
            )
        return self.hands.run("left_click", {"coordinate": inside[0]})

    def _fill_save_dialog(self) -> str:
        """Type the full path into the Save dialog's File name box and press Enter (D-106).

        Models clicked folders in the dialog's side pane again and again; the path the
        task and the owner gave is enough on its own.
        """
        for _ in range(SAVE_DIALOG_TRIES):
            self._pause(SAVE_DIALOG_WAIT)
            try:
                found = self.hands.run("uia_find", {"query": "File name", "role": "Edit"}).text
            except ActionError:
                continue
            box = re.search(r"^\[(e\d+)\] Edit", found or "", re.M)
            if box is None:
                continue
            self.hands.run("uia_set_value", {"element": box.group(1), "value": self.save_to})
            self.hands.run("key", {"text": "enter"})
            self.typed.setdefault(_typed_key({"text": self.save_to}), 0)
            saved, self.save_to = self.save_to, ""
            return f"Saved as {saved}"
        return "no Save dialog came up"

    @staticmethod
    def _stuck(steps: list[Step]) -> bool:
        last = steps[-STUCK_REPEATS:]
        if len(last) < STUCK_REPEATS:
            return False
        if (
            len(steps) >= 2 * SWITCHES
            and _switching(steps[-2 * SWITCHES : -SWITCHES])
            and (_switching(steps))
        ):
            return True  # still only switching windows after being told to act
        first = last[0]
        if first.target and all(
            s.target == first.target and s.action["action"] == first.action["action"] for s in last
        ):
            return True  # "Backend developer" invoked under a new id each step (D-106)
        same = all(s.action == first.action for s in last)
        if same and all(s.error for s in last):
            return True  # the same refused click, twenty times in round 7 (D-106)
        if same and first.action.get("action") in REPEAT_IS_STUCK:
            return True  # focusing the window it is in, again and again (D-106)
        return same and all("no visible change" in s.result for s in last)

    def _finish(self, status: Status, message: str, steps: list[Step]) -> Outcome:
        self.emit({"event": status, "message": message, "steps": len(steps)})
        return Outcome(status, message, steps)
