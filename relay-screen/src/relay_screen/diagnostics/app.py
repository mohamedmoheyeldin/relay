"""App-function tests (D-101, D-103): Relay's own window driven the way the owner uses
it, with real mouse clicks and keys, no model. Every action is one click (or key) and is
timed from the click until its result shows, so a control that needs two clicks, or a
slow screen, fails.

Covered: navigation (back, forward, collapsing the sidebar), every Settings page and
toggle (saved, still set after leaving the page, put back), the composer (empty, Enter,
Shift+Enter, a long message, the model and effort picker), chats (new, switch, pin,
rename, search, unpin, delete, and all of it still true after Relay restarts), attach,
Stop and a smaller window.

The chats a script makes are deleted, and every setting it changes is put back.
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

    def press(self, name: str, role: str | None = None) -> str: ...  # one real click

    def fill(self, name: str, text: str) -> None: ...  # click in, select all, type

    def key(self, keys: str) -> None: ...  # "{Enter}", "{Esc}", "{Shift}{Enter}"

    def value(self, name: str, role: str | None = None) -> str | None: ...

    def toggled(self, name: str) -> bool | None: ...

    def dialog(self) -> str | None: ...  # an open file dialog's title

    def restart(self) -> str | None: ...  # close Relay and start it again; why not

    def size(self) -> tuple[int, int, bool]: ...  # width, height, maximized

    def resize(self, width: int, height: int, maximized: bool = False) -> None: ...

    def inside(self, name: str, role: str | None = None) -> bool: ...  # fully in view


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
# Every toggle in Settings: its page, its label and where settings.json keeps it.
TOGGLES = (
    ("General", "Start Relay when I sign in to Windows", ("general", "start_at_login")),
    ("Safety", "Send approval requests to my phone", ("safety", "phone_approvals")),
    ("Updates", "Check for updates when Relay starts", ("updates", "check_on_start")),
)
MODEL_LABELS = {"": "Default", "opus": "Opus", "sonnet": "Sonnet", "haiku": "Haiku"}
EFFORT_LABELS = {"": "Default", "low": "Low", "medium": "Medium", "high": "High", "max": "Max"}
QUESTION = "Reply with the number seven times six, digits only."
SECOND = "Reply with the word blue, nothing else."
LONG_ASK = "Write a 600-word story about a lighthouse keeper who finds a radio."
LONG_MESSAGE = "Reply with OK only. " + "This sentence is filler for a long message. " * 70
# Titles of every chat these scripts make (a title is the start of its first message).
MADE = (QUESTION[:40], SECOND[:30], LONG_ASK[:40], LONG_MESSAGE[:30], "Relay test ")
ONE_CLICK = 5.0  # seconds a click's result may take to show before it counts as not working
SLOW_MS = 2500  # a result that shows later than this is reported as slow


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

    # -- checks -------------------------------------------------------------------

    def expect(self, ok: bool, what: str, ms: int | None = None) -> bool:
        check: dict[str, Any] = {"what": what, "ok": ok}
        if ms is not None:
            check["ms"] = ms
        self.checks.append(check)
        self.event({"event": "check", **check})
        if not ok:
            self.failed.append(what)
        return ok

    def until(self, done: Callable[[], bool], seconds: float) -> int | None:
        """Milliseconds until `done()` held, or None if it didn't within `seconds`."""
        start = self.clock()
        while True:
            if done():
                return round((self.clock() - start) * 1000)
            if self.clock() - start >= seconds:
                return None
            self.sleep(0.1)

    def shows(self, name: str, role: str | None = None) -> Callable[[], bool]:
        return lambda: bool(self.ui.find(name, role))

    def gone(self, name: str, role: str | None = None) -> Callable[[], bool]:
        return lambda: not self.ui.find(name, role)

    def wait(self, name: str, role: str | None = None, *, gone: bool = False, seconds=10.0):
        test = self.gone(name, role) if gone else self.shows(name, role)
        return self.until(test, seconds) is not None

    def act(
        self,
        what: str,
        name: str,
        role: str | None,
        then: Callable[[], bool],
        seconds: float = ONE_CLICK,
    ) -> bool:
        """Click `name` once and time it until `then()` holds: works on one click, fast."""
        if not self.wait(name, role, seconds=5):
            return self.expect(False, f"{what}: {role or 'control'} {name!r} is there")
        start = self.clock()
        try:
            pressed = self.ui.press(name, role)
        except LookupError:
            return self.expect(False, f"{what}: {role or 'control'} {name!r} is there")
        self.event({"event": "press", "name": pressed})
        took = self.until(then, seconds)
        ms = None if took is None else round((self.clock() - start) * 1000)
        if not self.expect(took is not None, f"{what} on one click", ms):
            return False
        if ms is not None and ms > SLOW_MS:
            self.expect(False, f"{what} took {ms} ms (slow)", ms)
        return True

    def keys(self, what: str, keys: str, then: Callable[[], bool], seconds=ONE_CLICK) -> bool:
        start = self.clock()
        self.ui.key(keys)
        took = self.until(then, seconds)
        ms = None if took is None else round((self.clock() - start) * 1000)
        return self.expect(took is not None, what, ms)

    # -- settings.json ------------------------------------------------------------

    def settings(self) -> dict[str, Any]:
        if self.settings_file is None or not self.settings_file.is_file():
            return {}
        try:
            data = json.loads(self.settings_file.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        return data if isinstance(data, dict) else {}

    def setting(self, path: tuple[str, str]) -> Any:
        section = self.settings().get(path[0])
        return section.get(path[1]) if isinstance(section, dict) else None

    def saved(self, path: tuple[str, str], want: Any) -> Callable[[], bool]:
        return lambda: (self.setting(path) or type(want)()) == want


def options(title: str) -> str:
    """The chat menu button's name for a chat titled `title`."""
    return f"Options for {title}"


def delete_chat(ctx: Ctx, title: str) -> bool:
    """Delete every chat whose title starts with `title`; False if a Delete didn't."""
    ok = True
    for _ in range(6):
        count = len(ctx.ui.find(options(title), "Button"))
        if not count:
            return ok
        if not ctx.act(
            "Chat menu opens", options(title), "Button", ctx.shows("Delete", "MenuItem")
        ):
            ctx.ui.key("{Esc}")
            return False
        ok = (
            ctx.act(
                f"Delete removes {title.strip()!r}",
                "Delete",
                "MenuItem",
                lambda n=count: len(ctx.ui.find(options(title), "Button")) < n,
            )
            and ok
        )
    return ok


def clean_up(ctx: Ctx) -> None:
    """Chats an earlier, interrupted run left behind (not checked)."""
    for title in MADE:
        for _ in range(6):
            if not ctx.ui.find(options(title), "Button"):
                break
            ctx.ui.press(options(title), "Button")
            if not ctx.wait("Delete", "MenuItem", seconds=2):
                ctx.ui.key("{Esc}")
                break
            ctx.ui.press("Delete", "MenuItem")
            ctx.sleep(1.0)


def new_chat(ctx: Ctx) -> bool:
    if ctx.ui.find("Open sidebar", "Button"):
        ctx.ui.press("Open sidebar", "Button")
        ctx.wait("New chat", "Button", seconds=3)
    return ctx.act("New chat", "New chat", "Button", ctx.shows("Message", "Edit"))


def send(ctx: Ctx, text: str, title: str) -> bool:
    """Type `text` and send it with Enter; the chat shows in the sidebar."""
    try:
        ctx.ui.fill("Message", text)
    except LookupError:
        return ctx.expect(False, "the message box is there")
    return ctx.keys(
        f"Enter sends {title!r}", "{Enter}", ctx.shows(options(title), "Button"), seconds=20
    )


# -- scripts ------------------------------------------------------------------------


def navigation(ctx: Ctx) -> None:
    """Sidebar collapse and open, Settings, then Back and Forward."""
    if not new_chat(ctx):
        return
    ctx.act("Close sidebar", "Close sidebar", "Button", ctx.shows("Open sidebar", "Button"))
    ctx.shot("sidebar-closed")
    ctx.act("Open sidebar", "Open sidebar", "Button", ctx.shows("Close sidebar", "Button"))
    ctx.act("Settings opens", "Settings", "Button", ctx.shows(PAGES[0][1]))
    ctx.act("Back returns to the chat", "Back", "Button", ctx.shows("Message", "Edit"))
    ctx.act("Forward goes to Settings again", "Forward", "Button", ctx.shows(PAGES[0][1]))
    ctx.shot("forward")
    new_chat(ctx)


def settings_pages(ctx: Ctx) -> None:
    """Every page opens on one click; every toggle saves, stays set and goes back."""
    if not ctx.act("Settings opens", "Settings", "Button", ctx.shows(PAGES[0][1])):
        return
    for label, words in PAGES:
        ctx.act(f"Settings > {label} opens", label, "Button", ctx.shows(words))
        ctx.shot(f"settings-{label.lower().replace(' ', '-')}")
    for page, toggle, path in TOGGLES:
        words = dict(PAGES)[page]
        if not ctx.act(f"Settings > {page} opens", page, "Button", ctx.shows(words)):
            continue
        before = bool(ctx.setting(path))
        if not ctx.act(f"{toggle!r} switches", toggle, None, ctx.saved(path, not before)):
            continue
        other = "Privacy" if page != "Privacy" else "General"
        ctx.act(f"Settings > {other} opens", other, "Button", ctx.shows(dict(PAGES)[other]))
        ctx.act(f"Settings > {page} opens", page, "Button", ctx.shows(words))
        ctx.expect(ctx.ui.toggled(toggle) is (not before), f"{toggle!r} is still switched")
        ctx.act(f"{toggle!r} switches back", toggle, None, ctx.saved(path, before))
    new_chat(ctx)


def composer(ctx: Ctx) -> None:
    """Empty send, Shift+Enter, Enter, a long message, and the model and effort picker."""
    clean_up(ctx)
    if not new_chat(ctx):
        return
    chats_before = len(ctx.ui.find("Options for", "Button"))
    try:
        ctx.ui.fill("Message", "")
    except LookupError:
        ctx.expect(False, "the message box is there")
        return
    ctx.ui.key("{Enter}")
    ctx.sleep(1.5)
    ctx.expect(
        len(ctx.ui.find("Options for", "Button")) == chats_before,
        "an empty message sends nothing",
    )
    ctx.ui.fill("Message", "line one")
    ctx.ui.key("{Shift}{Enter}")
    ctx.ui.key("line two")
    text = (ctx.ui.value("Message", "Edit") or "").replace("\r\n", "\n").replace("\r", "\n")
    ctx.expect("line one\nline two" in text, "Shift+Enter starts a new line")
    ctx.ui.fill("Message", "")
    picker(ctx)
    if send(ctx, QUESTION, QUESTION[:40]) and ctx.claude:
        ms = ctx.until(ctx.shows("42"), 120)
        ctx.expect(ms is not None, "Claude's reply (42) shows in the chat", ms)
    ctx.shot("reply")
    if new_chat(ctx) and send(ctx, LONG_MESSAGE, LONG_MESSAGE[:30]) and ctx.claude:
        ms = ctx.until(ctx.shows("OK"), 120)
        ctx.expect(ms is not None, "a long message gets its reply (OK)", ms)
    ctx.shot("long-message")
    delete_chat(ctx, QUESTION[:40])
    delete_chat(ctx, LONG_MESSAGE[:30])


def picker(ctx: Ctx) -> None:
    """The model and effort menu opens on one click, saves each pick and is put back."""
    model = ctx.setting(("claude", "model")) or ""
    effort = ctx.setting(("claude", "effort")) or ""
    picks = (("model", "Sonnet", "sonnet"), ("effort", "High", "high"))
    for key, label, value in picks:
        opened = ctx.act(
            "The model and effort menu opens", "Model and effort", "Button",
            ctx.shows(label, "MenuItem"),
        )  # fmt: skip
        if not opened:
            ctx.ui.key("{Esc}")
            return
        ctx.shot(f"picker-{key}")
        ctx.act(f"Picking {label} saves it", label, "MenuItem", ctx.saved(("claude", key), value))
    for key, value, labels in (("model", model, MODEL_LABELS), ("effort", effort, EFFORT_LABELS)):
        label = labels.get(value, value)
        ctx.act(
            "The model and effort menu opens", "Model and effort", "Button",
            ctx.shows("Low", "MenuItem"),
        )  # fmt: skip
        if label == "Default":  # the model's reads "Default Claude Code's choice"
            names = ctx.ui.find("Default", "MenuItem")
            longer = [n for n in names if n.strip().casefold() != "default"]
            plain = [n for n in names if n.strip().casefold() == "default"]
            choose = longer if key == "model" else plain
            label = choose[0] if choose else label
        ctx.act(
            f"{key} goes back to {value or 'default'}", label, "MenuItem",
            ctx.saved(("claude", key), value),
        )  # fmt: skip


def chats(ctx: Ctx) -> None:
    """Two chats: switch, pin, rename, search, then restart Relay; unpin and delete,
    then restart again. Every step is one click and is still true after a restart."""
    title = f"Relay test {ctx.stamp}"
    first, second = QUESTION[:40], SECOND[:30]
    clean_up(ctx)
    if not (new_chat(ctx) and send(ctx, QUESTION, first)):
        return
    if ctx.claude:
        ctx.wait("42", seconds=120)
    if not (new_chat(ctx) and send(ctx, SECOND, second)):
        return
    if ctx.claude:
        ctx.wait("blue", seconds=120)
        ctx.act("Switching to the first chat", first, "Button", ctx.shows("42"))
        ctx.act("Switching to the second chat", second, "Button", ctx.shows("blue"))
    ctx.act("Chat menu opens", options(first), "Button", ctx.shows("Pin", "MenuItem"))
    ctx.act("Pin", "Pin", "MenuItem", ctx.shows("Pinned"))
    ctx.act("Chat menu opens", options(first), "Button", ctx.shows("Rename", "MenuItem"))
    if ctx.act("Rename", "Rename", "MenuItem", ctx.shows("Chat name", "Edit")):
        ctx.sleep(0.5)  # the menu hands focus to the name box, as for a person
        try:
            ctx.ui.fill("Chat name", title)
        except LookupError:
            ctx.expect(False, "the name box stays open while typing")
        ctx.keys("Enter saves the new name", "{Enter}", ctx.shows(options(title), "Button"))
    ctx.shot("renamed")
    if ctx.act("Search opens", "Search", "Button", ctx.shows("Search chats", "Edit")):
        ctx.ui.fill("Search chats", ctx.stamp)
        ctx.expect(ctx.wait(options(title), "Button", seconds=3), "Search finds the chat")
        ctx.expect(
            ctx.wait(options(second), "Button", gone=True, seconds=3),
            "Search hides chats that don't match",
        )
        ctx.shot("search")
        ctx.act("Close search", "Close search", "Button", ctx.shows("Search", "Button"))
    if restart(ctx):
        ctx.expect(ctx.wait("Pinned", seconds=5), "the pin is kept after a restart")
        kept = ctx.wait(options(title), "Button", seconds=2)
        ctx.expect(kept, "the new name is kept after a restart")
        kept = ctx.wait(options(second), "Button", seconds=2)
        ctx.expect(kept, "chats are kept after a restart")
    ctx.act("Chat menu opens", options(title), "Button", ctx.shows("Unpin", "MenuItem"))
    ctx.act("Unpin", "Unpin", "MenuItem", ctx.gone("Pinned"))
    deleted = delete_chat(ctx, title)
    deleted = delete_chat(ctx, second) and deleted
    ctx.shot("deleted")
    if deleted and restart(ctx):
        ctx.expect(
            not ctx.ui.find(options(title), "Button") and not ctx.ui.find(options(second)),
            "deleted chats stay deleted after a restart",
        )
        ctx.expect(not ctx.ui.find("Pinned"), "the unpin is kept after a restart")


def restart(ctx: Ctx) -> bool:
    start = ctx.clock()
    why = ctx.ui.restart()
    if why:
        return ctx.expect(False, f"Relay restarts ({why})")
    ok = ctx.wait("New chat", "Button", seconds=60)
    return ctx.expect(ok, "Relay restarts", round((ctx.clock() - start) * 1000))


def attach(ctx: Ctx) -> None:
    if not new_chat(ctx):
        return
    opened = ctx.act(
        "Attach opens a file picker", "Attach files", "Button", lambda: bool(ctx.ui.dialog())
    )
    if opened:
        ctx.shot("attach")
        ctx.keys("Esc closes the file picker", "{Esc}", lambda: ctx.ui.dialog() is None)


def stop(ctx: Ctx) -> None:
    """A long Claude answer stops on one click, and the composer is ready again."""
    if not (new_chat(ctx) and send(ctx, LONG_ASK, LONG_ASK[:40])):
        return
    if ctx.expect(ctx.wait("Stop", "Button", seconds=15), "Stop shows while Relay works"):
        ctx.shot("working")
        ctx.act("Stop ends the answer", "Stop", "Button", ctx.shows("Send", "Button"), 15)
    ctx.shot("stopped")
    delete_chat(ctx, LONG_ASK[:40])


def small_window(ctx: Ctx) -> None:
    """At 900 x 640 the composer, Send and the sidebar buttons are still fully in view.
    Ends on Settings > Developer, where a full test from the app was started."""
    width, height, maximized = ctx.ui.size()
    try:
        ctx.ui.resize(900, 640)
        ctx.sleep(1.0)
        ctx.shot("small")
        for name, role in (("Message", "Edit"), ("Send", "Button"), ("Attach files", "Button"),
                           ("Model and effort", "Button")):  # fmt: skip
            ctx.expect(ctx.ui.inside(name, role), f"{name} is in view in a small window")
        side = "New chat" if ctx.ui.find("New chat", "Button") else "Open sidebar"
        ctx.expect(ctx.ui.inside(side, "Button"), f"{side} is in view in a small window")
    finally:
        ctx.ui.resize(width, height, maximized)
    if ctx.act("Settings opens", "Settings", "Button", ctx.shows(PAGES[0][1])):
        ctx.act("Settings > Developer opens", "Developer", "Button", ctx.shows(PAGES[-1][1]))


SCRIPTS: dict[str, Callable[[Ctx], None]] = {
    "app-navigation": navigation,
    "app-settings": settings_pages,
    "app-composer": composer,
    "app-chats": chats,
    "app-attach": attach,
    "app-stop": stop,
    "app-small-window": small_window,
}
# Scripts that need Claude signed in (a reply, or something to stop).
NEEDS_CLAUDE = {"app-stop"}
