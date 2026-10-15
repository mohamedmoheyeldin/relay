"""Installed-app discovery and launching, found at run time (D-030).

Nothing in Relay hard-codes where a program lives. The Windows backend
(apps_windows.py) reports Start menu apps (Store apps included), registered App Paths
and the registered web browsers with the default one marked; this module merges them
into one catalog Claude can search with ``list_apps`` and open with ``launch_app``.
"""

from __future__ import annotations

import json
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass, replace
from difflib import SequenceMatcher
from pathlib import PureWindowsPath
from typing import Protocol

from relay_screen.errors import ActionError
from relay_screen.scaling import Rect

BROWSER_ALIASES = frozenset({"browser", "default browser", "web browser"})
CACHE_SECONDS = 300.0  # rescan installed apps at most this often unless asked
LIST_LIMIT = 80  # names in one unfiltered listing
MATCH_LIMIT = 20
FUZZY_MIN = 0.75
WINDOW_PATIENCE = 10.0  # s to wait for the launched app's window
WINDOW_POLL = 0.25
WINDOW_SETTLE = 0.4  # s after a window appears, so its title is filled in
MAX_ARGS = 20


@dataclass(frozen=True)
class App:
    name: str
    app_id: str | None = None  # Start menu AppUserModelID / parsing name
    exe: str | None = None  # full path, when known
    browser: bool = False
    default_browser: bool = False

    @property
    def stem(self) -> str | None:
        return PureWindowsPath(self.exe).stem.casefold() if self.exe else None


@dataclass(frozen=True)
class Window:
    handle: int
    title: str
    rect: Rect  # physical px
    exe: str | None = None


class AppBackend(Protocol):
    def start_apps(self) -> list[App]: ...
    def app_paths(self) -> dict[str, str]: ...  # "chrome.exe" -> full path
    def browsers(self) -> list[App]: ...  # registered browsers, default one marked
    def launch(self, app: App, args: Sequence[str]) -> None: ...
    def windows(self) -> list[Window]: ...  # visible top-level windows with titles
    def foreground(self) -> int: ...
    def bring_to_front(self, handle: int) -> bool: ...


def _same_exe(a: str | None, b: str | None) -> bool:
    return a is not None and b is not None and a.casefold() == b.casefold()


def merge(start: Sequence[App], app_paths: dict[str, str], browsers: Sequence[App]) -> list[App]:
    """One entry per program: Start menu names first, then browsers, then App Paths."""
    apps = list(start)
    for browser in browsers:
        index = next(
            (
                i
                for i, a in enumerate(apps)
                if _same_exe(a.exe, browser.exe) or a.name.casefold() == browser.name.casefold()
            ),
            None,
        )
        if index is None:
            apps.append(browser)
            continue
        found = apps[index]
        apps[index] = replace(
            found,
            exe=found.exe or browser.exe,
            browser=True,
            default_browser=found.default_browser or browser.default_browser,
        )
    stems = {a.stem for a in apps if a.stem}
    for _, path in sorted(app_paths.items()):
        name = PureWindowsPath(path).stem
        if any(_same_exe(a.exe, path) for a in apps) or name.casefold() in stems:
            continue
        apps.append(App(name, exe=path))
        stems.add(name.casefold())
    return sorted(apps, key=lambda a: (a.name.casefold(), a.app_id or "", a.exe or ""))


def score(app: App, query: str) -> float:
    q = query.casefold().strip()
    name = app.name.casefold()
    if not q:
        return 0.0
    if name == q:
        return 1.0
    if app.stem == q:
        return 0.95
    if name.startswith(q):
        return 0.9
    if any(word.startswith(q) for word in name.split()):
        return 0.85
    if q in name:
        return 0.8
    ratio = SequenceMatcher(None, q, name).ratio()
    return ratio * 0.8 if ratio >= FUZZY_MIN else 0.0


