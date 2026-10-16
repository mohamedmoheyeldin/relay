"""App-function tests (D-101): Relay's own window driven through UI Automation, with no
model, the way the owner uses it. New chat, a Claude reply, the chat menu (pin, rename,
delete), search, the model and effort picker, attach, Stop and every Settings page.

Each script returns the checks that failed. The chats it makes are deleted at the end,
and the model and effort picked in the composer are put back as they were.
"""

from __future__ import annotations

import json
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol


class AppUi(Protocol):
    """Relay's main window. Names match by prefix, case-insensitively, exact first."""

    def ready(self) -> str | None: ...  # why the window can't be used, or None

    def find(self, name: str, role: str | None = None) -> list[str]: ...

    def press(self, name: str, role: str | None = None) -> str: ...  # LookupError if absent

    def fill(self, name: str, text: str) -> None: ...  # focus, select all, type

    def key(self, keys: str) -> None: ...  # "{Enter}", "{Esc}"

    def dialog(self) -> str | None: ...  # an open file dialog's title


# Settings pages and words on each one (from relay-ui/src/screens/settings.tsx).
PAGES = (
    ("General", "Startup, the tray and language."),
    ("Account", "Your Claude sign-in"),
    ("Local AI", "The models that run on your GPU"),
    ("Connectors", "MCP servers and connectors"),
    ("Safety", "Approvals, the kill switch"),
    ("Playbooks", "Tasks Relay has learned to repeat."),
    ("Privacy", "Activity logs and screenshots."),
    ("Updates", "Relay's version and updates."),
    ("Developer", "Health checks, full test runs"),
)
MODEL_LABELS = {"": "Default", "opus": "Opus", "sonnet": "Sonnet", "haiku": "Haiku"}
EFFORT_LABELS = {"": "Default", "low": "Low", "medium": "Medium", "high": "High", "max": "Max"}
QUESTION = "Reply with the number seven times six, digits only."
LONG_ASK = "Write a 600-word story about a lighthouse keeper who finds a radio."


@dataclass
class Ctx:
    ui: AppUi
    clock: Callable[[], float]
    sleep: Callable[[float], None]
    shot: Callable[[str], object]
    event: Callable[[dict[str, Any]], object]
    settings_file: Path | None
    stamp: str
    claude: bool  # Claude Code is signed in, so chats get replies
    failed: list[str] = field(default_factory=list)
    checks: list[dict[str, Any]] = field(default_factory=list)

    def wait(self, name: str, role: str | None = None, *, gone: bool = False, seconds=10.0):
        """True once `name` shows (or is gone), polling every quarter second."""
        until = self.clock() + seconds
        while True:
            there = bool(self.ui.find(name, role))
            if there != gone:
                return True
            if self.clock() >= until:
                return False
            self.sleep(0.25)

    def expect(self, ok: bool, what: str) -> bool:
        check = {"what": what, "ok": ok}
        self.checks.append(check)
        self.event({"event": "check", **check})
        if not ok:
            self.failed.append(what)
        return ok

    def press(self, name: str, role: str | None = None, *, seconds: float = 5.0) -> bool:
        """Press `name` once it shows; a missing control is a failed check."""
        if not self.wait(name, role, seconds=seconds):
            return self.expect(False, f"{role or 'control'} {name!r} is there")
        try:
            pressed = self.ui.press(name, role)
        except LookupError:
            return self.expect(False, f"{role or 'control'} {name!r} is there")
        self.event({"event": "press", "name": pressed})
        self.sleep(0.3)
        return True

    def settings(self) -> dict[str, Any]:
        if self.settings_file is None or not self.settings_file.is_file():
            return {}
        try:
            data = json.loads(self.settings_file.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}

    def claude_setting(self, key: str) -> str:
        claude = self.settings().get("claude")
        return str(claude.get(key) or "") if isinstance(claude, dict) else ""

    def until_setting(self, key: str, want: str, seconds: float = 5.0) -> bool:
        until = self.clock() + seconds
        while self.claude_setting(key) != want:
            if self.clock() >= until:
                return False
            self.sleep(0.25)
        return True


def options(title: str) -> str:
    """The chat menu button's name for a chat titled `title`."""
    return f"Options for {title}"


def delete_chat(ctx: Ctx, title: str) -> None:
    """Delete every chat whose title starts with `title` (left over or just made)."""
    for _ in range(5):
        if not ctx.ui.find(options(title), "Button"):
            return
        ctx.press(options(title), "Button")
        ctx.press("Delete", "MenuItem")
        ctx.wait(options(title), "Button", gone=True, seconds=3)


def new_chat(ctx: Ctx) -> bool:
    return ctx.press("New chat", "Button") and ctx.expect(
        ctx.wait("Message", "Edit"), "New chat shows an empty message box"
    )


def send(ctx: Ctx, text: str) -> bool:
    try:
        ctx.ui.fill("Message", text)
    except LookupError:
        return ctx.expect(False, "the message box is there")
    return ctx.press("Send", "Button")


# -- scripts ------------------------------------------------------------------------


def settings_pages(ctx: Ctx) -> None:
    if not ctx.press("Settings", "Button"):
        return
    for label, words in PAGES:
        if ctx.press(label, "Button"):
            ctx.expect(ctx.wait(words, seconds=5), f"Settings > {label} opens")
            ctx.shot(f"settings-{label.lower().replace(' ', '-')}")


