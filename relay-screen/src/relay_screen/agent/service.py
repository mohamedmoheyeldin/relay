"""Relay's operator service (D-086): the local AI kept running while Relay is open.

    relay-screen operator

The app starts it once and talks JSON lines over stdin/stdout:

    {"op": "task", "id": "t1", "text": "go to dice.com and search python jobs"}
    {"op": "say", "id": "t1", "text": "use the remote filter too"}
    {"op": "stop", "id": "t1"}
    {"op": "route", "id": "r1", "text": "<owner message>", "claude": false}  (claude optional)
    {"op": "state", "id": "s1"}
    {"op": "quit"}

Every event it writes carries the id it belongs to:

    {"id": "t1", "event": "accepted", "text": ...}
    {"id": "t1", "event": "step", "step": 3, "say": "Typed \\"python\\" into ...", ...}
    {"id": "t1", "event": "note", "text": "Loading the local model..."}
    {"id": "t1", "event": "done" | "failed" | "stopped", "message": ..., "steps": n}
    {"id": "t1", "event": "unavailable", "message": ...}
    {"id": "r1", "event": "route", "to": "operator" | "claude", "why": ...}
    {"id": "s1", "event": "state", "foreground": ..., "browser_url": ..., ...}

It also listens on a localhost port named in `operator.json` (with a token), so the
`ui_task` tool in the relay-screen Claude starts each chat turn hands its tasks here:
one mouse, one operator, and the app sees those steps too ("source": "claude").
The interface stays the same when the operator moves into relay-core (D-088).
"""

from __future__ import annotations

import contextlib
import json
import os
import re
import secrets
import shutil
import socket
import subprocess
import sys
import threading
import time
from collections import deque
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

from relay_screen.agent.local import (
    ADDRESS_BAR,
    BROWSERS,
    MAX_STEPS,
    Hands,
    Outcome,
    Status,
    instant_plan,
)
from relay_screen.runtime.supervisor import RuntimeUnavailable

Write = Callable[[dict[str, Any]], None]

POLL_SECONDS = 1.0  # foreground watch
RECENT = 20  # foreground changes kept in memory (never written to disk)
ASK_TIMEOUT = 120.0  # s for one ask-Claude answer
ANSWER_LIMIT = 4000

# Claude for thinking: writing, judging, explaining, planning (D-088).
THINK = re.compile(
    r"\b(write|draft|compose|reply to|summari[sz]e|explain|why|what is|what's|what are|"
    r"how do|how does|how can|research|compare|analy[sz]e|plan|recommend|translate|"
    r"calculate|tell me|should i|which (?:one|is)|review|decide|cover letter|email .* about)\b",
    re.IGNORECASE,
)
# The local AI for doing things on the screen.
SCREEN = re.compile(
    r"\b(open|go to|goto|visit|navigate|click|press|type|fill|search|scroll|switch|close|"
    r"minimi[sz]e|maximi[sz]e|select|tick|check the box|log ?in|sign in|download|upload|"
    r"attach|move|drag|launch|start|play|pause|mute|apply|bookmark|refresh|reload|"
    r"go back|scroll|zoom|rename|copy|paste|save|print|show me|bring up|find)\b",
    re.IGNORECASE,
)


def route(text: str, claude_ready: bool = True) -> tuple[str, str]:
    """Whether an owner message goes straight to the local AI or to Claude first.

    With Claude signed out or missing everything goes to the local AI, which works on
    its own (D-067, D-093).
    """
    stripped = text.strip()
    if not stripped:
        return "claude", "empty message"
    if not claude_ready:
        return "operator", "Claude isn't available, so the local AI does it"
    if instant_plan(stripped) is not None:
        return "operator", "a single move on the screen"
    if stripped.endswith("?"):
        return "claude", "a question"
    if THINK.search(stripped):
        return "claude", "needs thinking or writing first"
    if SCREEN.search(stripped):
        return "operator", "work on the screen"
    return "claude", "not a screen task"