def preferred_browser(apps: Sequence[App]) -> App | None:
    """The default browser, unless it has no Start menu entry and another browser does.

    Windows can keep a browser the owner never uses as the https handler (Edge as a
    bare system component, D-028); the one they installed and pinned is the better bet.
    """
    browsers = [a for a in apps if a.browser or a.default_browser]
    defaults = [a for a in browsers if a.default_browser]
    listed = [a for a in browsers if a.app_id is not None]
    pick = next((a for a in defaults if a.app_id is not None), None)
    if pick is None and len(listed) == 1:
        pick = listed[0]
    if pick is None and defaults:
        pick = defaults[0]
    return pick or (browsers[0] if len(browsers) == 1 else None)


@dataclass
class AppCatalog:
    backend: AppBackend
    clock: Callable[[], float] = time.monotonic
    ttl: float = CACHE_SECONDS
    _apps: list[App] | None = None
    _scanned: float = 0.0

    def apps(self, refresh: bool = False) -> list[App]:
        if refresh or self._apps is None or self.clock() - self._scanned > self.ttl:
            self._apps = merge(
                self.backend.start_apps(), self.backend.app_paths(), self.backend.browsers()
            )
            self._scanned = self.clock()
        return self._apps

    def search(self, query: str, refresh: bool = False) -> list[tuple[float, App]]:
        apps = self.apps(refresh)
        if query.casefold().strip() in BROWSER_ALIASES:
            browser = preferred_browser(apps)
            return [(1.0, browser)] if browser else []
        scored = [(s, a) for a in apps if (s := score(a, query)) > 0]
        return sorted(scored, key=lambda pair: (-pair[0], pair[1].name.casefold()))

    def resolve(self, name: str) -> App:
        """The one app ``name`` means, or an error naming the closest ones."""
        found = self.search(name)
        if not found:
            found = self.search(name, refresh=True)  # installed since the last scan?
        if not found:
            raise ActionError(f"No installed app matches {name!r}. Use list_apps to see them.")
        best_score, best = found[0]
        if best_score >= 0.95:  # exact name or program name; prefer an entry with an exe
            exact = [a for s, a in found if s == best_score]
            return next((a for a in exact if a.exe), best)
        strong = [a for s, a in found if s >= 0.85]
        if len(strong) == 1 or len(found) == 1:
            return best
        names = ", ".join(_label(a) for _, a in found[:5])
        raise ActionError(f"{name!r} is ambiguous. Closest: {names}. Pass the exact name.")

    def listing(self, query: str | None = None, refresh: bool = False) -> str:
        if query:
            found = [a for _, a in self.search(query, refresh)][:MATCH_LIMIT]
            if not found:
                return f"No installed app matches {query!r}."
            head = f"{len(found)} app(s) match {query!r}:"
            return "\n".join([head, *(_label(a) for a in found)])
        apps = self.apps(refresh)
        head = (
            f"{len(apps)} apps installed (Start menu, App Paths, browsers). "
            'Open one with launch_app(name); "browser" opens the web browser.'
        )
        lines = [_label(a) for a in apps[:LIST_LIMIT]]
        if len(apps) > LIST_LIMIT:
            lines.append(f"... {len(apps) - LIST_LIMIT} more; pass query to filter.")
        return "\n".join([head, *lines])


def _label(app: App) -> str:
    if app.default_browser:
        return f"{app.name} · default browser"
    if app.browser:
        return f"{app.name} · browser"
    return app.name


def new_window(
    before: dict[int, str], after: Sequence[Window], foreground: int, previous_fg: int, app: App
) -> Window | None:
    """The window a launch produced: new and in front, new, newly in front, or retitled."""
    mine = [w for w in after if app.exe is None or w.exe is None or _same_exe(w.exe, app.exe)]
    fresh = [w for w in mine if w.handle not in before]
    front = next((w for w in fresh if w.handle == foreground), None)
    if front or fresh:
        return front or fresh[0]
    if foreground != previous_fg:
        moved = next((w for w in mine if w.handle == foreground), None)
        if moved:
            return moved
    if app.exe is None:
        return None  # without an exe, a retitled window could be anything
    return next((w for w in mine if before.get(w.handle, w.title) != w.title), None)


