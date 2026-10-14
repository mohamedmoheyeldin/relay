"""Manual check for app discovery (D-030) on a real Windows desktop (not run in CI).

    uv run python scripts/check_apps.py

The approval prompt is off for this script only (scripts/check_safety.py covers it).
1. Discovery: list_apps finds Notepad and a web browser; prints the browser "browser"
   resolves to and how long a scan takes.
2. Notepad: launch_app opens it, reports its window title and brings it to the front.
   Closes only the Notepad it opened.
3. Browser: launch_app("browser", ["https://example.com"]) opens the page in the
   owner's browser and reports that window. Closes only that window.
Prints PASS/FAIL per check.
"""

from __future__ import annotations

import ctypes
import re
import time

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen import displays  # noqa: E402
from relay_screen.actions import Computer  # noqa: E402
from relay_screen.apps import AppCatalog  # noqa: E402
from relay_screen.apps_windows import WinAppBackend  # noqa: E402
from relay_screen.capture import MssGrabber  # noqa: E402
from relay_screen.state import ScreenState  # noqa: E402

WM_CLOSE = 0x0010
results: dict[str, bool] = {}


def report(name: str, ok: bool, detail: str = "") -> None:
    results[name] = ok
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


def close(handle: int) -> None:
    ctypes.windll.user32.PostMessageW(handle, WM_CLOSE, 0, 0)  # pyright: ignore[reportAttributeAccessIssue]


backend = WinAppBackend()
catalog = AppCatalog(backend)
state = ScreenState(displays.enumerate_displays, displays.foreground_display)
computer = Computer(state, MssGrabber(), None, report_changes=False, apps=catalog)

# 1. Discovery.
start = time.monotonic()
apps = catalog.apps(refresh=True)
scan = time.monotonic() - start
print(computer.run("list_apps", {}).text)
browsers = [a for a in apps if a.browser or a.default_browser]
for b in browsers:
    print("Browser:", b)
report("Discovery finds apps", len(apps) > 20, f"{len(apps)} apps in {scan:.1f} s")
notepads = computer.run("list_apps", {"query": "notepad"}).text or ""
report("Discovery finds Notepad", "Notepad" in notepads, notepads.replace("\n", " | "))
browser = catalog.resolve("browser") if browsers else None
report("Discovery finds a browser", browser is not None, browser.name if browser else "")


def opened_window(text: str) -> int | None:
    """The handle of the window launch_app reported, looked up by its title."""
    title = re.search(r'window "(.*)"', text)
    if title is None:
        return None
    return next((w.handle for w in backend.windows() if w.title == title.group(1)), None)


# 2. Notepad by name.
text = computer.run("launch_app", {"name": "Notepad"}).text or ""
print(text)
handle = opened_window(text)
report("launch_app Notepad opens a window", handle is not None and "Notepad" in text, text)
report("launch_app brings it to the front", "in front" in text)
if handle is not None:
    close(handle)
    time.sleep(0.5)

# 3. The browser, with a URL.
if browser is not None:
    text = (
        computer.run(
            "launch_app", {"name": "browser", "args": ["--new-window", "https://example.com"]}
        ).text
        or ""
    )
    print(text)
    handle = opened_window(text)
    report("launch_app browser opens the page", "Example Domain" in text, text)
    if handle is not None and "Example Domain" in text:
        close(handle)

print("ALL PASS" if all(results.values()) else "SOME FAILED")