class Runner(Protocol):
    """`LocalOperator.run` with the hooks the service needs."""

    def __call__(
        self,
        task: str,
        on_event: Write,
        inbox: Callable[[], list[str]],
        cancelled: Callable[[], bool],
    ) -> tuple[str, Outcome]: ...


@dataclass
class Running:
    id: str
    text: str
    source: str
    write: Write
    notes: list[str] = field(default_factory=list)
    stop: threading.Event = field(default_factory=threading.Event)
    lock: threading.Lock = field(default_factory=threading.Lock)

    def take_notes(self) -> list[str]:
        with self.lock:
            fresh, self.notes = self.notes, []
        return fresh


class Awareness:
    """What the owner is doing, from the window list: kept in memory only (D-086).

    The window in front can be one the list leaves out (a console host, a tool
    window) or Relay itself; then the topmost other window stands in for it. The
    browser is the topmost browser window wherever it is in the stack, so its
    address is known while another program is in front (D-092).
    """

    def __init__(self, hands: Hands, clock: Callable[[], float] = time.time) -> None:
        self.hands = hands
        self.clock = clock
        self.recent: deque[dict[str, Any]] = deque(maxlen=RECENT)
        self.windows: list[str] = []
        self.foreground = ""
        self.polled: float | None = None
        self._lock = threading.Lock()

    def poll(self) -> None:
        try:
            listing = self.hands.run("list_windows", {}).text or ""
        except Exception:
            return
        lines = [line for line in listing.splitlines()[1:] if " · " in line]
        others = [line for line in lines if "Relay itself" not in line]
        front = next((line for line in others if "in front" in line.split(" · ")), "")
        relay_in_front = any("in front" in line.split(" · ") for line in lines)
        if not front and others and not (relay_in_front and self.foreground):
            front = others[0]  # the real front window is not listed, or it is Relay
        with self._lock:
            self.windows = lines
            self.polled = self.clock()
            if front and _title(front) != _title(self.foreground):
                self.recent.append({"at": round(self.clock()), "window": _title(front)})
            if front:
                self.foreground = front

    def browser(self) -> str:
        """The topmost open browser window's line, in front or not."""
        with self._lock:
            lines = list(self.windows)
        return next((line for line in lines if _exe(line) in BROWSERS), "")

    def browser_url(self) -> str | None:
        """The address in the topmost browser window, read from its address bar through UIA."""
        line = self.browser()
        if not line:
            return None
        params = {"query": ADDRESS_BAR, "window": _title(line)}
        try:
            found = self.hands.run("uia_find", params).text or ""
        except Exception:
            return None
        match = re.search(r"value='([^']*)'", found) or re.search(r'value="([^"]*)"', found)
        return match.group(1) if match else None

    def snapshot(self) -> dict[str, Any]:
        if self.polled is None or self.clock() - self.polled >= POLL_SECONDS:
            self.poll()  # asked before the first poll, or the poller fell behind
        url = self.browser_url()
        browser = self.browser()
        with self._lock:
            return {
                "foreground": _title(self.foreground),
                "windows": [_title(w) for w in self.windows[:15]],
                "recent": list(self.recent),
                "browser": _title(browser) if browser else None,
                "browser_url": url,
            }

    def start(self, stopping: threading.Event) -> None:
        def loop() -> None:
            while not stopping.wait(POLL_SECONDS):
                self.poll()

        threading.Thread(target=loop, name="relay-awareness", daemon=True).start()


def _exe(line: str) -> str:
    parts = line.split(" · ")
    return parts[1].casefold().removesuffix(".exe") if len(parts) > 1 else ""


def _title(line: str) -> str:
    head = line.split(" · ", 1)[0].strip()
    try:
        title = json.loads(head)
    except ValueError:
        return head.strip('"')
    return title if isinstance(title, str) else head


