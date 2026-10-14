"""Manual check for M1-06 on a real Windows desktop (not run in CI).

    uv run python scripts/check_safety.py

Needs no clicks from the owner:
1. Approval prompt: asks about a harmless act (pressing Shift) with a short timeout,
   takes a screenshot to show the prompt is up, then tries to click Approve with
   synthetic input. The prompt must ignore that click and time out as denied.
2. Kill switch, corner: a 10-step batch of mouse moves; after step 3 the script puts
   the cursor in the top-left corner the way the owner would (SetCursorPos, not
   Relay's input). The batch must stop and halt the rest.
3. Kill switch, hotkey: presses Ctrl+Alt+Pause and checks that it trips.
4. Audit log: every action above must be in runs/<run-id>/actions.jsonl.
Prints PASS/FAIL per check. The prompt screenshot is saved next to the log.
"""

from __future__ import annotations

import ctypes
import ctypes.wintypes
import json
import tempfile
import threading
import time
from pathlib import Path

from relay_screen.dpi import set_dpi_awareness

print("DPI awareness:", set_dpi_awareness())

from relay_screen import displays, win32  # noqa: E402
from relay_screen.actions import HALT_TEXT, Computer  # noqa: E402
from relay_screen.capture import MssGrabber  # noqa: E402
from relay_screen.input.driver import InputDriver  # noqa: E402
from relay_screen.input.events import KeyEvent  # noqa: E402
from relay_screen.input.keys import VK_CONTROL, VK_MENU, VK_PAUSE  # noqa: E402
from relay_screen.input.sendinput import SendInputBackend  # noqa: E402
from relay_screen.safety.approval import (  # noqa: E402
    ActRequest,
    ApprovalDeniedError,
    ApprovalGate,
    SubprocessPrompter,
)
from relay_screen.safety.audit import AuditLog  # noqa: E402
from relay_screen.safety.killswitch import GuardedBackend, KillSwitch  # noqa: E402
from relay_screen.state import ScreenState  # noqa: E402

results: dict[str, bool] = {}


def report(name: str, ok: bool, detail: str = "") -> None:
    results[name] = ok
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


raw = SendInputBackend()
switch = KillSwitch()
switch.start_cursor_watcher(win32.cursor_position)
switch.start_hotkey_watcher()
audit = AuditLog.create(Path(tempfile.gettempdir()) / "relay-check-runs")
state = ScreenState(displays.enumerate_displays, displays.foreground_display)
grabber = MssGrabber()
gate = ApprovalGate(SubprocessPrompter(), timeout=8)


def check_switch(request: ActRequest) -> None:
    switch.check()


def make(with_gate: bool) -> Computer:
    hooks = [check_switch] + ([gate.check] if with_gate else [])
    return Computer(
        state,
        grabber,
        InputDriver(GuardedBackend(raw, switch)),
        report_changes=False,
        before_act=hooks,
        sleep=switch.wait,
        audit=audit,
    )


primary = next(d for d in state.displays() if d.primary)
state.select(primary.id)
geometry = state.geometry()
print("Active display:", primary.name, primary.rect)

# 1. Approval prompt ignores synthetic clicks and times out as denied.
found: dict[str, object] = {}


def try_to_self_approve() -> None:
    time.sleep(2.5)  # let the prompt appear
    user32 = win32.user32()
    hwnd = user32.FindWindowW(None, "Relay: approval needed")
    found["hwnd"] = hwnd
    grabber.grab(geometry.display).save(audit.directory / "prompt.png")
    if hwnd:
        rect = ctypes.wintypes.RECT()
        user32.GetWindowRect(hwnd, ctypes.byref(rect))
        # Approve is the second button from the right; click across the button row.
        y = rect.bottom - 25
        for x in range(rect.right - 330, rect.right - 20, 30):
            user32.SetCursorPos(x, y)
            raw.send([*_click()])
            time.sleep(0.05)


def _click():
    from relay_screen.input.events import MouseButton

    return [MouseButton("left", True), MouseButton("left", False)]


threading.Thread(target=try_to_self_approve, daemon=True).start()
start = time.monotonic()
try:
    make(with_gate=True).run("key", {"text": "shift"})
    report("prompt ignores synthetic clicks", False, "the act was approved")
except ApprovalDeniedError as exc:
    waited = time.monotonic() - start
    ok = bool(found.get("hwnd")) and "No answer" in str(exc) and waited >= 7
    report(
        "prompt ignores synthetic clicks",
        ok,
        f"window found={bool(found.get('hwnd'))}, denied after {waited:.1f} s",
    )

# 2. Corner stops a 10-step batch within one action.
center = (geometry.width // 2, geometry.height // 2)
actions = [
    {"name": "mouse_move", "input": {"coordinate": [center[0] + 20 * i, center[1]]}}
    for i in range(10)
]
original_send = raw.send
steps = 0


def counting_send(events):
    global steps
    original_send(events)
    steps += 1
    if steps == 3:
        win32.user32().SetCursorPos(0, 0)  # the owner flicks the mouse to the corner
        time.sleep(0.2)  # the watcher polls every 50 ms


raw.send = counting_send  # type: ignore[method-assign]
batch = make(with_gate=False).run_batch(actions)
raw.send = original_send  # type: ignore[method-assign]
first_error = next((i for i, r in enumerate(batch) if r.is_error), None)
ok = (
    first_error == 3
    and "kill switch" in (batch[3].text or "")
    and all(r.text == HALT_TEXT for r in batch[4:])
)
report("corner stops a 10-step batch", ok, f"first error at step {(first_error or -1) + 1}")
win32.user32().SetCursorPos(primary.rect.left + 400, primary.rect.top + 400)
time.sleep(0.2)
switch.rearm()

# 3. Ctrl+Alt+Pause trips the switch.
raw.send([KeyEvent(VK_CONTROL, True), KeyEvent(VK_MENU, True), KeyEvent(VK_PAUSE, True)])
raw.send([KeyEvent(VK_PAUSE, False), KeyEvent(VK_MENU, False), KeyEvent(VK_CONTROL, False)])
time.sleep(0.5)
report("Ctrl+Alt+Pause trips the switch", switch.tripped, switch.reason)
switch.rearm()

# 4. Audit log has every action.
lines = [json.loads(line) for line in audit.path.read_text(encoding="utf-8").splitlines()]
logged = [line["action"] for line in lines]
ok = logged[:1] == ["key"] and logged.count("mouse_move") == 4 and "error" in lines[0]
report("audit log", ok, f"{len(lines)} lines in {audit.path}")

print("ALL PASS" if all(results.values()) else "SOME FAILED")