def launch(
    backend: AppBackend,
    app: App,
    args: Sequence[str],
    sleep: Callable[[float], None],
    patience: float = WINDOW_PATIENCE,
) -> tuple[Window | None, bool]:
    """Start ``app`` and wait for its window; returns it and whether it is in front."""
    before = {w.handle: w.title for w in backend.windows()}
    previous_fg = backend.foreground()
    backend.launch(app, args)
    waited = 0.0
    window = None
    while waited < patience:
        sleep(WINDOW_POLL)
        waited += WINDOW_POLL
        window = new_window(before, backend.windows(), backend.foreground(), previous_fg, app)
        if window is not None:
            break
    if window is None:
        return None, False
    sleep(WINDOW_SETTLE)
    window = next((w for w in backend.windows() if w.handle == window.handle), window)
    in_front = backend.foreground() == window.handle or backend.bring_to_front(window.handle)
    return window, in_front


# -- open windows (D-078) ------------------------------------------------------

RELAY_EXE = "relay"  # the Tauri app's binary stem (productName "Relay")
OFFSCREEN = -32000  # where Windows parks a minimized window
WINDOW_LIMIT = 40


def _stem(exe: str | None) -> str | None:
    return PureWindowsPath(exe).stem.casefold() if exe else None


def is_relay(window: Window) -> bool:
    return _stem(window.exe) == RELAY_EXE


def minimized(window: Window) -> bool:
    return window.rect.left <= OFFSCREEN


def window_label(window: Window, foreground: int) -> str:
    parts = [json.dumps(window.title, ensure_ascii=False)]
    if window.exe:
        parts.append(PureWindowsPath(window.exe).name)
    if is_relay(window):
        parts.append("Relay itself, never the target")
    if window.handle == foreground:
        parts.append("in front")
    elif minimized(window):
        parts.append("minimized")
    return " · ".join(parts)


def window_listing(windows: Sequence[Window], foreground: int, query: str | None = None) -> str:
    """Open windows, topmost first; what list_windows and the local operator see."""
    shown = list(windows)
    if query:
        shown = [w for _, w in rank_windows(shown, query)]
        if not shown:
            return f"No open window matches {query!r}. Use launch_app to open the program."
    if not shown:
        return "No open windows."
    head = f"{len(shown)} open window(s), topmost first:"
    lines = [window_label(w, foreground) for w in shown[:WINDOW_LIMIT]]
    if len(shown) > WINDOW_LIMIT:
        lines.append(f"... {len(shown) - WINDOW_LIMIT} more; pass query to filter.")
    return "\n".join([head, *lines])


def window_score(window: Window, query: str, app: App | None = None) -> float:
    q = query.casefold().strip()
    title = window.title.casefold()
    stem = _stem(window.exe)
    if not q:
        return 0.0
    if title == q:
        return 1.0
    if stem == q or (app is not None and app.exe is not None and _same_exe(window.exe, app.exe)):
        return 0.95
    if app is not None and stem is not None and stem == app.stem:
        return 0.95
    if q in title:
        return 0.85
    if stem is not None and stem.startswith(q):
        return 0.8
    return 0.0


def rank_windows(
    windows: Sequence[Window], query: str, app: App | None = None
) -> list[tuple[float, Window]]:
    """Matching windows, best first; ties keep z-order (topmost first). Never Relay's own."""
    scored = [(s, w) for w in windows if not is_relay(w) and (s := window_score(w, query, app)) > 0]
    return sorted(scored, key=lambda pair: -pair[0])  # stable: z-order within a score


def find_window(windows: Sequence[Window], query: str, catalog: AppCatalog | None = None) -> Window:
    """The open window ``query`` means: a title, a program name, or an installed app's name."""
    app = None
    if catalog is not None:
        try:
            app = catalog.resolve(query)
        except ActionError:
            app = None
    found = rank_windows(windows, query, app)
    if not found:
        raise ActionError(
            f"No open window matches {query!r}. Use list_windows to see them, "
            "or launch_app to open the program."
        )
    return found[0][1]
