"""Manual check for M1-08 on a real Windows desktop (not run in CI).

    uv run python scripts/check_ocr.py

Opens a new Notepad, opens its Save dialog through UIA, and checks that find_text
finds the dialog's "Save" and "Cancel" buttons. Then reports the median full-display
OCR time over 5 runs on the active display. Cancels the dialog and closes only the
Notepad it opened.
"""

from __future__ import annotations

import csv
import io
import statistics
import subprocess
import time

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen import displays  # noqa: E402
from relay_screen.actions import Computer  # noqa: E402
from relay_screen.capture import MssGrabber  # noqa: E402
from relay_screen.input.driver import InputDriver  # noqa: E402
from relay_screen.input.sendinput import SendInputBackend  # noqa: E402
from relay_screen.ocr import OcrService  # noqa: E402
from relay_screen.ocr_windows import WinOcrEngine  # noqa: E402
from relay_screen.state import ScreenState  # noqa: E402
from relay_screen.uia import UiaService  # noqa: E402
from relay_screen.uia_windows import WinUiaBackend  # noqa: E402

results: dict[str, bool] = {}


def report(name: str, ok: bool, detail: str = "") -> None:
    results[name] = ok
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


def pids() -> set[int]:
    out = subprocess.run(
        ["tasklist", "/fi", "imagename eq notepad.exe", "/fo", "csv", "/nh"],
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


def select_hint(text: str) -> int | None:
    marker = "select_display "
    if marker not in text:
        return None
    return int(text.split(marker)[1].split(")")[0])


grabber = MssGrabber()
state = ScreenState(displays.enumerate_displays, displays.foreground_display)
ocr = OcrService(WinOcrEngine(), grabber)
computer = Computer(
    state,
    grabber,
    InputDriver(SendInputBackend()),
    report_changes=False,
    uia=UiaService(WinUiaBackend()),
    ocr=ocr,
)

before = pids()
subprocess.Popen(["notepad.exe"])
time.sleep(2.5)
mine = pids() - before
try:
    found = computer.run("uia_find", {"query": "File", "role": "MenuItem", "window": "Notepad"})
    hint = select_hint(found.text or "")
    if hint is not None:
        state.select(hint)
        found = computer.run("uia_find", {"query": "File", "role": "MenuItem", "window": "Notepad"})
    file_id = first_id(found.text or "")
    if file_id:
        computer.run("uia_invoke", {"element": file_id})
        time.sleep(0.8)
    save = first_id(computer.run("uia_find", {"query": "Save", "role": "MenuItem"}).text or "")
    if save:
        computer.run("uia_invoke", {"element": save})
        time.sleep(1.5)
    for word in ("Save", "Cancel"):
        text = computer.run("find_text", {"text": word, "fuzzy": False}).text or ""
        print(text)
        report(f'find_text "{word}" in the Save dialog', "center (" in text, text.splitlines()[0])
    timings = []
    for _ in range(5):
        ocr.read(state.geometry().display)
        timings.append(ocr.last_seconds)
    median = statistics.median(timings)
    display = state.geometry().display
    report(
        "full-display OCR latency",
        median < 3.0,
        f"median {median:.2f} s over 5 runs on {display.width}x{display.height}",
    )
    computer.run("key", {"text": "Escape"})
    time.sleep(0.5)
finally:
    for pid in mine:
        subprocess.run(["taskkill", "/pid", str(pid), "/f"], check=False, capture_output=True)

print("ALL PASS" if all(results.values()) else "SOME FAILED")
