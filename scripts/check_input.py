"""Manual check for M1-04 on a real Windows desktop (not run in CI).

    uv run python scripts/check_input.py

1. Moves the cursor to the centre and corners of every display and reports any error.
2. Opens a small always-on-top text box, clicks into it, types a test string with
   Unicode, a newline and a tab, and compares what the box received with what was sent.
Nothing is saved; the text box closes itself.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen.displays import enumerate_displays  # noqa: E402
from relay_screen.input.driver import InputDriver  # noqa: E402
from relay_screen.input.sendinput import SendInputBackend  # noqa: E402

EXPECTED = "Hello, Relay ✓ éè \U0001f600\nSecond line\ttabbed"

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

result = Path(tempfile.mkdtemp(prefix="relay-")) / "typed.json"
target = Path(__file__).with_name("type_target.py")
proc = subprocess.Popen([sys.executable, str(target), str(result), "8"])
ready = result.with_suffix(".ready")
deadline = time.monotonic() + 10
while not ready.exists() and time.monotonic() < deadline:
    time.sleep(0.1)
if not ready.exists():
    sys.exit("typing target did not start")
time.sleep(0.2)
box = json.loads(ready.read_text(encoding="utf-8"))
driver.click((box["x"] + 40, box["y"] + 20))  # click gives the box keyboard focus
time.sleep(0.3)
started = time.perf_counter()
driver.type_text(EXPECTED)
print(f"typed {len(EXPECTED)} characters in {time.perf_counter() - started:.2f}s")
proc.wait(timeout=20)
received = json.loads(result.read_text(encoding="utf-8"))["text"]
print("expected:", ascii(EXPECTED))
print("received:", ascii(received))
print("TYPING OK" if received == EXPECTED else "TYPING MISMATCH")
sys.exit(0 if received == EXPECTED and worst <= 1 else 1)
