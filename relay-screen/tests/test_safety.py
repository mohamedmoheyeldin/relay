import json
import threading

import pytest

from relay_screen.actions import HALT_TEXT, Computer
from relay_screen.input.driver import InputDriver
from relay_screen.input.events import KeyEvent, MouseButton, MouseMove
from relay_screen.safety.approval import (
    ActRequest,
    AppPrompter,
    ApprovalDeniedError,
    ApprovalGate,
    Decision,
    PromptRequest,
    describe,
    is_risky,
)
from relay_screen.safety.audit import AuditLog
from relay_screen.safety.killswitch import (
    GuardedBackend,
    KillSwitch,
    KillSwitchError,
    KillZoneError,
)
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber, RecordingBackend


class FakePrompter:
    def __init__(self, *answers: Decision) -> None:
        self.answers: list[Decision] = list(answers)
        self.requests: list[PromptRequest] = []

    def ask(self, request: PromptRequest) -> Decision:
        self.requests.append(request)
        return self.answers.pop(0) if self.answers else "deny"


def computer_with(owner_displays, *, switch=None, gate=None, audit=None, backend=None):
    backend = backend or RecordingBackend()
    switch = switch or KillSwitch()
    hooks = []

    def check_switch(request: ActRequest) -> None:
        switch.check()

    hooks.append(check_switch)
    if gate is not None:
        hooks.append(gate.check)
    computer = Computer(
        ScreenState(lambda: owner_displays, lambda ds: None),
        GradientGrabber(),
        InputDriver(GuardedBackend(backend, switch)),
        report_changes=False,
        before_act=hooks,
        sleep=switch.wait,
        audit=audit,
    )
    return computer, backend, switch


# -- kill switch ---------------------------------------------------------------


def test_corner_trips_and_rearm_clears():
    switch = KillSwitch()
    switch.poll_cursor(lambda: (500, 500))
    assert not switch.tripped
    switch.poll_cursor(lambda: (3, 5))
    assert switch.tripped and "corner" in switch.reason
    with pytest.raises(KillSwitchError, match="kill switch"):
        switch.check()
    switch.rearm()
    switch.check()


def test_guarded_backend_refuses_presses_but_lets_releases_through():
    inner = RecordingBackend()
    switch = KillSwitch()
    backend = GuardedBackend(inner, switch)
    switch.trip("test")
    with pytest.raises(KillSwitchError):
        backend.send([KeyEvent(0x41, True)])
    backend.send([KeyEvent(0x41, False), MouseButton("left", False)])
    assert inner.events == [KeyEvent(0x41, False), MouseButton("left", False)]


def test_relay_never_moves_into_the_corner(owner_displays):
    computer, backend, _ = computer_with(owner_displays)
    with pytest.raises(KillZoneError):
        computer.run("mouse_move", {"coordinate": [0, 0]})
    assert backend.events == []
    computer.run("mouse_move", {"coordinate": [10, 10]})  # outside the 8x8 px corner


def test_wait_wakes_up_when_tripped():
    switch = KillSwitch()
    threading.Timer(0.05, switch.trip, args=("test",)).start()
    with pytest.raises(KillSwitchError):
        switch.wait(5)


def test_corner_during_a_ten_step_batch_stops_within_one_action(owner_displays):
    inner = RecordingBackend()
    switch = KillSwitch()
    sent = 0

    def send(events):
        nonlocal sent
        inner.batches.append(list(events))
        sent += 1
        if sent == 3:  # the owner reaches the corner while step 3 runs
            switch.poll_cursor(lambda: (0, 0))

    inner.send = send  # type: ignore[method-assign]
    computer, _, _ = computer_with(owner_displays, switch=switch, backend=inner)
    actions = [{"name": "key", "input": {"text": "a"}} for _ in range(10)]
    results = computer.run_batch(actions)
    assert [r.is_error for r in results[:3]] == [False, False, False]
    assert results[3].is_error and "kill switch" in (results[3].text or "")
    assert all(r.text == HALT_TEXT for r in results[4:])
    assert sent == 3


def test_hold_key_released_when_stopped(owner_displays):
    switch = KillSwitch()
    computer, backend, _ = computer_with(owner_displays, switch=switch)
    threading.Timer(0.05, switch.trip, args=("test",)).start()
    with pytest.raises(KillSwitchError):
        computer.run("hold_key", {"text": "shift", "duration": 5})
    assert backend.events[-1] == KeyEvent(0x10, False)


def test_reads_still_work_when_stopped(owner_displays):
    computer, _, switch = computer_with(owner_displays)
    switch.trip("test")
    assert computer.run("screenshot").image is not None
    with pytest.raises(KillSwitchError):
        computer.run("left_click", {"coordinate": [10, 10]})


# -- approval gate ---------------------------------------------------------------


