from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from relay_screen.diagnostics import app
from relay_screen.diagnostics import run as diag

from .test_diagnostics import FakeCloud, FakeLink, tools

PICKS = {
    "DefaultCloud AI's choice": ("model", ""),
    "OpusMost capable": ("model", "opus"),
    "SonnetFast and smart": ("model", "sonnet"),
    "Default": ("effort", ""),
    "Low": ("effort", "low"),
    "High": ("effort", "high"),
}
REPLIES = {app.QUESTION: "42", app.SECOND: "blue", app.LONG_MESSAGE: "OK"}


class FakeRelay:
    """Enough of Relay's window to run the scripts: a list of (role, name) per state.
    `double_delete` is the owner's bug: Delete needs a second click."""

    def __init__(self, settings: Path, *, picker_works=True, double_delete=False) -> None:
        self.settings = settings
        self.picker_works = picker_works
        self.double_delete = double_delete
        self.deletes = 0
        self.saved: list[dict[str, Any]] = []  # chats on disk: kept across restarts
        self.restarts = 0
        self._fresh()

    def _fresh(self) -> None:
        self.history: list[tuple[str, dict[str, Any] | None]] = [("chat", None)]
        self.at = 0
        self.page = "General"
        self.sidebar = True
        self.menu: str | None = None
        self.searching: str | None = None
        self.renaming: dict[str, Any] | None = None
        self.dialog_open = False
        self.busy = False
        self.message = ""
        self.focus = ""
        self.window = (1600, 1000, True)

    # -- state ----------------------------------------------------------------------

    @property
    def view(self) -> str:
        return self.history[self.at][0]

    @property
    def chat(self) -> dict[str, Any] | None:
        return self.history[self.at][1]

    def go(self, view: str, chat: dict[str, Any] | None = None) -> None:
        self.history = [*self.history[: self.at + 1], (view, chat)]
        self.at += 1

    def data(self) -> dict[str, Any]:
        return json.loads(self.settings.read_text())

    def put(self, section: str, key: str, value: Any) -> None:
        data = self.data()
        data.setdefault(section, {})[key] = value
        self.settings.write_text(json.dumps(data))

    def elements(self) -> list[tuple[str, str]]:
        found: list[tuple[str, str]] = []
        if not self.sidebar:
            found += [("Button", "Open sidebar"), ("Button", "New chat"), ("Button", "Settings")]
        else:
            found += [("Button", "Close sidebar"), ("Button", "Back"), ("Button", "Forward")]
            found += [("Button", "New chat"), ("Button", "Settings")]
            if self.searching is None:
                found.append(("Button", "Search"))
            else:
                found += [("Edit", "Search chats"), ("Button", "Close search")]
            shown = [c for c in self.saved if not self.searching or self.searching in c["title"]]
            if any(c["pinned"] for c in self.saved):
                found.append(("Text", "Pinned"))
            for c in shown:
                found += [("Button", c["title"]), ("Button", f"Options for {c['title']}")]
            if self.renaming:
                found.append(("Edit", "Chat name"))
        if self.view == "settings":
            found += [("Button", label) for label, _ in app.PAGES]
            found.append(("Text", dict(app.PAGES)[self.page]))
            found += [("CheckBox", toggle) for page, toggle, _ in app.TOGGLES if page == self.page]
        else:
            found += [("Edit", "Message"), ("Button", "Model and effort")]
            found += [("Button", "Attach files")]
            found.append(("Button", "Stop") if self.busy else ("Button", "Send"))
            if self.chat and self.chat.get("reply"):
                found.append(("Text", self.chat["reply"]))
        if self.menu == "picker":
            found += [("MenuItem", name) for name in PICKS]
        elif self.menu:
            chat = next(c for c in self.saved if c["title"] == self.menu)
            found += [("MenuItem", "Unpin" if chat["pinned"] else "Pin")]
            found += [("MenuItem", "Rename"), ("MenuItem", "Delete")]
        return found

    # -- AppUi ------------------------------------------------------------------------

    def ready(self) -> str | None:
        return None

    def find(self, name: str, role: str | None = None) -> list[str]:
        q = name.casefold()
        hits = [
            n
            for r, n in self.elements()
            if (role is None or r == role) and n.casefold().startswith(q)
        ]
        return sorted(hits, key=lambda n: n.casefold() != q)

    def press(self, name: str, role: str | None = None) -> str:
        hits = self.find(name, role)
        if not hits:
            raise LookupError(name)
        hit = hits[0]
        menu, self.menu = self.menu, None
        toggles = {toggle: path for _, toggle, path in app.TOGGLES}
        titles = {c["title"]: c for c in self.saved}
        if hit == "New chat":
            self.go("chat")
        elif hit in ("Close sidebar", "Open sidebar"):
            self.sidebar = hit == "Open sidebar"
        elif hit == "Settings":
            self.go("settings")
        elif hit == "Back" and self.at > 0:
            self.at -= 1
        elif hit == "Forward" and self.at < len(self.history) - 1:
            self.at += 1
        elif self.view == "settings" and hit in dict(app.PAGES):
            self.page = hit
        elif hit in toggles:
            section, key = toggles[hit]
            self.put(section, key, not self.data().get(section, {}).get(key, False))
        elif hit == "Model and effort":
            self.menu = "picker" if self.picker_works else None
        elif menu == "picker":
            self.put("cloud", *PICKS[hit])
        elif hit.startswith("Options for "):
            self.menu = hit.removeprefix("Options for ")
        elif menu and hit in ("Pin", "Unpin", "Rename", "Delete"):
            chat = titles[menu]
            if hit == "Delete":
                self.deletes += 1
                if not self.double_delete or self.deletes % 2 == 0:
                    self.saved.remove(chat)
            elif hit == "Rename":
                self.renaming = chat
            else:
                chat["pinned"] = hit == "Pin"
        elif hit in titles:
            self.go("chat", titles[hit])
        elif hit == "Stop":
            self.busy = False
        elif hit == "Search":
            self.searching = ""
        elif hit == "Close search":
            self.searching = None
        elif hit == "Attach files":
            self.dialog_open = True
        return hit

    def fill(self, name: str, text: str) -> None:
        if not self.find(name, "Edit"):
            raise LookupError(name)
        self.focus = name
        if name == "Message":
            self.message = text
        elif name == "Search chats":
            self.searching = text
        elif name == "Chat name" and self.renaming:
            self.renaming["new"] = text

    def key(self, keys: str) -> None:
        if keys == "{Esc}":
            self.dialog_open = False
            self.menu = None
        elif keys == "{Enter}" and self.renaming:
            self.renaming["title"] = self.renaming.pop("new", self.renaming["title"])
            self.renaming = None
        elif keys == "{Enter}" and self.focus == "Message" and self.message.strip():
            reply = REPLIES.get(self.message)
            chat = {"title": self.message[:60], "pinned": False, "reply": reply}
            self.saved.append(chat)
            self.history[self.at] = ("chat", chat)
            self.busy = self.message == app.LONG_ASK
            self.message = ""
        elif keys == "{Shift}{Enter}":
            self.message += "\n"
        elif not keys.startswith("{"):
            self.message += keys

    def value(self, name: str, role: str | None = None) -> str | None:
        return self.message if name == "Message" else None

    def toggled(self, name: str) -> bool | None:
        path = next((p for _, toggle, p in app.TOGGLES if toggle == name), None)
        return bool(self.data().get(path[0], {}).get(path[1])) if path else None

    def dialog(self) -> str | None:
        return "Open" if self.dialog_open else None

    def restart(self) -> str | None:
        self.restarts += 1
        self._fresh()
        return None

    def size(self) -> tuple[int, int, bool]:
        return self.window

    def resize(self, width: int, height: int, maximized: bool = False) -> None:
        self.window = (width, height, maximized)

    def inside(self, name: str, role: str | None = None) -> bool:
        return bool(self.find(name, role))