def claude_exe() -> str | None:
    if found := os.environ.get("RELAY_CLAUDE"):
        return found
    if found := shutil.which("claude"):
        return found
    home = Path.home() / ".local" / "bin" / ("claude.exe" if sys.platform == "win32" else "claude")
    return str(home) if home.is_file() else None


NO_TOOLS = (
    "Bash,PowerShell,Edit,Write,MultiEdit,NotebookEdit,Read,Glob,Grep,WebFetch,WebSearch,Task"
)


def ask_claude(question: str, task: str, run: Callable[..., Any] = subprocess.run) -> str:
    """One question to Claude through the owner's `claude` CLI, text only, no tools (D-002)."""
    exe = claude_exe()
    if exe is None:
        raise RuntimeError("Claude Code is not installed")
    prompt = (
        "Relay's local AI is doing a task on the owner's PC and needs your judgment. "
        f"The task: {task}\nIts question: {question}\n"
        "Answer with exactly what it should use (the text to type, or the choice), "
        "with no preamble."
    )
    flags = 0x08000000 if sys.platform == "win32" else 0  # CREATE_NO_WINDOW
    done = run(
        [
            exe,
            "-p",
            "--output-format",
            "text",
            "--strict-mcp-config",
            "--mcp-config",
            '{"mcpServers":{}}',
            "--disallowedTools",
            NO_TOOLS,
        ],
        input=prompt.encode("utf-8"),
        capture_output=True,
        timeout=ASK_TIMEOUT,
        creationflags=flags,
        check=False,
    )
    answer = done.stdout.decode("utf-8", errors="replace").strip()
    if done.returncode != 0 or not answer:
        error = done.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(error[:300] or "Claude gave no answer")
    return answer[:ANSWER_LIMIT]


