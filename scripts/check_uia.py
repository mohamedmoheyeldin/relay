"""Manual check for M1-07 on a real Windows desktop (not run in CI).

    uv run python scripts/check_uia.py

Needs no clicks; the approval prompt is off for this script only (the prompt itself
is covered by scripts/check_safety.py).
1. Notepad: opens a new Notepad, lists its tree, finds File > Save with uia_find and
   activates both with uia_invoke (no mouse, no vision); passes if the Save dialog opens.
   Then cancels the dialog with Escape and closes the Notepad it opened.
2. Edge: opens example.com in a throwaway Edge profile (with
   --force-renderer-accessibility, D-017) and checks that page links come back with
   bounds in screenshot pixels. Closes that Edge afterwards.
3. Size: the Edge window's tree at depth 12 must stay under ~8k tokens.
Prints PASS/FAIL per check.
"""

from __future__ import annotations

import csv
import io
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
    tree = computer.run("uia_tree", {"depth": 4}).text or ""
    print(tree[:1500])
    file_id = first_id(computer.run("uia_find", {"query": "File", "role": "MenuItem"}).text or "")
    opened = computer.run("uia_invoke", {"element": file_id}).text if file_id else None
    print("File:", opened)
    time.sleep(0.8)
    found = computer.run("uia_find", {"query": "Save", "role": "MenuItem"}).text or ""
    print(found)
    save_id = first_id(found)
    saved = computer.run("uia_invoke", {"element": save_id}).text if save_id else None
    print("Save:", saved)
    time.sleep(1.5)
    dialog = computer.run("uia_find", {"query": "Save as"}).text or ""
    report(
        "Notepad File > Save via UIA",
        bool(file_id and save_id) and "No element" not in dialog,
        dialog.splitlines()[0] if dialog else "",
    )
    computer.run("key", {"text": "Escape"})
    time.sleep(0.5)
finally:
    for pid in new_notepads:
        subprocess.run(["taskkill", "/pid", str(pid), "/f"], check=False, capture_output=True)

# 2 + 3. Edge page controls with bounds, and tree size.
edge = shutil.which("msedge") or r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
profile = Path(tempfile.mkdtemp(prefix="relay-edge-"))
proc = subprocess.Popen(
    [
        edge,
        f"--user-data-dir={profile}",
        "--no-first-run",
        "--force-renderer-accessibility",
        "--new-window",
        "https://example.com",
    ]
)
try:
    time.sleep(6)
    links = computer.run("uia_find", {"query": "More information", "window": "Example"}).text
    links = links or ""
    if "No element" in links:
        links = computer.run("uia_find", {"query": "Learn more", "window": "Example"}).text or ""
    print(links)
    report("Edge page control with bounds", "center (" in links, links.splitlines()[0])
    tree = computer.run("uia_tree", {"window": "Example", "depth": 12}).text or ""
    tokens = len(tree) // 4
    report("Edge tree under 8k tokens", tokens < 8000, f"~{tokens} tokens, {len(tree)} chars")
finally:
    subprocess.run(
        ["taskkill", "/pid", str(proc.pid), "/t", "/f"], check=False, capture_output=True
    )
    time.sleep(1)
    shutil.rmtree(profile, ignore_errors=True)

print("ALL PASS" if all(results.values()) else "SOME FAILED")