START = {
    "cloud": {"model": "opus", "effort": ""},
    "general": {"start_at_login": True},
    "safety": {"phone_approvals": False},
    "updates": {"check_on_start": True},
}


def app_tools(tmp_path: Path, **kw: Any) -> tuple[diag.Tools, FakeRelay, Path]:
    settings = tmp_path / "settings.json"
    settings.write_text(json.dumps(START))
    relay = FakeRelay(settings, **kw)
    t = tools(FakeLink(), cloud=FakeCloud())
    t.app, t.settings_file, t.stamp = relay, settings, "123456"
    return t, relay, settings


def run(t: diag.Tools, ids: list[str], tmp_path: Path) -> dict[str, diag.Result]:
    chosen = [s for s in diag.scenarios("123456", "http://127.0.0.1") if s.id in ids]
    results = diag.run_all(chosen, t, tmp_path / "run", lambda e: None)
    return {r.id: r for r in results}


APP = [s.id for s in diag.scenarios("1", "http://x") if s.kind == "app"]


def test_app_scripts_pass_on_a_working_app(tmp_path: Path) -> None:
    t, relay, settings = app_tools(tmp_path)
    results = run(t, APP, tmp_path)
    assert list(results) == APP and len(APP) == len(app.SCRIPTS)
    for r in results.values():
        assert r.area == "Relay app"
        assert r.status == "passed", (r.id, r.failed_checks)
    checks = [c for r in results.values() for c in r.details["checks"]]
    assert len(checks) > 60
    assert all("ms" in c for c in checks if "one click" in c["what"])
    # Settings are as they were, the test chats are gone, Relay restarted twice and
    # ended on Settings > Developer.
    assert json.loads(settings.read_text()) == START
    assert relay.saved == [] and relay.restarts == 2
    assert relay.view == "settings" and relay.page == "Developer"
    assert relay.window == (1600, 1000, True)


def test_a_delete_that_needs_two_clicks_fails(tmp_path: Path) -> None:
    t, _, _ = app_tools(tmp_path, double_delete=True)
    result = run(t, ["app-chats"], tmp_path)["app-chats"]
    assert result.status == "failed"
    assert any(c.startswith("Delete removes") for c in result.failed_checks)


def test_a_picker_that_does_not_open_fails(tmp_path: Path) -> None:
    t, _, _ = app_tools(tmp_path, picker_works=False)
    result = run(t, ["app-composer"], tmp_path)["app-composer"]
    assert result.status == "failed"
    assert "The model and effort menu opens on one click" in result.failed_checks


def test_app_tests_skip_without_the_window(tmp_path: Path) -> None:
    t, _, _ = app_tools(tmp_path)
    t.app = None
    assert run(t, ["app-chats"], tmp_path)["app-chats"].status == "skipped"
