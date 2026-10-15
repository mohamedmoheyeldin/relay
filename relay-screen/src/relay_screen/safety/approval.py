"""Approval gate (D-011, D-019, D-024, D-073).

Mode "risky" asks only before acts that look irreversible: pressing or
clicking a control named like send, submit, pay, buy, delete and so on. Mode "ask"
asks before every act, and "off" never asks. Relay asks with its own prompt window,
which ignores synthetic input, so Claude can never approve its own actions. In a
batch the gate asks before each action; the owner may approve the rest of the batch
in one go.
"""

from __future__ import annotations

import base64
import json
import re
import subprocess
import sys
import threading
from collections.abc import Callable, Mapping
from dataclasses import dataclass, field
from typing import Any, Literal, Protocol

from relay_screen.errors import ActionError

Decision = Literal["approve", "approve_batch", "deny", "timeout"]
Mode = Literal["risky", "ask", "off"]

CLICKS = frozenset({"left_click", "right_click", "middle_click", "double_click", "triple_click"})
# Words on a control that mean the click can't be taken back (D-073).
RISKY = re.compile(
    r"\b(send|submit|pay|payment|buy|purchase|order|checkout|check out|delete|remove|"
    r"erase|uninstall|format|transfer|confirm|publish|post|reply|discard|empty|reset|"
    r"book|reserve|subscribe|unsubscribe|donate|place|sign|agree|accept|install)\b",
    re.IGNORECASE,
)

PROMPT_TIMEOUT = 120.0  # s before an unanswered prompt counts as denied
TEXT_LIMIT = 300


class ApprovalDeniedError(ActionError):
    """The owner said no, or did not answer in time."""


class PromptUnavailableError(ActionError):
    """Relay could not show its approval prompt, so nobody was asked."""

    def __init__(self, reason: str) -> None:
        super().__init__(
            f"Relay could not show its approval prompt ({reason}), so this was not run. "
            "The owner was not asked and did not refuse. Tell them Relay's approval "
            "prompt is broken; Settings > Diagnostics shows the details."
        )


@dataclass
class PromptRequest:
    title: str
    detail: str
    step: int | None = None
    total: int | None = None
    image: bytes | None = None  # PNG preview of the target, optional
    timeout: float = PROMPT_TIMEOUT
    approve_label: str = "Approve"
    allow_batch: bool = False
    batch_label: str = "Approve rest of batch"


class Prompter(Protocol):
    def ask(self, request: PromptRequest) -> Decision: ...


@dataclass
class ActRequest:
    """What the gate needs to know about one act."""

    name: str
    params: Mapping[str, Any]
    step: int | None = None  # 1-based position in a batch
    total: int | None = None
    preview: Callable[[], bytes | None] = lambda: None
    summary: str | None = None  # plain words when params alone don't say enough


def _quote(text: str) -> str:
    shown = text if len(text) <= TEXT_LIMIT else text[:TEXT_LIMIT] + "…"
    return json.dumps(shown, ensure_ascii=False)


def _point(params: Mapping[str, Any], key: str = "coordinate") -> str | None:
    point = params.get(key)
    if isinstance(point, (list, tuple)) and len(point) == 2:
        return f"({point[0]}, {point[1]})"
    return None


def _at(params: Mapping[str, Any], key: str = "coordinate") -> str:
    point = _point(params, key)
    return f" at {point}" if point else ""


def _with(params: Mapping[str, Any]) -> str:
    text = params.get("text")
    return f" holding {text}" if text else ""


def is_risky(act: ActRequest) -> bool:
    """Whether an act presses a control whose name says it can't be undone (D-073).

    Clicks are judged by the UI Automation name of the control under the pointer
    (Computer puts it in the summary); a click where UIA sees nothing is not asked.
    """
    if act.name not in CLICKS and act.name != "uia_invoke":
        return False
    if not act.summary:
        return False
    target = act.summary.split(" on ", 1)[-1] if act.name in CLICKS else act.summary
    return RISKY.search(target) is not None


def describe(name: str, params: Mapping[str, Any]) -> str:
    """One line in plain words, coordinates in screenshot pixels."""
    match name:
        case "left_click" | "right_click" | "middle_click" | "double_click" | "triple_click":
            verb = name.replace("_", " ")
            where = _at(params) or " where the mouse is"
            return f"{verb.capitalize()}{where}{_with(params)}"
        case "mouse_move":
            return f"Move the mouse{_at(params)}"
        case "left_click_drag":
            start = _point(params, "start_coordinate")
            return f"Drag from {start} to {_point(params)}{_with(params)}"
        case "left_mouse_down":
            return "Press and hold the left mouse button"
        case "left_mouse_up":
            return "Release the left mouse button"
        case "scroll":
            amount = params.get("scroll_amount")
            direction = params.get("scroll_direction")
            return f"Scroll {direction} {amount} notches{_at(params)}{_with(params)}"
        case "type":
            return f"Type {_quote(str(params.get('text', '')))}"
        case "key":
            repeat = params.get("repeat")
            times = f" {repeat} times" if isinstance(repeat, int) and repeat > 1 else ""
            return f"Press {params.get('text')}{times}"
        case "hold_key":
            return f"Hold {params.get('text')} for {params.get('duration')} s"
        case _:
            return f"{name} {json.dumps(dict(params), ensure_ascii=False)[:TEXT_LIMIT]}"


