from __future__ import annotations

import json
from pathlib import Path

from relay_screen.diagnostics import app
from relay_screen.diagnostics import run as diag

from .test_diagnostics import FakeClaude, FakeLink, tools


class FakeRelay:
    """Relay's window as a list of (role, name): enough of the app to run the scripts."""

    def __init__(self, settings: Path, *, picker_works: bool = True) -> None:
        self.settings = settings
        self.picker_works = picker_works
        self.page = "chat"
        self.menu: str | None = None
        self.chats: list[dict[str, object]] = []
        self.searching: str | None = None
        self.renaming = False
        self.dialog_open = False
        self.busy = False
        self.focused = ""
        self.typed: dict[str, str] = {}

    def _save(self, key: str, value: str) -> None:
        data = json.loads(self.settings.read_text())
        data["claude"][key] = value
        self.settings.write_text(json.dumps(data))

    def elements(self) -> list[tuple[str, str]]:
        found = [("Button", "New chat"), ("Button", "Settings")]
        if self.searching is None:
            found.append(("Button", "Search"))
        else:
            found += [("Edit", "Search chats"), ("Button", "Close search")]
        shown = [
            c
            for c in self.chats
            if self.searching in (None, "") or self.searching in str(c["title"])
        ]
        if any(c["pinned"] for c in self.chats):
            found.append(("Text", "Pinned"))
        found += [("Button", f"Options for {c['title']}") for c in shown]
        if self.renaming:
            found.append(("Edit", "Chat name"))
        if self.page == "settings":
            found += [("Button", label) for label, _ in app.PAGES]
            found.append(("Text", self.settings_words))
        else:
            found += [
                ("Edit", "Message"),
                ("Button", "Model and effort"),
                ("Button", "Attach files"),
            ]
            found.append(("Button", "Stop") if self.busy else ("Button", "Send"))
            if any(c.get("reply") for c in self.chats):
                found.append(("Text", "42"))
        if self.menu == "picker":
            found += [("MenuItem", "DefaultClaude Code's choice"), ("MenuItem", "OpusMost capable")]
            found += [("MenuItem", "SonnetFast and smart"), ("MenuItem", "Default")]
            found += [("MenuItem", "Low"), ("MenuItem", "High")]
        elif self.menu:
            chat = next(c for c in self.chats if c["title"] == self.menu)
            found += [("MenuItem", "Unpin" if chat["pinned"] else "Pin")]
            found += [("MenuItem", "Rename"), ("MenuItem", "Delete")]
        return found

    settings_words = app.PAGES[0][1]

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
        if hit == "New chat":
            self.page = "chat"
        elif hit == "Settings":
            self.page = "settings"
        elif self.page == "settings" and any(hit == label for label, _ in app.PAGES):
            self.settings_words = dict(app.PAGES)[hit]
        elif hit == "Model and effort":
            self.menu = "picker" if self.picker_works else None
        elif menu == "picker":
            values = {
                "DefaultClaude Code's choice": ("model", ""),
                "SonnetFast and smart": ("model", "sonnet"),
            }
            values |= {"OpusMost capable": ("model", "opus"), "Default": ("effort", "")}
            values |= {"Low": ("effort", "low"), "High": ("effort", "high")}
            self._save(*values[hit])
        elif hit.startswith("Options for "):
            self.menu = hit.removeprefix("Options for ")
        elif menu and hit in ("Pin", "Unpin", "Rename", "Delete"):
            chat = next(c for c in self.chats if c["title"] == menu)
            if hit == "Delete":
                self.chats.remove(chat)
            elif hit == "Rename":
                self.renaming, self.editing = True, chat
            else:
                chat["pinned"] = hit == "Pin"
        elif hit == "Send":
            text = self.typed.get("Message", "")
            self.chats.append({"title": text[:60], "pinned": False, "reply": text == app.QUESTION})
            self.busy = text == app.LONG_ASK
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
        self.focused = name
        self.typed[name] = text
        if name == "Search chats":
            self.searching = text

    def key(self, keys: str) -> None:
        if keys == "{Esc}":
            self.dialog_open = False
            self.menu = None
        if keys == "{Enter}" and self.renaming:
            self.editing["title"] = self.typed["Chat name"]
            self.renaming = False

    def dialog(self) -> str | None:
        return "Open" if self.dialog_open else None


def app_tools(tmp_path: Path, **kw: object) -> tuple[diag.Tools, FakeRelay, Path]:
    settings = tmp_path / "settings.json"
    settings.write_text(json.dumps({"claude": {"model": "opus", "effort": ""}}))
    relay = FakeRelay(settings, **kw)  # type: ignore[arg-type]
    t = tools(FakeLink(), claude=FakeClaude())
    t.app, t.settings_file, t.stamp = relay, settings, "123456"
    return t, relay, settings


def run(t: diag.Tools, ids: list[str], tmp_path: Path) -> dict[str, diag.Result]:
    chosen = [s for s in diag.scenarios("123456", "http://127.0.0.1") if s.id in ids]
    results = diag.run_all(chosen, t, tmp_path / "run", lambda e: None)
    return {r.id: r for r in results}


def test_app_scripts_pass_on_a_working_app(tmp_path: Path) -> None:
    t, relay, settings = app_tools(tmp_path)
    ids = ["app-settings", "app-model-picker", "app-chat", "app-attach", "app-stop"]
    results = run(t, ids, tmp_path)
    for r in results.values():
        assert r.status == "passed", (r.id, r.failed_checks)
        assert r.details["checks"]
    # The picker was put back, the test chats are gone and Relay is on Settings.
    assert json.loads(settings.read_text())["claude"] == {"model": "opus", "effort": ""}
    assert relay.chats == []
    assert relay.page == "settings" and relay.settings_words == dict(app.PAGES)["Developer"]


def test_a_picker_that_does_not_open_fails(tmp_path: Path) -> None:
    t, _, _ = app_tools(tmp_path, picker_works=False)
    result = run(t, ["app-model-picker"], tmp_path)["app-model-picker"]
    assert result.status == "failed"
    assert "the model and effort menu opens" in result.failed_checks


def test_app_tests_skip_without_the_window(tmp_path: Path) -> None:
    t, _, _ = app_tools(tmp_path)
    t.app = None
    assert run(t, ["app-chat"], tmp_path)["app-chat"].status == "skipped"