def test_every_act_is_asked_and_reads_are_not(owner_displays):
    prompter = FakePrompter("approve", "approve")
    computer, backend, _ = computer_with(owner_displays, gate=ApprovalGate(prompter))
    computer.run("screenshot")
    computer.run("left_click", {"coordinate": [100, 50]})
    computer.run("type", {"text": "hi"})
    assert [r.detail for r in prompter.requests] == ["Left click at (100, 50)", 'Type "hi"']
    assert prompter.requests[0].image is not None  # preview with the target circled
    assert backend.events[0] == MouseMove(*computer.state.geometry().to_physical(100, 50))


def test_denied_act_sends_nothing(owner_displays):
    computer, backend, _ = computer_with(owner_displays, gate=ApprovalGate(FakePrompter("deny")))
    with pytest.raises(ApprovalDeniedError, match="denied"):
        computer.run("key", {"text": "Return"})
    assert backend.events == []


def test_timeout_counts_as_denied(owner_displays):
    gate = ApprovalGate(FakePrompter("timeout"), timeout=30)
    computer, _, _ = computer_with(owner_displays, gate=gate)
    with pytest.raises(ApprovalDeniedError, match="30 s"):
        computer.run("key", {"text": "Return"})


def test_batch_asks_per_action_until_rest_is_approved(owner_displays):
    prompter = FakePrompter("approve", "approve_batch")
    computer, _, _ = computer_with(owner_displays, gate=ApprovalGate(prompter))
    actions = [{"name": "key", "input": {"text": k}} for k in ("a", "b", "c", "d")]
    results = computer.run_batch(actions)
    assert not any(r.is_error for r in results)
    assert [(r.step, r.total) for r in prompter.requests] == [(1, 4), (2, 4)]
    assert prompter.requests[0].allow_batch
    # The next batch starts asking again.
    prompter.answers = ["deny"]
    results = computer.run_batch(actions[:2])
    assert results[0].is_error and results[1].text == HALT_TEXT


def test_denial_mid_batch_halts_the_rest(owner_displays):
    prompter = FakePrompter("approve", "deny")
    computer, backend, _ = computer_with(owner_displays, gate=ApprovalGate(prompter))
    actions = [{"name": "key", "input": {"text": "a"}} for _ in range(4)]
    results = computer.run_batch(actions)
    assert [r.is_error for r in results] == [False, True, True, True]
    assert results[2].text == HALT_TEXT
    assert len(backend.batches) == 1


def test_approval_off_never_asks(owner_displays):
    prompter = FakePrompter()
    gate = ApprovalGate(prompter, mode="off")
    computer, _, _ = computer_with(owner_displays, gate=gate)
    computer.run("key", {"text": "a"})
    assert prompter.requests == []


@pytest.mark.parametrize(
    ("name", "params", "text"),
    [
        ("double_click", {"text": "ctrl"}, "Double click where the mouse is holding ctrl"),
        (
            "left_click_drag",
            {"start_coordinate": [1, 2], "coordinate": [3, 4]},
            "Drag from (1, 2) to (3, 4)",
        ),
        ("scroll", {"scroll_direction": "down", "scroll_amount": 3}, "Scroll down 3 notches"),
        ("key", {"text": "ctrl+s", "repeat": 2}, "Press ctrl+s 2 times"),
        ("hold_key", {"text": "shift", "duration": 1}, "Hold shift for 1 s"),
        ("type", {"text": "x" * 400}, 'Type "' + "x" * 300 + '…"'),
    ],
)
def test_describe(name, params, text):
    assert describe(name, params) == text


# -- audit log -------------------------------------------------------------------


def test_audit_log_records_every_action(owner_displays, tmp_path):
    audit = AuditLog.create(tmp_path, "run1")
    prompter = FakePrompter("approve", "deny")
    computer, _, _ = computer_with(owner_displays, gate=ApprovalGate(prompter), audit=audit)
    computer.run("screenshot")
    computer.run("key", {"text": "a"})
    with pytest.raises(ApprovalDeniedError):
        computer.run("type", {"text": "secret plan"})
    lines = [json.loads(line) for line in audit.path.read_text(encoding="utf-8").splitlines()]
    assert [line["action"] for line in lines] == ["screenshot", "key", "type"]
    assert "approval" not in lines[0]
    assert lines[1]["approval"] == "approved" and lines[1]["result"] == "OK"
    assert (audit.directory / lines[1]["before"]).exists()
    assert (audit.directory / lines[1]["after"]).exists()
    assert "denied" in lines[2]["error"]
    assert all("duration_ms" in line and "time" in line for line in lines)


def _run_prompter(monkeypatch, stdout="", stderr="", returncode=0, raises=None):
    import subprocess
    import types

    from relay_screen import win32
    from relay_screen.safety.approval import PromptRequest, SubprocessPrompter

    fake_user32 = types.SimpleNamespace(GetForegroundWindow=lambda: 0)
    monkeypatch.setattr(win32, "user32", lambda: fake_user32)

    def run(*args, **kwargs):
        if raises:
            raise raises
        return subprocess.CompletedProcess(args, returncode, stdout, stderr)

    monkeypatch.setattr(subprocess, "run", run)
    return SubprocessPrompter().ask(PromptRequest(title="t", detail="d"))


