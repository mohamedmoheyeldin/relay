from __future__ import annotations

from collections.abc import Sequence

import pytest

from relay_screen.actions import Computer
from relay_screen.apps import (
    App,
    AppCatalog,
    Window,
    find_window,
    launch,
    merge,
    new_window,
    preferred_browser,
    window_listing,
)
from relay_screen.errors import ActionError
from relay_screen.safety.approval import ActRequest
from relay_screen.scaling import Rect
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber

BRAVE = r"C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe"
EDGE = r"C:\Program Files (x86)\Microsoft\EdgeCore\141.0\msedge.exe"
NOTEPAD_ID = "Microsoft.WindowsNotepad_8wekyb3d8bbwe!App"


class FakeApps:
    def __init__(self) -> None:
        self.start = [
            App("Notepad", app_id=NOTEPAD_ID),
            App("Brave Origin", app_id="Brave.Origin", exe=BRAVE),
            App("Calculator", app_id="Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            App("Visual Studio Code", app_id="Microsoft.VisualStudioCode"),
        ]
        self.paths = {"brave.exe": BRAVE, "msedge.exe": EDGE, "code.exe": r"C:\VS Code\Code.exe"}
        self.registered = [
            App("Brave Origin", exe=BRAVE, browser=True),
            App("Microsoft Edge", exe=EDGE, browser=True, default_browser=True),
        ]
        self.scans = 0
        self.launched: list[tuple[str, list[str]]] = []
        self.open: list[Window] = [Window(1, "Inbox - Mail", Rect(0, 0, 800, 600))]
        self.fg = 1
        self.on_launch: Window | None = None
        self.front_ok = True

    def start_apps(self) -> list[App]:
        self.scans += 1
        return list(self.start)

    def app_paths(self) -> dict[str, str]:
        return dict(self.paths)

    def browsers(self) -> list[App]:
        return list(self.registered)

    def launch(self, app: App, args: Sequence[str]) -> None:
        self.launched.append((app.name, list(args)))
        if self.on_launch is not None:
            self.open.append(self.on_launch)

    def windows(self) -> list[Window]:
        return list(self.open)

    def foreground(self) -> int:
        return self.fg

    def bring_to_front(self, handle: int) -> bool:
        if self.front_ok:
            self.fg = handle
        return self.front_ok


def catalog(backend: FakeApps | None = None) -> AppCatalog:
    return AppCatalog(backend or FakeApps())


def test_merge_dedupes_and_marks_browsers():
    fake = FakeApps()
    apps = {a.name: a for a in merge(fake.start, fake.paths, fake.registered)}
    assert set(apps) == {
        "Brave Origin",
        "Calculator",
        "Code",
        "Microsoft Edge",
        "Notepad",
        "Visual Studio Code",
    }
    assert apps["Brave Origin"].browser and apps["Brave Origin"].app_id == "Brave.Origin"
    assert apps["Microsoft Edge"].default_browser
    assert apps["Code"].exe == r"C:\VS Code\Code.exe"


def test_browser_alias_prefers_a_start_menu_browser_over_a_hidden_default():
    apps = catalog().apps()
    assert preferred_browser(apps) == next(a for a in apps if a.name == "Brave Origin")
    assert catalog().resolve("browser").name == "Brave Origin"
    listed = [App("Edge", app_id="MSEdge", exe=EDGE, browser=True, default_browser=True)]
    assert preferred_browser([*apps, *listed]) == listed[0]


def test_resolve_exact_prefix_fuzzy_and_ambiguous():
    apps = catalog()
    assert apps.resolve("notepad").name == "Notepad"
    assert apps.resolve("calc").name == "Calculator"
    assert apps.resolve("Calculater").name == "Calculator"
    assert apps.resolve("msedge").name == "Microsoft Edge"
    with pytest.raises(ActionError, match="No installed app"):
        apps.resolve("Photoshop")
    assert apps.resolve("code").name == "Code"  # exact beats the word in "Visual Studio Code"


def test_resolve_prefers_exact_name():
    fake = FakeApps()
    fake.paths = {}
    fake.start.append(App("Code Writer", app_id="cw"))
    assert catalog(fake).resolve("Visual").name == "Visual Studio Code"
    with pytest.raises(ActionError, match="ambiguous"):
        catalog(fake).resolve("code")


def test_scan_is_cached_and_refreshed():
    clock = [0.0]
    fake = FakeApps()
    apps = AppCatalog(fake, clock=lambda: clock[0], ttl=10)
    apps.apps()
    apps.apps()
    assert fake.scans == 1
    clock[0] = 11
    apps.apps()
    assert fake.scans == 2
    apps.listing(refresh=True)
    assert fake.scans == 3
    fake.start.append(App("Paint", app_id="paint"))
    assert apps.resolve("Paint").name == "Paint"  # a miss rescans once
    assert fake.scans == 4


def test_listing():
    text = catalog().listing()
    assert text.startswith("6 apps installed")
    assert "Brave Origin · browser" in text and "Microsoft Edge · default browser" in text
    assert catalog().listing("note").splitlines() == ["1 app(s) match 'note':", "Notepad"]
    assert "No installed app" in catalog().listing("zzz")


def test_new_window_rules():
    notepad = App("Notepad", app_id=NOTEPAD_ID)
    brave = App("Brave Origin", exe=BRAVE)
    old = Window(1, "Inbox", Rect(0, 0, 10, 10), exe=BRAVE)
    new = Window(2, "Untitled - Notepad", Rect(0, 0, 10, 10))
    assert new_window({1: "Inbox"}, [old, new], 1, 1, notepad) == new
    # A browser opening a tab in its existing window: the title changes.
    retitled = Window(1, "Example Domain - Brave", Rect(0, 0, 10, 10), exe=BRAVE)
    assert new_window({1: "Inbox"}, [retitled], 1, 1, brave) == retitled
    assert new_window({1: "Inbox"}, [retitled], 1, 1, notepad) is None
    other = Window(3, "Other", Rect(0, 0, 10, 10), exe=r"C:\x\other.exe")
    assert new_window({1: "Inbox"}, [old, other], 1, 1, brave) is None


def test_launch_waits_and_brings_to_front():
    fake = FakeApps()
    fake.on_launch = Window(7, "Untitled - Notepad", Rect(100, 100, 800, 600))
    slept: list[float] = []
    window, in_front = launch(fake, App("Notepad", app_id=NOTEPAD_ID), [], slept.append)
    assert window == fake.on_launch and in_front and fake.fg == 7
    assert len(slept) == 2
    fake = FakeApps()
    window, in_front = launch(fake, App("Notepad"), [], slept.append, patience=1)
    assert window is None and not in_front


def computer(owner_displays, fake: FakeApps, hooks=()) -> Computer:
    state = ScreenState(lambda: owner_displays, lambda ds: None)
    return Computer(
        state,
        GradientGrabber(),
        None,
        report_changes=False,
        before_act=list(hooks),
        sleep=lambda s: None,
        apps=AppCatalog(fake),
    )


def test_launch_app_through_computer(owner_displays):
    fake = FakeApps()
    fake.on_launch = Window(9, "Example Domain - Brave", Rect(200, 1700, 1200, 800), exe=BRAVE)
    seen: list[ActRequest] = []
    c = computer(owner_displays, fake, [lambda r: seen.append(r)])
    text = c.run("launch_app", {"name": "browser", "args": ["https://example.com"]}).text or ""
    assert fake.launched == [("Brave Origin", ["https://example.com"])]
    assert seen[0].summary == 'Open Brave Origin with "https://example.com"'
    assert text.startswith('Opened Brave Origin · window "Example Domain - Brave" in front')
    assert text.endswith("on display 3, now the active display")  # below the primary
    assert c.state.active().id == 3  # screenshots follow the window (D-093)
    with pytest.raises(ActionError, match="list of strings"):
        c.run("launch_app", {"name": "Notepad", "args": "x"})
    with pytest.raises(ActionError, match="No installed app"):
        c.run("launch_app", {"name": "Photoshop"})
    assert len(fake.launched) == 1


def test_denied_launch_never_starts(owner_displays):
    fake = FakeApps()

    def deny(request: ActRequest) -> str:
        raise ActionError("denied")

    with pytest.raises(ActionError, match="denied"):
        computer(owner_displays, fake, [deny]).run("launch_app", {"name": "Notepad"})
    assert fake.launched == []


def test_list_apps_through_computer(owner_displays):
    c = computer(owner_displays, FakeApps())
    assert "Notepad" in (c.run("list_apps", {"query": "note"}).text or "")
    bare = Computer(ScreenState(lambda: owner_displays, lambda ds: None), GradientGrabber(), None)
    with pytest.raises(ActionError, match="not available"):
        bare.run("list_apps", {})


# --- open windows (D-078) ---------------------------------------------------

RELAY = r"C:\Program Files\Relay\relay.exe"


def desktop() -> list[Window]:
    return [
        Window(5, "Relay", Rect(0, 0, 1200, 900), exe=RELAY),
        Window(6, "Monster Jobs - Brave", Rect(-32000, -32000, 160, 28), exe=BRAVE),
        Window(7, "notes.txt - Notepad", Rect(100, 100, 800, 600), exe=r"C:\W\notepad.exe"),
        Window(8, "Dice - Brave", Rect(0, 0, 1600, 900), exe=BRAVE),
    ]


def test_window_listing_marks_relay_front_and_minimized():
    text = window_listing(desktop(), 5)
    assert text.splitlines() == [
        "4 open window(s), topmost first:",
        '"Relay" · relay.exe · Relay itself, never the target · in front',
        '"Monster Jobs - Brave" · brave.exe · minimized',
        '"notes.txt - Notepad" · notepad.exe',
        '"Dice - Brave" · brave.exe',
    ]
    assert (
        window_listing(desktop(), 5, "brave").splitlines()[0] == "2 open window(s), topmost first:"
    )
    assert "No open window matches" in window_listing(desktop(), 5, "excel")


def test_find_window_by_title_program_or_app_name_never_relay():
    windows = desktop()
    assert find_window(windows, "Dice").handle == 8
    assert find_window(windows, "brave").handle == 6  # topmost Brave window
    assert find_window(windows, "Brave Origin", catalog()).handle == 6  # app name → exe
    assert find_window(windows, "notepad").handle == 7
    with pytest.raises(ActionError, match="No open window"):
        find_window(windows, "Relay")
    with pytest.raises(ActionError, match="launch_app"):
        find_window(windows, "Calculator", catalog())


def test_find_window_takes_the_looser_names_a_model_gives():
    """The 0.1.10 Dice run: focus_window failed twice on names like these (D-093)."""
    windows = desktop()
    assert find_window(windows, "Brave Origin").handle == 6  # no catalog: program word
    assert find_window(windows, "Brave browser").handle == 6
    assert find_window(windows, "https://www.dice.com/").handle == 8
    assert find_window(windows, "dice.com").handle == 8
    with pytest.raises(ActionError, match='Open windows: "Monster Jobs - Brave"'):
        find_window(windows, "Excel")


def test_focus_window_through_computer(owner_displays):
    fake = FakeApps()
    fake.open = desktop()
    fake.fg = 5
    seen: list[ActRequest] = []
    c = computer(owner_displays, fake, [lambda r: seen.append(r)])
    text = c.run("focus_window", {"window": "Monster"}).text or ""
    assert fake.fg == 6 and text.startswith('"Monster Jobs - Brave" is in front')
    assert seen[0].summary == 'Bring "Monster Jobs - Brave" to the front'
    listing = c.run("list_windows", {"query": "brave"}).text or ""
    assert '"Monster Jobs - Brave" · brave.exe · in front' in listing
    fake.front_ok = False
    text = c.run("focus_window", {"window": "Dice"}).text or ""
    assert "kept it behind" in text
    with pytest.raises(ActionError, match="No open window"):
        c.run("focus_window", {"window": "Photoshop"})


def test_open_url_opens_the_page_in_the_owners_browser(owner_displays):
    fake = FakeApps()
    fake.on_launch = Window(9, "Dice - Brave", Rect(0, 0, 1600, 900), exe=BRAVE)
    seen: list[ActRequest] = []
    c = computer(owner_displays, fake, [lambda r: seen.append(r)])
    text = c.run("open_url", {"url": "dice.com"}).text or ""
    assert fake.launched == [("Brave Origin", ["https://dice.com"])]
    assert seen[0].summary == "Open https://dice.com in Brave Origin"
    assert text.startswith(
        'Opened https://dice.com in Brave Origin · window "Dice - Brave" in front'
    )
    for bad in ("file:///C:/x", "javascript:alert(1)", "dice .com", ""):
        with pytest.raises(ActionError, match="url must be"):
            c.run("open_url", {"url": bad})
    assert len(fake.launched) == 1
