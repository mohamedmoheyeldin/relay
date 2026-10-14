"""Manual check for M1-04 on a real Windows desktop (not run in CI).

    uv run python scripts/check_input.py

1. Moves the cursor to the centre and corners of every display and reports any error.
2. Opens Notepad and types a test line with Unicode characters.
Move the mouse away at any time to watch; nothing is saved or clicked.
"""

from __future__ import annotations

import subprocess
import time

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen.displays import enumerate_displays  # noqa: E402
from relay_screen.input.driver import InputDriver  # noqa: E402
from relay_screen.input.sendinput import SendInputBackend  # noqa: E402

backend = SendInputBackend()
driver = InputDriver(backend)

worst = 0
for d in enumerate_displays():
    r = d.rect
    print(f"display {d.id} {d.name} {r} scale={d.scale} primary={d.primary}")
    for x, y in [
        (r.left + r.width // 2, r.top + r.height // 2),
        (r.left + 10, r.top + 10),
        (r.right - 11, r.bottom - 11),
    ]:
        driver.move(x, y)
        time.sleep(0.15)
        cx, cy = backend.cursor()
        error = max(abs(cx - x), abs(cy - y))
        worst = max(worst, error)
        print(f"  target ({x}, {y}) -> cursor ({cx}, {cy})  error {error}px")
print(f"worst cursor error: {worst}px")

subprocess.Popen(["notepad.exe"])
time.sleep(1.5)
driver.type_text("Hello, Relay ✓ éè \U0001f600\nSecond line\ttabbed")
print("Typed into Notepad. Close it without saving.")