@dataclass
class ApprovalGate:
    prompter: Prompter
    mode: Mode = "ask"
    timeout: float = PROMPT_TIMEOUT
    _batch_approved: bool = field(default=False, init=False)
    _lock: threading.Lock = field(default_factory=threading.Lock, init=False)

    def check(self, act: ActRequest) -> str:
        """Ask before an act. Returns how it was approved; raises if it was not."""
        if self.mode == "off":
            return "not asked (approval off)"
        if self.mode == "risky" and not is_risky(act):
            return "not asked (not risky)"
        in_batch = act.total is not None and act.total > 1
        if act.step in (None, 1):
            self._batch_approved = False
        if in_batch and self._batch_approved:
            return "approved (rest of batch)"
        request = PromptRequest(
            title="Relay wants to act on your PC",
            detail=act.summary or describe(act.name, act.params),
            step=act.step if in_batch else None,
            total=act.total if in_batch else None,
            image=act.preview(),
            timeout=self.timeout,
            allow_batch=in_batch and act.step is not None and act.step < (act.total or 0),
        )
        with self._lock:
            decision = self.prompter.ask(request)
        if decision == "approve_batch" and in_batch:
            self._batch_approved = True
            return "approved (rest of batch)"
        if decision in ("approve", "approve_batch"):
            return "approved"
        if decision == "timeout":
            raise ApprovalDeniedError(
                f"No answer from the owner within {self.timeout:g} s, so this action was "
                "not run. Ask them in chat before trying again."
            )
        raise ApprovalDeniedError(
            "The owner denied this action. Do not retry it; ask them what to do instead."
        )

    def confirm(self, title: str, detail: str, approve_label: str = "Approve") -> bool:
        """Ask the owner a yes/no question in the same prompt (used by rearm and approve)."""
        return self.ask(title, detail, approve_label) in ("approve", "approve_batch")

    def ask(
        self, title: str, detail: str, approve_label: str = "Approve", always_label: str = ""
    ) -> Decision:
        """Ask the owner; with `always_label`, a third button answers "approve_batch"."""
        request = PromptRequest(
            title=title,
            detail=detail,
            timeout=self.timeout,
            approve_label=approve_label,
            allow_batch=bool(always_label),
            batch_label=always_label or "Approve rest of batch",
        )
        with self._lock:
            return self.prompter.ask(request)


class SubprocessPrompter:
    """Shows the native prompt (safety/prompt.py) in a child process and waits.

    Raises PromptUnavailableError when the prompt could not be shown, so a broken
    prompt is never reported to Claude as the owner saying no.
    """

    def ask(self, request: PromptRequest) -> Decision:
        from ctypes import wintypes

        from relay_screen import win32

        user32 = win32.user32()
        user32.GetForegroundWindow.restype = wintypes.HWND
        payload = {
            "title": request.title,
            "detail": request.detail,
            "step": request.step,
            "total": request.total,
            "image": base64.b64encode(request.image).decode() if request.image else None,
            "timeout": request.timeout,
            "approve_label": request.approve_label,
            "allow_batch": request.allow_batch,
            "batch_label": request.batch_label,
            "restore_hwnd": user32.GetForegroundWindow() or 0,
        }
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        try:
            done = subprocess.run(
                [sys.executable, "-m", "relay_screen.safety.prompt"],
                input=json.dumps(payload),
                capture_output=True,
                text=True,
                encoding="utf-8",
                timeout=request.timeout + 15,
                creationflags=flags,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return "timeout"
        except OSError as exc:
            raise PromptUnavailableError(f"could not start it: {exc}") from exc
        try:
            answer = json.loads(done.stdout or "{}")
        except ValueError:
            answer = {}
        decision = answer.get("decision")
        if decision in ("approve", "approve_batch", "deny", "timeout"):
            return decision
        lines = [line for line in (done.stderr or "").splitlines() if line.strip()]
        reason = answer.get("error") or (
            lines[-1].strip() if lines else f"exit code {done.returncode}"
        )
        raise PromptUnavailableError(str(reason))
