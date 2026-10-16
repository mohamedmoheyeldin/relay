"""Manual check for M1-07 on a real Windows desktop (not run in CI).

    uv run python scripts/check_uia.py

Needs no clicks; the approval prompt is off for this script only (the prompt itself
is covered by scripts/check_safety.py).
1. Notepad: opens a new Notepad, lists its tree, finds File > Save with uia_find and
   activates both with uia_invoke (no mouse, no vision); passes if the Save dialog opens.
   Then cancels the dialog with Escape and closes the Notepad it opened.
2. Browser: opens example.com in a new Brave Origin window (the owner's browser, own
   profile) and checks that the page link comes back with bounds in screenshot
   pixels, then closes only that window. Without Brave Origin it uses a throwaway
   Edge (EdgeCore on newer installs) or Chrome profile with
   --force-renderer-accessibility (D-017). On several monitors, follows the
   "select_display N" hint as Cloud AI would.
3. Size: that window's tree at depth 12 must stay under ~8k tokens.
Prints PASS/FAIL per check.
"""

from __future__ import annotations

import csv
import ctypes
import io
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen import displays  # noqa: E402
from relay_screen.actions import Computer  # noqa: E402
from relay_screen.capture import MssGrabber  # noqa: E402
from relay_screen.input.driver import InputDriver  # noqa: E402
from relay_screen.input.sendinput import SendInputBackend  # noqa: E402
from relay_screen.state import ScreenState  # noqa: E402
from relay_screen.uia import UiaService  # noqa: E402
from relay_screen.uia_windows import WinUiaBackend  # noqa: E402

results: dict[str, bool] = {}


def report(name: str, ok: bool, detail: str = "") -> None:
    results[name] = ok
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


def pids(image: str) -> set[int]:
    out = subprocess.run(
        ["tasklist", "/fi", f"imagename eq {image}", "/fo", "csv", "/nh"],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    return {int(row[1]) for row in csv.reader(io.StringIO(out)) if len(row) > 1}


def first_id(text: str) -> str | None:
    lines = text.splitlines()
    if len(lines) < 2 or "[" not in lines[1]:
        return None
    return lines[1].split("]")[0].split("[")[1]


def follow_display(text: str) -> bool:
    """Select the display a listing points to, as Cloud AI would with select_display."""
    hint = re.search(r"select_display (\d+)", text)
    if hint is None:
        return False
    print("Selecting display", state.select(int(hint.group(1))).id)
    return True


def best(text: str) -> str:
    lines = text.splitlines()
    return lines[1].strip() if len(lines) > 1 else text


state = ScreenState(displays.enumerate_displays, displays.foreground_display)
uia = UiaService(WinUiaBackend())
computer = Computer(
    state, MssGrabber(), InputDriver(SendInputBackend()), report_changes=True, uia=uia
)

# 1. Notepad File > Save without vision.
before = pids("notepad.exe")
subprocess.Popen(["notepad.exe"])
time.sleep(2.5)
new_notepads = pids("notepad.exe") - before
try:
    # By title: a Notepad launched from a console often opens behind the foreground window.
    win = "Untitled - Notepad"
    tree = computer.run("uia_tree", {"window": win, "depth": 4}).text or ""
    if follow_display(tree):
        tree = computer.run("uia_tree", {"window": win, "depth": 4}).text or ""
    print(tree[:1500])
    file_id = first_id(
        computer.run("uia_find", {"query": "File", "role": "MenuItem", "window": win}).text or ""
    )
    opened = computer.run("uia_invoke", {"element": file_id}).text if file_id else None
    print("File:", opened)
    time.sleep(0.8)
    found = (
        computer.run("uia_find", {"query": "Save", "role": "MenuItem", "window": win}).text or ""
    )
    print(found)
    save_id = first_id(found)
    saved = computer.run("uia_invoke", {"element": save_id}).text if save_id else None
    print("Save:", saved)
    time.sleep(1.5)
    # role Window, so the still-listed "Save as" menu item cannot pass for the dialog.
    dialog = computer.run("uia_find", {"query": "Save as", "role": "Window", "window": win}).text
    dialog = dialog or ""
    report(
        "Notepad File > Save via UIA",
        bool(file_id and save_id) and "No element" not in dialog,
        best(dialog),
    )
    computer.run("key", {"text": "Escape"})
    time.sleep(0.5)
finally:
    for pid in new_notepads:
        subprocess.run(["taskkill", "/pid", str(pid), "/f"], check=False, capture_output=True)


# 2 + 3. Browser page controls with bounds, and tree size.
def owner_brave() -> Path | None:
    """Brave Origin's program, found by app discovery (D-030), not a fixed path."""
    from relay_screen.apps import AppCatalog
    from relay_screen.apps_windows import WinAppBackend

    apps = AppCatalog(WinAppBackend()).apps()
    found = next((a for a in apps if a.exe and "brave origin" in a.name.casefold()), None)
    return Path(found.exe) if found and found.exe else None


BRAVE = owner_brave()
WM_CLOSE = 0x0010


def close_window(title: str) -> None:
    """Close one top-level window by exact title, leaving the rest of the browser open."""
    user32 = ctypes.windll.user32  # pyright: ignore[reportAttributeAccessIssue]
    hwnd = user32.FindWindowW(None, title)
    if hwnd:
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)


def throwaway_browser() -> str:
    """Edge (EdgeCore on newer installs) or Chrome, for PCs without Brave Origin."""
    candidates = (
        shutil.which("msedge"),
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        *map(str, sorted(Path(r"C:\Program Files (x86)\Microsoft\EdgeCore").glob("*/msedge.exe"))),
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    )
    found = next((c for c in candidates if c and Path(c).exists()), None)
    if found is None:
        raise SystemExit("no Brave Origin, Edge or Chrome found")
    return found


# Brave Origin is the owner's browser. A fresh profile hits its purchase screen, so
# open one new window in the owner's own profile and close just that window. The
# screen-reader flag (D-017) makes it build the full tree. Elsewhere use a throwaway
# Edge/Chrome profile.
profile: Path | None = None
proc: subprocess.Popen[bytes] | None = None
if BRAVE is not None:
    browser = "Brave Origin"
    subprocess.Popen([str(BRAVE), "--new-window", "https://example.com"])
else:
    exe = throwaway_browser()
    browser = Path(exe).stem
    profile = Path(tempfile.mkdtemp(prefix="relay-edge-"))
    proc = subprocess.Popen(
        [
            exe,
            f"--user-data-dir={profile}",
            "--no-first-run",
            "--force-renderer-accessibility",
            "--new-window",
            "https://example.com",
        ]
    )
print("Browser:", browser)
try:
    time.sleep(6)
    links = computer.run("uia_find", {"query": "Learn more", "window": "Example"}).text or ""
    if follow_display(links):
        links = computer.run("uia_find", {"query": "Learn more", "window": "Example"}).text or ""
    print(links)
    report(f"{browser} page control with bounds", "center (" in links, best(links))
    tree = computer.run("uia_tree", {"window": "Example", "depth": 12}).text or ""
    tokens = len(tree) // 4
    report(f"{browser} tree under 8k tokens", tokens < 8000, f"~{tokens} tokens, {len(tree)} chars")
finally:
    if proc is None:
        close_window(f"Example Domain - {browser}")
    else:
        subprocess.run(
            ["taskkill", "/pid", str(proc.pid), "/t", "/f"], check=False, capture_output=True
        )
        time.sleep(1)
    if profile is not None:
        shutil.rmtree(profile, ignore_errors=True)

print("ALL PASS" if all(results.values()) else "SOME FAILED")