class OperatorService:
    """Runs one task at a time (there is one mouse) and answers the other ops at once."""

    def __init__(
        self,
        runner: Runner,
        awareness: Awareness | None,
        write: Write,
        rearm: Callable[[], None] | None = None,
        grant: Callable[[str | None], None] | None = None,
    ) -> None:
        self.runner = runner
        # The owner's own task text approves the risky steps it names (D-091); Claude's
        # hand-offs are Claude's words, so they never grant anything.
        self.grant = grant or (lambda text: None)
        # The kill switch stops the task in progress. The service lives for the whole
        # session, so a new task the owner sends from the app re-arms it; Claude's
        # hand-offs never do (D-086).
        self.rearm = rearm or (lambda: None)
        self.awareness = awareness
        self.write = write
        self.running: Running | None = None
        self.last: tuple[str, Outcome] | None = None  # (task, outcome) for follow-ups
        self.stopping = threading.Event()
        self.claude_ok = True  # false after Claude failed to answer (signed out, missing)
        self._lock = threading.Lock()
        self._busy = threading.Lock()
        self._ids = 0

    # -- ops ------------------------------------------------------------------

    def handle(
        self, command: dict[str, Any], write: Write | None = None
    ) -> threading.Thread | None:
        """Act on one command; a task runs on its own thread, which is returned."""
        out = write or self.write
        op = command.get("op")
        cid = str(command.get("id") or self._new_id())
        text = str(command.get("text") or "").strip()
        if op == "task":
            return self.start(cid, text, out, str(command.get("source") or "app"))
        if op == "say":
            return self.say(cid, text, out)
        if op == "stop":
            with self._lock:
                running = self.running
            if running is not None and (not command.get("id") or running.id == cid):
                running.stop.set()
            return None
        if op == "route":
            ready = command.get("claude") is not False and self.claude_ok
            to, why = route(text, ready)
            out({"id": cid, "event": "route", "to": to, "why": why})
            return None
        if op == "state":
            snapshot = self.awareness.snapshot() if self.awareness else {}
            out({"id": cid, "event": "state", **snapshot})
            return None
        if op == "quit":
            self.stopping.set()
            with self._lock:
                if self.running is not None:
                    self.running.stop.set()
            return None
        out({"id": cid, "event": "error", "message": f"unknown op {op!r}"})
        return None

    def start(self, cid: str, text: str, write: Write, source: str) -> threading.Thread | None:
        if not text:
            write({"id": cid, "event": "failed", "message": "The task is empty.", "steps": 0})
            return None
        with self._lock:
            busy = self.running is not None
            if not busy:
                self.running = Running(cid, text, source, write)
        if busy:
            message = "Relay is already doing a task. Stop it first, or add to it."
            write({"id": cid, "event": "failed", "message": message, "steps": 0})
            return None
        if source == "app":
            self.rearm()
        thread = threading.Thread(target=self._run, name=f"relay-task-{cid}", daemon=True)
        thread.start()
        return thread

    def say(self, cid: str, text: str, write: Write) -> threading.Thread | None:
        """A follow-up: into the running task, or a new task that knows the last one."""
        if not text:
            return None
        with self._lock:
            running = self.running
            if running is not None:
                with running.lock:
                    running.notes.append(text)
        if running is not None:
            write({"id": running.id, "event": "note", "text": f"Got it: {text}"})
            return None
        task = text
        if self.last is not None:
            before, outcome = self.last
            task = (
                f"{text}\n(This follows the owner's last task: {before!r}, which ended "
                f"{outcome.status}: {outcome.message})"
            )
        return self.start(cid, task, write, "app")

    # -- the task thread --------------------------------------------------------

    def _run(self) -> None:
        running = self.running
        assert running is not None
        out = running.write

        def emit(event: dict[str, Any]) -> None:
            if event.get("event") == "unavailable":
                return
            line = {"id": running.id, **event}
            if running.source != "app":
                line["source"] = running.source
            out(line)
            if out is not self.write:  # the app sees Claude's ui_task steps too
                self.write(line)

        emit({"event": "accepted", "text": running.text})
        self.grant(running.text if running.source == "app" else None)
        try:
            with self._busy:
                model_id, outcome = self.runner(
                    running.text, emit, running.take_notes, running.stop.is_set
                )
            self.last = (running.text, outcome)
            emit({"event": "model", "model": model_id})
        except RuntimeUnavailable as exc:
            emit({"event": "note", "text": str(exc)})
            line = {"id": running.id, "event": "unavailable", "message": str(exc)}
            out(line)
            if out is not self.write:
                self.write(line)
        except Exception as exc:
            message = f"The local AI failed: {exc.__class__.__name__}: {exc}"
            emit({"event": "failed", "message": message, "steps": 0})
        finally:
            self.grant(None)
            with self._lock:
                self.running = None

    def _new_id(self) -> str:
        self._ids += 1
        return f"o{self._ids}"


# -- the localhost port ui_task uses -------------------------------------------


def record_path(home: Path) -> Path:
    return home / "operator.json"


def serve_port(service: OperatorService, home: Path) -> socket.socket:
    """Listen on 127.0.0.1 for ui_task hand-offs; the token is in operator.json."""
    token = secrets.token_hex(16)
    sock = socket.socket()
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", 0))
    sock.listen(4)
    port = sock.getsockname()[1]
    path = record_path(home)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({"port": port, "token": token, "pid": os.getpid()}), "utf-8")

    def client(conn: socket.socket) -> None:
        with conn, conn.makefile("rwb") as stream:
            lock = threading.Lock()

            def send(event: dict[str, Any]) -> None:
                with lock, contextlib.suppress(OSError):
                    stream.write((json.dumps(event) + "\n").encode("utf-8"))
                    stream.flush()

            line = stream.readline()
            try:
                command = json.loads(line)
            except ValueError:
                return
            if not isinstance(command, dict) or command.get("token") != token:
                send({"event": "error", "message": "bad token"})
                return
            command.setdefault("source", "claude")
            thread = service.handle(command, send)
            if thread is not None:
                thread.join()

    def accept() -> None:
        while not service.stopping.is_set():
            try:
                conn, _ = sock.accept()
            except OSError:
                return
            threading.Thread(target=client, args=(conn,), daemon=True).start()

    threading.Thread(target=accept, name="relay-operator-port", daemon=True).start()
    return sock