def test_prompter_returns_the_owners_answer(monkeypatch):
    assert _run_prompter(monkeypatch, stdout='{"decision": "approve"}') == "approve"
    assert _run_prompter(monkeypatch, stdout='{"decision": "deny"}') == "deny"


def test_prompter_failure_is_not_a_denial(monkeypatch):
    import subprocess

    from relay_screen.safety.approval import PromptUnavailableError

    crash = "Traceback...\nModuleNotFoundError: No module named 'tkinter'\n"
    with pytest.raises(PromptUnavailableError, match="tkinter"):
        _run_prompter(monkeypatch, stderr=crash, returncode=1)
    with pytest.raises(PromptUnavailableError, match="OSError: boom"):
        _run_prompter(monkeypatch, stdout='{"decision": "error", "error": "OSError: boom"}')
    with pytest.raises(PromptUnavailableError, match="could not start"):
        _run_prompter(monkeypatch, raises=OSError("no python"))
    timeout = subprocess.TimeoutExpired("python", 1)
    assert _run_prompter(monkeypatch, raises=timeout) == "timeout"


@pytest.mark.parametrize(
    ("name", "summary", "risky"),
    [
        ("left_click", 'Left click at (1, 2) on Button "Send"', True),
        ("double_click", 'Double click at (1, 2) on ListItem "Delete me"', True),
        ("left_click", 'Left click at (1, 2) on Button "Search"', False),
        ("left_click", None, False),
        ("uia_invoke", 'Activate Button "Place order"', True),
        ("uia_invoke", 'Activate MenuItem "Save"', False),
        ("type", 'Type "send it"', False),
        ("uia_set_value", 'Set Edit "To" to "delete"', False),
    ],
)
def test_what_counts_as_risky(name, summary, risky):
    assert is_risky(ActRequest(name, {}, summary=summary)) is risky


def test_risky_mode_asks_for_risky_acts_only():
    prompter = FakePrompter("approve")
    gate = ApprovalGate(prompter, mode="risky")
    assert gate.check(ActRequest("key", {"text": "ctrl+s"})) == "not asked (not risky)"
    assert gate.check(ActRequest("uia_invoke", {}, summary='Activate Button "Pay now"')) == (
        "approved"
    )
    assert [r.detail for r in prompter.requests] == ['Activate Button "Pay now"']


def test_the_owners_words_approve_the_steps_they_name():
    prompter = FakePrompter("deny")
    gate = ApprovalGate(prompter, mode="risky")
    gate.grant("Fill the Dice application and submit it")
    submit = ActRequest("uia_invoke", {}, summary='Activate Button "Submit application"')
    assert gate.check(submit) == "approved (the owner's task says submit)"
    assert prompter.requests == []
    # A step the task didn't name still asks, even next to a granted word.
    both = ActRequest("left_click", {}, summary='Left click on Button "Submit and delete"')
    with pytest.raises(ApprovalDeniedError):
        gate.check(both)
    gate.grant(None)
    with pytest.raises(ApprovalDeniedError):
        gate.check(submit)
    assert len(prompter.requests) == 2


class _Fallback:
    def __init__(self) -> None:
        self.asked = 0

    def ask(self, request: PromptRequest) -> Decision:
        self.asked += 1
        return "deny"


def _app(answer: dict, seen: list) -> tuple:
    from http.server import BaseHTTPRequestHandler, HTTPServer

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self) -> None:
            body = self.rfile.read(int(self.headers["Content-Length"]))
            seen.append((self.path, self.headers["Authorization"], json.loads(body)))
            data = json.dumps(answer).encode()
            self.send_response(200)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, *args: object) -> None:
            pass

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, {"port": server.server_address[1], "token": "t0k"}


def test_app_prompter_asks_in_the_relay_app() -> None:
    seen: list = []
    server, info = _app({"decision": "approve_batch"}, seen)
    fallback = _Fallback()
    try:
        prompter = AppPrompter(fallback=fallback, address=lambda: info)
        request = PromptRequest(title="Relay wants to act", detail="Click", image=b"png")
        assert prompter.ask(request) == "approve_batch"
    finally:
        server.shutdown()
    path, auth, body = seen[0]
    assert path == "/ask" and auth == "Bearer t0k"
    assert body["detail"] == "Click" and body["image"] == "cG5n"
    assert fallback.asked == 0


def test_app_prompter_falls_back_when_the_app_is_not_running() -> None:
    fallback = _Fallback()
    assert (
        AppPrompter(fallback=fallback, address=lambda: None).ask(PromptRequest("t", "d")) == "deny"
    )
    closed = AppPrompter(fallback=fallback, address=lambda: {"port": 1, "token": "x"})
    assert closed.ask(PromptRequest("t", "d", timeout=1)) == "deny"
    assert fallback.asked == 2