def model_picker(ctx: Ctx) -> None:
    """The model and effort menu next to Send opens, saves a pick and shows a check."""
    model, effort = ctx.claude_setting("model"), ctx.claude_setting("effort")
    if not new_chat(ctx):
        return
    picks = (("model", "Sonnet", "sonnet"), ("effort", "High", "high"))
    for key, label, value in picks:
        if not ctx.press("Model and effort", "Button"):
            return
        opened = ctx.wait(label, "MenuItem", seconds=3)
        ctx.expect(opened, "the model and effort menu opens")
        if not opened:
            ctx.ui.key("{Esc}")
            return
        ctx.shot(f"picker-{key}")
        ctx.press(label, "MenuItem")
        ctx.expect(ctx.until_setting(key, value), f"picking {label} saves {key} = {value}")
    # Put both back the way they were (Default is the first item, the model's).
    for key, value, labels in (("model", model, MODEL_LABELS), ("effort", effort, EFFORT_LABELS)):
        label = labels.get(value, value)
        if not ctx.press("Model and effort", "Button"):
            return
        if label == "Default":  # the model's reads "Default Claude Code's choice"
            names = ctx.ui.find("Default", "MenuItem")
            longer = [n for n in names if n.strip().casefold() != "default"]
            plain = [n for n in names if n.strip().casefold() == "default"]
            choose = longer if key == "model" else plain
            label = choose[0] if choose else label
        ctx.press(label, "MenuItem")
        ctx.expect(ctx.until_setting(key, value), f"{key} is back to {value or 'default'}")


def chat_functions(ctx: Ctx) -> None:
    """New chat, a reply, then the chat menu: pin, rename, search and delete."""
    title = f"Relay test {ctx.stamp}"
    delete_chat(ctx, QUESTION[:40])
    if not new_chat(ctx) or not send(ctx, QUESTION):
        return
    if ctx.claude:
        ctx.expect(ctx.wait("42", seconds=120), "Claude's reply (42) shows in the chat")
    ctx.shot("reply")
    if not ctx.expect(
        ctx.wait(options(QUESTION[:40]), "Button", seconds=20), "the chat is in the sidebar"
    ):
        return
    ctx.press(options(QUESTION[:40]), "Button")
    if ctx.press("Pin", "MenuItem"):
        ctx.expect(ctx.wait("Pinned", seconds=3), "Pin puts the chat under Pinned")
    ctx.press(options(QUESTION[:40]), "Button")
    if ctx.press("Rename", "MenuItem"):
        try:
            ctx.ui.fill("Chat name", title)
            ctx.ui.key("{Enter}")
        except LookupError:
            ctx.expect(False, "Rename shows a name box")
        ctx.expect(ctx.wait(options(title), "Button", seconds=3), "Rename changes the title")
    ctx.shot("renamed")
    if ctx.press("Search", "Button"):
        try:
            ctx.ui.fill("Search chats", ctx.stamp)
            ctx.expect(ctx.wait(options(title), "Button", seconds=3), "Search finds the chat")
            ctx.ui.fill("Search chats", "no chat is called this zq")
            ctx.expect(
                ctx.wait(options(title), "Button", gone=True, seconds=3),
                "Search hides chats that don't match",
            )
        except LookupError:
            ctx.expect(False, "Search shows a search box")
        ctx.shot("search")
        ctx.press("Close search", "Button")
    ctx.press(options(title), "Button")
    if ctx.press("Unpin", "MenuItem"):
        ctx.expect(ctx.wait("Pinned", gone=True, seconds=3), "Unpin takes it out of Pinned")
    delete_chat(ctx, title)
    ctx.expect(not ctx.ui.find(options(title), "Button"), "Delete removes the chat")


def attach(ctx: Ctx) -> None:
    if not new_chat(ctx) or not ctx.press("Attach files", "Button"):
        return
    until = ctx.clock() + 8
    title = ctx.ui.dialog()
    while title is None and ctx.clock() < until:
        ctx.sleep(0.25)
        title = ctx.ui.dialog()
    ctx.expect(title is not None, "Attach opens a file picker")
    ctx.shot("attach")
    if title is not None:
        ctx.ui.key("{Esc}")
        until = ctx.clock() + 5
        while ctx.ui.dialog() is not None and ctx.clock() < until:
            ctx.sleep(0.25)
        ctx.expect(ctx.ui.dialog() is None, "Esc closes the file picker")


def stop(ctx: Ctx) -> None:
    """A long Claude answer stops with Stop, and the composer is ready again."""
    delete_chat(ctx, LONG_ASK[:40])
    if not new_chat(ctx) or not send(ctx, LONG_ASK):
        return
    if ctx.expect(ctx.wait("Stop", "Button", seconds=15), "Stop shows while Relay works"):
        ctx.shot("working")
        ctx.press("Stop", "Button")
        ctx.expect(ctx.wait("Stop", "Button", gone=True, seconds=15), "Stop ends the answer")
        ctx.expect(ctx.wait("Send", "Button", seconds=5), "Send is back after Stop")
    ctx.shot("stopped")
    delete_chat(ctx, LONG_ASK[:40])
    # Leave Relay where a full test from the app was started: Settings > Developer.
    if ctx.press("Settings", "Button"):
        ctx.press("Developer", "Button")


SCRIPTS: dict[str, Callable[[Ctx], None]] = {
    "app-settings": settings_pages,
    "app-model-picker": model_picker,
    "app-chat": chat_functions,
    "app-attach": attach,
    "app-stop": stop,
}
# Scripts that need Claude signed in (a reply, or something to stop).
NEEDS_CLAUDE = {"app-stop"}