def hand_off(
    home: Path, task: str, max_steps: int, timeout: float = 1800.0
) -> tuple[str, Outcome, list[str]] | None:
    """Give `task` to the running operator service; None when there is none.

    Returns the model id, the outcome and the steps in plain words.
    """
    try:
        info = json.loads(record_path(home).read_text("utf-8"))
        port, token = int(info["port"]), str(info["token"])
    except (OSError, ValueError, KeyError, TypeError):
        return None
    try:
        conn = socket.create_connection(("127.0.0.1", port), timeout=2)
    except OSError:
        return None
    conn.settimeout(timeout)
    model = "local"
    steps: list[str] = []
    with conn, conn.makefile("rwb") as stream:
        command = {"token": token, "op": "task", "text": task, "max_steps": max_steps}
        stream.write((json.dumps(command) + "\n").encode("utf-8"))
        stream.flush()
        for raw in stream:
            try:
                event = json.loads(raw)
            except ValueError:
                continue
            kind = event.get("event")
            if kind == "step":
                steps.append(f"Step {event.get('step')}: {event.get('say') or event.get('action')}")
            elif kind == "model":
                model = str(event.get("model"))
            elif kind in ("done", "failed", "stopped"):
                status: Status = "done" if kind == "done" else kind
                outcome = Outcome(status, str(event.get("message") or ""))
                model = _model_line(stream) or model  # it follows the outcome
                return model, outcome, steps
            elif kind == "unavailable":
                raise RuntimeUnavailable(str(event.get("message")))
            elif kind == "error":
                return None
    return None


def _model_line(stream: Any) -> str | None:
    for raw in stream:
        with contextlib.suppress(ValueError, AttributeError):
            event = json.loads(raw)
            if event.get("event") == "model":
                return str(event.get("model"))
    return None


# -- main ---------------------------------------------------------------------


def main(argv: list[str]) -> int:
    del argv
    from relay_screen.agent.operator import LocalOperator
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    from relay_screen.server import default_services

    services = default_services()
    hands = services.computer()
    lock = threading.Lock()

    def write(event: dict[str, Any]) -> None:
        with lock:
            sys.stdout.write(json.dumps(event, ensure_ascii=False) + "\n")
            sys.stdout.flush()

    operator = LocalOperator()

    def runner(
        task: str,
        on_event: Write,
        inbox: Callable[[], list[str]],
        cancelled: Callable[[], bool],
    ) -> tuple[str, Outcome]:
        return operator.run(
            task,
            hands,
            MAX_STEPS,
            on_event,
            inbox=inbox,
            cancelled=cancelled,
            ask=lambda question: ask(question, task),
        )

    def ask(question: str, task: str) -> str:
        try:
            answer = ask_claude(question, task)
        except Exception:
            service.claude_ok = False  # route the next messages to the local AI
            raise
        service.claude_ok = True
        return answer

    awareness = Awareness(hands)
    switch = services.killswitch
    gate = services.gate
    service = OperatorService(
        runner,
        awareness,
        write,
        switch.rearm if switch else None,
        gate.grant if gate else None,
    )
    awareness.poll()
    awareness.start(service.stopping)
    sock = serve_port(service, operator.paths.home)
    # Load the model in the background so the first task doesn't wait for it, while
    # route and state answer at once.
    threading.Thread(target=operator.warm, name="relay-warm", daemon=True).start()
    write({"event": "ready"})
    try:
        for raw in sys.stdin:
            try:
                command = json.loads(raw)
            except ValueError:
                continue
            if isinstance(command, dict):
                service.handle(command)
                if service.stopping.is_set():
                    break
    finally:
        service.stopping.set()
        with contextlib.suppress(OSError):
            sock.close()
        with contextlib.suppress(OSError):
            record_path(operator.paths.home).unlink()
        operator.supervisor.stop_all()  # Relay is closing: free the VRAM
    return 0
