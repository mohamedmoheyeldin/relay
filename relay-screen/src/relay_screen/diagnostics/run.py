"""Full test runs (D-096): real tasks on the owner's PC, with everything recorded.

    relay-screen diagnostics [--only ID ...] [--skip ID ...] [--out DIR] [--json] [--list]

Each run drives the same local operator the app uses (the running operator service when
Relay is open, else one started in this process) through real tasks: Notepad, File
Explorer, a browser tab and web search, a web form served from this process, every
monitor, the router, a Cloud AI round trip, Cloud AI handing a task to the local AI, and the
local AI alone as if Cloud AI were signed out. Results are checked from outside the agent.

Everything goes in a timestamped folder under Downloads\\Relay-diagnostics: per-step
screenshots, every operator event with its time, timings, the Cloud AI transcript, the
runtime and audit log lines written during the run, an environment snapshot,
summary.json (for the app and for Cloud AI) and report.html (for the owner). latest.json
next to the folders names the newest run.

This opens windows and moves the mouse, so it only runs when the owner asks (D-049).
"""

from __future__ import annotations

import contextlib
import html
import http.server
import io
import json
import os
import platform
import queue
import re
import socket
import subprocess
import sys
import threading
import time
import urllib.parse
from collections.abc import Callable, Mapping, Sequence
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Literal, Protocol

from relay_screen import __version__, cli_names
from relay_screen.agent.bench import Desktop
from relay_screen.agent.local import Hands
from relay_screen.agent.service import NO_TOOLS, record_path, route
from relay_screen.diagnostics import app as app_tests

SCHEMA = 1
FINAL = ("done", "failed", "stopped", "unavailable")
ANSWER = "Use your default choice. This is Relay's diagnostics run and nobody is watching."
CLOUD_TIMEOUT = 420.0
TASK_TIMEOUT = 900.0
EXPLORER_TEST = "relay-explorer-test"  # a folder the other-monitor scenario opens
SHOT_WIDTH = 1600  # recorded screenshots are scaled down to this width
LOG_TAIL = 4_000_000  # bytes of a log copied into the run at most

Kind = Literal["operator", "displays", "route", "cloud-text", "cloud-ui", "app"]
Event = Callable[[dict[str, Any]], None]


@dataclass(frozen=True)
class Scenario:
    id: str
    title: str
    kind: Kind
    task: str = ""
    checks: tuple[dict[str, Any], ...] = ()
    offline: bool = False  # the local AI may not ask Cloud AI (as if signed out)
    max_steps: int = 20
    close_tab: str = ""  # a browser tab whose title has this is closed afterwards
    answer: str = ""  # the owner's answer if the operator asks (else ANSWER)
    setup: str = ""  # something to prepare first, see Tools.setup
    interfere: bool = False  # the "owner" moves the mouse and types after step 1
    cleanup_files: tuple[str, ...] = ()


# The full run's areas, in the order they run and are shown (D-101).
AREAS = ("Relay app", "Real-world tasks", "Cloud AI and signed out", "System")


def area_of(s: Scenario) -> str:
    if s.kind == "app":
        return AREAS[0]
    if s.kind.startswith("cloud") or s.offline:
        return AREAS[2]
    if s.kind in ("displays", "route"):
        return AREAS[3]
    return AREAS[1]


def scenarios(stamp: str, site: str) -> list[Scenario]:
    """Everything, area by area: the app, real tasks, Cloud AI, then the system checks."""
    return sorted(_scenarios(stamp, site), key=lambda s: AREAS.index(area_of(s)))


def _scenarios(stamp: str, site: str) -> list[Scenario]:
    """Every scenario. `stamp` makes the typed text unique to this run."""
    note = f"Relay diagnostics {stamp}"
    query = f"relay diagnostics {stamp}"
    cloud_note = f"Cloud AI sent this {stamp}"
    form_url = f"{site}/form"
    saved = f"relay-diag-{stamp}.txt"
    return [
        Scenario("displays", "Every monitor: screenshot and pointer", "displays"),
        Scenario("route", "Routing: local AI or Cloud AI", "route"),
        Scenario(
            "notepad-instant",
            "Open Notepad (instant, no model)",
            "operator",
            "open Notepad",
            ({"front_window": "Notepad"},),
            max_steps=5,
        ),
        Scenario(
            "notepad-type",
            "Notepad: open it and type",
            "operator",
            f"Open Notepad and type: {note}",
            ({"uia_text": {"window": "Notepad", "contains": note}},),
            max_steps=12,
        ),
        Scenario(
            "explorer-downloads",
            "File Explorer: show Downloads",
            "operator",
            "Open File Explorer and show the Downloads folder.",
            ({"front_window": "Downloads"},),
            max_steps=10,
        ),
        Scenario(
            "browser-search",
            "Browser: new tab and web search",
            "operator",
            f"Open a new tab in the web browser and search the web for {query}",
            ({"window_title": query},),
            max_steps=12,
            close_tab=query,
        ),
        Scenario(
            "web-form",
            "Browser: fill in and submit a web form",
            "operator",
            f"Go to {form_url} and fill in the form: type Relay Test in the Name box, "
            "choose Blue as the colour, tick the I agree box, then press Submit.",
            ({"form": {"name": "Relay Test", "colour": "blue", "agree": "on"}},),
            max_steps=20,
            close_tab="Relay form",
        ),
        Scenario(
            "local-only",
            "Local AI alone (Cloud AI signed out)",
            "operator",
            "Open Calculator and work out 12 times 34.",
            ({"uia_text": {"window": "Calculator", "contains": "Display is 408"}},),
            offline=True,
            max_steps=15,
        ),
        # Harder, closer to real use: several pages, apps and decisions in one task.
        Scenario(
            "job-application",
            "Browser: a three-page job application",
            "operator",
            f"Go to {site}/job and apply. Full name: Alex Relay. Email: alex.relay@example.com. "
            "Phone: 555 0100. Then press Next. On the next page choose Backend developer as "
            "the role, 6 years of experience, Remote work style, and for Why this job write: "
            "I like reliable tools. Press Next, tick the box to confirm the details, and "
            "submit the application.",
            (
                {
                    "form": {
                        "full_name": "Alex Relay",
                        "email": "alex.relay@example.com",
                        "phone": "555 0100",
                        "role": "backend",
                        "years": "6",
                        "style": "remote",
                        "why": "I like reliable tools",
                        "confirm": "on",
                    },
                    "path": "/applied",
                },
            ),
            max_steps=35,
            close_tab="Application",
        ),
        Scenario(
            "compare-and-copy",
            "Browser to Notepad: find the cheapest and write it down",
            "operator",
            f"Go to {site}/prices, find the cheapest laptop there, then open Notepad and "
            "write down its name and price.",
            (
                {"uia_text": {"window": "Notepad", "contains": "Nimbus 14"}},
                {"uia_text": {"window": "Notepad", "contains": "749"}},
            ),
            max_steps=20,
            close_tab="Laptop prices",
        ),
        Scenario(
            "save-where",
            "Notepad: save a file when the place isn't given",
            "operator",
            f"Open Notepad, type Relay saved this {stamp}, and save it as {saved}",
            ({"file_contains": {"path": f"~/Downloads/{saved}", "text": f"saved this {stamp}"}},),
            max_steps=20,
            answer="Save it in my Downloads folder.",
            cleanup_files=(f"~/Downloads/{saved}", f"~/Documents/{saved}", f"~/Desktop/{saved}"),
        ),
        Scenario(
            "other-monitor",
            "A window on another monitor",
            "operator",
            f"In the File Explorer window that shows the {EXPLORER_TEST} folder, open the "
            "incoming folder in it.",
            ({"front_window": "incoming"},),
            max_steps=12,
            setup="explorer-other-display",
        ),
        Scenario(
            "owner-interrupts",
            "The owner moves the mouse and types mid-task",
            "operator",
            f"Open Notepad and type: First line {stamp}. Then press Enter and type: "
            f"Second line {stamp}",
            (
                {"uia_text": {"window": "Notepad", "contains": f"First line {stamp}"}},
                {"uia_text": {"window": "Notepad", "contains": f"Second line {stamp}"}},
            ),
            max_steps=20,
            interfere=True,
        ),
        # Never tuned on: a program and a site no fix was made for, to show the local AI
        # carries over to new work rather than learning the other scenarios (D-109).
        Scenario(
            "settings-about",
            "New program: Windows Settings, find the About page",
            "operator",
            "Open Windows Settings and show the About page for this PC.",
            ({"uia_text": {"window": "Settings", "contains": "Product ID"}},),
            max_steps=15,
        ),
        Scenario(
            "library-reserve",
            "New site: search a catalogue and pick the right copy",
            "operator",
            f"Go to {site}/library, search the catalogue for Dune and reserve the paperback "
            "copy of Dune.",
            ({"form": {"book": "dune-paperback"}, "path": "/reserved"},),
            max_steps=20,
            close_tab="library",
        ),
        Scenario("cloud-text", "Cloud AI round trip", "cloud-text"),
        Scenario(
            "cloud-ui",
            "Cloud AI hands a task to the local AI",
            "cloud-ui",
            f"Open Notepad and type: {cloud_note}",
            ({"uia_text": {"window": "Notepad", "contains": cloud_note}},),
        ),
        Scenario("app-navigation", "Relay app: sidebar, Back and Forward", "app"),
        Scenario("app-settings", "Relay app: every Settings page and toggle", "app"),
        Scenario("app-composer", "Relay app: composer, keys, long message, model picker", "app"),
        Scenario("app-chats", "Relay app: chats survive a restart (pin, rename, delete)", "app"),
        Scenario("app-attach", "Relay app: attach opens a file picker", "app"),
        Scenario("app-stop", "Relay app: Stop ends an answer", "app"),
        Scenario("app-small-window", "Relay app: a smaller window", "app"),
    ]


# -- what a scenario needs ------------------------------------------------------


class OperatorLink(Protocol):
    """Runs one task on the local operator and streams its events."""

    name: str

    def run(
        self, task: str, on_event: Event, *, offline: bool, max_steps: int, answer: str = ANSWER
    ) -> None: ...


class CloudLink(Protocol):
    def status(self) -> dict[str, Any]: ...

    def ask(self, question: str) -> str: ...

    def run_task(self, prompt: str, folder: Path, on_line: Event) -> dict[str, Any]: ...


@dataclass
class Tools:
    hands: Hands
    desktop: Desktop
    link: OperatorLink
    cloud: CloudLink | None
    capture: Callable[[], bytes | None] = lambda: None
    displays: Callable[[], list[dict[str, Any]]] = lambda: []
    select_display: Callable[[int], object] = lambda display_id: None
    active_display: Callable[[], int] = lambda: 0
    form: FormServer | None = None
    # Prepares a scenario's `setup`; returns why it can't run here, or None.
    setup: Callable[[Scenario], str | None] = lambda s: None
    # Stands in for the owner using the PC mid-task; returns what it did.
    interfere: Callable[[], str] = lambda: "nothing"
    clock: Callable[[], float] = time.monotonic
    sleep: Callable[[float], None] = time.sleep
    app: app_tests.AppUi | None = None  # Relay's own window, for the app-function tests
    settings_file: Path | None = None  # %APPDATA%\\Relay\\settings.json
    stamp: str = ""


@dataclass
class Result:
    id: str
    title: str
    kind: str
    status: str  # passed, failed, skipped
    message: str
    seconds: float
    steps: int = 0
    model: str | None = None
    failed_checks: list[str] = field(default_factory=list)
    timings: dict[str, Any] = field(default_factory=dict)
    details: dict[str, Any] = field(default_factory=dict)
    screenshots: list[str] = field(default_factory=list)
    folder: str = ""
    area: str = ""

    @property
    def passed(self) -> bool:
        return self.status == "passed"


class Recorder:
    """One scenario's folder: events.jsonl with times, screenshots, result.json."""

    def __init__(self, folder: Path, capture: Callable[[], bytes | None], clock: Callable) -> None:
        self.folder = folder
        folder.mkdir(parents=True, exist_ok=True)
        self.capture = capture
        self.clock = clock
        self.start = clock()
        self.shots: list[str] = []
        self._events = (folder / "events.jsonl").open("a", encoding="utf-8")
        self._lock = threading.Lock()

    def elapsed(self) -> float:
        return round(self.clock() - self.start, 3)

    def event(self, event: Mapping[str, Any]) -> dict[str, Any]:
        line = {"t": self.elapsed(), **event}
        with self._lock:
            self._events.write(json.dumps(line, ensure_ascii=False) + "\n")
            self._events.flush()
        return line

    def shot(self, name: str) -> str | None:
        try:
            data = self.capture()
        except Exception as exc:
            self.event({"event": "screenshot_failed", "name": name, "error": str(exc)})
            return None
        if not data:
            return None
        file = f"{name}.jpg"
        (self.folder / file).write_bytes(data)
        self.shots.append(file)
        return file

    def save(self, file: str, data: bytes) -> None:
        (self.folder / file).write_bytes(data)
        self.shots.append(file)

    def close(self) -> None:
        with self._lock:
            self._events.close()


# -- the web form ---------------------------------------------------------------

FORM_PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Relay form</title>
<style>body{font:18px system-ui;margin:48px}label{display:block;margin:18px 0}
input,select,button{font:inherit;padding:6px}</style></head>
<body><h1>Relay form</h1>
<form action="/sent" method="get">
<label>Name <input name="name" id="name" aria-label="Name"></label>
<label>Colour <select name="colour" aria-label="Colour">
<option value="">Choose...</option><option value="red">Red</option>
<option value="green">Green</option><option value="blue">Blue</option></select></label>
<label><input type="checkbox" name="agree"> I agree</label>
<button type="submit">Submit</button>
</form></body></html>"""


JOB_STYLE = (
    "<style>body{font:18px system-ui;margin:48px;max-width:720px}label{display:block;"
    "margin:16px 0}input,select,textarea,button{font:inherit;padding:6px}"
    "textarea{width:100%}</style>"
)


def _page(title: str, body: str) -> str:
    return (
        f'<!doctype html><html><head><meta charset="utf-8"><title>{html.escape(title)}</title>'
        f"{JOB_STYLE}</head><body><h1>{html.escape(title)}</h1>{body}</body></html>"
    )


def _hidden(fields: Mapping[str, str]) -> str:
    return "".join(
        f'<input type="hidden" name="{html.escape(k)}" value="{html.escape(v)}">'
        for k, v in fields.items()
    )


PRICES_PAGE = _page(
    "Laptop prices",
    "<p>Three laptops, all in stock.</p><table border=1 cellpadding=8>"
    "<tr><th>Laptop</th><th>Price</th></tr>"
    "<tr><td>Orion 15</td><td>$1,049</td></tr>"
    "<tr><td>Nimbus 14</td><td>$749</td></tr>"
    "<tr><td>Vega 13</td><td>$899</td></tr></table>",
)


def site_page(path: str, fields: Mapping[str, str]) -> str:
    """The test site's pages: a one-page form, a three-page job application and a
    price list. Each page passes on what was entered before in hidden fields."""
    if path == "/sent":
        shown = html.escape(fields.get("name", ""))
        return f"<title>Relay form sent: {shown}</title><h1>Sent: {shown}</h1>"
    if path == "/prices":
        return PRICES_PAGE
    if path == "/job":
        return _page(
            "Job application - step 1 of 3: about you",
            '<form action="/job2" method="get">'
            '<label>Full name <input name="full_name" aria-label="Full name"></label>'
            '<label>Email <input name="email" type="email" aria-label="Email"></label>'
            '<label>Phone <input name="phone" aria-label="Phone"></label>'
            '<button type="submit">Next</button></form>',
        )
    if path == "/job2":
        return _page(
            "Job application - step 2 of 3: experience",
            f'<form action="/job3" method="get">{_hidden(fields)}'
            '<label>Role <select name="role" aria-label="Role"><option value="">Choose...</option>'
            '<option value="frontend">Frontend developer</option>'
            '<option value="backend">Backend developer</option>'
            '<option value="data">Data analyst</option></select></label>'
            '<label>Years of experience <input name="years" type="number" '
            'aria-label="Years of experience"></label>'
            "<fieldset><legend>Work style</legend>"
            '<label><input type="radio" name="style" value="remote"> Remote</label>'
            '<label><input type="radio" name="style" value="office"> Office</label></fieldset>'
            '<label>Why this job? <textarea name="why" rows="3" aria-label="Why this job?">'
            "</textarea></label>"
            '<button type="submit">Next</button></form>',
        )
    if path == "/job3":
        rows = "".join(
            f"<tr><td>{html.escape(k)}</td><td>{html.escape(v)}</td></tr>"
            for k, v in fields.items()
        )
        return _page(
            "Job application - step 3 of 3: review",
            f'<table border=1 cellpadding=6>{rows}</table><form action="/applied" method="get">'
            f'{_hidden(fields)}<label><input type="checkbox" name="confirm"> '
            "I confirm these details are correct</label>"
            '<button type="submit">Submit application</button></form>',
        )
    if path == "/applied":
        return _page("Application received", "<p>Thank you.</p>")
    if path == "/library":
        return library_page(fields.get("q", ""))
    if path == "/reserved":
        book = next((b for b in BOOKS if b[0] == fields.get("book")), None)
        return _page("City library: reserved", f"<p>Reserved: {book[1] if book else 'nothing'}</p>")
    return FORM_PAGE


BOOKS = (
    ("dune-hardback", "Dune", "Hardback", "Frank Herbert"),
    ("messiah-paperback", "Dune Messiah", "Paperback", "Frank Herbert"),
    ("dune-paperback", "Dune", "Paperback", "Frank Herbert"),
    ("foundation-paperback", "Foundation", "Paperback", "Isaac Asimov"),
)


def library_page(query: str) -> str:
    """A catalogue: a search box, then one Reserve button per matching copy."""
    found = [b for b in BOOKS if query and query.casefold() in b[1].casefold()]
    rows = "".join(
        f"<tr><td>{title}</td><td>{kind}</td><td>{author}</td><td>"
        f'<form action="/reserved" method="get"><input type="hidden" name="book" '
        f'value="{book}"><button type="submit">Reserve</button></form></td></tr>'
        for book, title, kind, author in found
    )
    table = (
        f"<p>{len(found)} copies found.</p><table border=1 cellpadding=6>"
        f"<tr><th>Title</th><th>Format</th><th>Author</th><th></th></tr>{rows}</table>"
        if query
        else ""
    )
    return _page(
        "City library catalogue",
        '<form action="/library" method="get"><label>Search the catalogue '
        f'<input name="q" aria-label="Search the catalogue" value="{html.escape(query)}">'
        f'</label><button type="submit">Search</button></form>{table}',
    )


def _same(got: str, want: str) -> bool:
    """Equal, ignoring case, spaces at the ends and a final full stop."""
    return got.strip().rstrip(".").casefold() == want.strip().rstrip(".").casefold()


class FormServer:
    """The test site on 127.0.0.1; what its forms receive is checked from outside."""

    def __init__(self) -> None:
        self.received: list[tuple[str, dict[str, str]]] = []
        server = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                url = urllib.parse.urlparse(self.path)
                fields = {k: v[-1] for k, v in urllib.parse.parse_qs(url.query).items()}
                if url.path in ("/sent", "/job2", "/job3", "/applied", "/reserved"):
                    server.received.append((url.path, fields))
                data = site_page(url.path, fields).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, format: str, *args: Any) -> None:
                del format, args

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.base = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        self.url = f"{self.base}/form"
        threading.Thread(target=self.httpd.serve_forever, name="relay-form", daemon=True).start()

    def check(self, want: Mapping[str, str], path: str = "/sent") -> str | None:
        got_here = [f for p, f in self.received if p == path]
        for got in reversed(got_here):
            if all(_same(got.get(k, ""), v) for k, v in want.items()):
                return None
        if not got_here:
            pages = sorted({p for p, _ in self.received})
            reached = f" (it reached {', '.join(pages)})" if pages else ""
            return f"the form was never submitted to {path}{reached}"
        return f"{path} got {got_here[-1]}, expected {dict(want)}"

    def close(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()


# -- running scenarios ------------------------------------------------------------


def check(tools: Tools, spec: Mapping[str, Any]) -> str | None:
    """None if the check passes, else what was wrong."""
    if "form" in spec:
        if tools.form is None:
            return "no form server"
        return tools.form.check(spec["form"], str(spec.get("path", "/sent")))
    if "front_window" in spec:
        return front_window(tools.hands, str(spec["front_window"]))
    return tools.desktop.check(spec)


def front_window(hands: Hands, want: str) -> str | None:
    """The window in front has `want` in its title: the task brought it forward, which
    a window left open from before can't fake."""
    try:
        listing = hands.run("list_windows", {}).text or ""
    except Exception as exc:
        return f"could not list windows: {exc}"
    front = next((x for x in listing.splitlines() if "in front" in x.split(" · ")), "")
    if want.lower() in front.split(" · ", 1)[0].lower():
        return None
    title = front.split(" · ", 1)[0].strip() or "nothing"
    return f"{title} is in front, not a window titled like {want!r}"


def progress(s: Scenario, event: Mapping[str, Any]) -> dict[str, Any]:
    """An operator or Cloud AI event as the run reports it: `kind` is the event's own name."""
    return {**event, "event": "progress", "scenario": s.id, "kind": event.get("event")}


def run_operator(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    before = set(tools.desktop.titles())
    remove_files(s)
    if s.setup:
        why = tools.setup(s)
        if why:
            cleanup(s, tools, before)
            return _skipped(s, why)
        rec.event({"event": "setup", "what": s.setup})
        tools.sleep(1.5)
    rec.shot("before")
    interfered = threading.Event()

    def interfere() -> None:
        try:
            done = tools.interfere()
        except Exception as exc:
            done = f"failed: {exc}"
        rec.event({"event": "interference", "what": done})

    final: dict[str, Any] = {}
    model: str | None = None
    steps = 0
    timings: list[dict[str, Any]] = []
    details: dict[str, Any] = {"task": s.task, "notes": [], "questions": [], "bad_replies": 0}
    first_step: float | None = None

    def on_event(event: dict[str, Any]) -> None:
        nonlocal model, steps, first_step
        line = rec.event(event)
        kind = event.get("event")
        if kind == "step":
            steps = max(steps, int(event.get("step") or 0))
            if first_step is None:
                first_step = line["t"]
            rec.shot(f"step-{steps:02d}")
            if s.interfere and not interfered.is_set():
                interfered.set()
                threading.Thread(target=interfere, name="relay-interfere", daemon=True).start()
        elif kind == "timing":
            timings.append({k: v for k, v in event.items() if k not in ("event", "id")})
        elif kind == "model":
            model = str(event.get("model"))
        elif kind == "plan":
            details["plan"] = {k: event.get(k) for k in ("goal", "app", "steps", "done_when")}
        elif kind == "note":
            details["notes"].append(str(event.get("text") or "")[:300])
        elif kind == "question":
            details["questions"].append(str(event.get("text") or ""))
        elif kind == "bad_reply":
            details["bad_replies"] += 1
        elif kind in FINAL:
            final.update(event)
            if "instant_ms" in event:
                details["instant_ms"] = event["instant_ms"]
        emit(progress(s, event))

    start = tools.clock()
    try:
        tools.link.run(
            s.task, on_event, offline=s.offline, max_steps=s.max_steps, answer=s.answer or ANSWER
        )
    except Exception as exc:
        final = {"event": "failed", "message": f"{type(exc).__name__}: {exc}"}
        rec.event({"event": "error", "message": final["message"]})
    seconds = round(tools.clock() - start, 2)
    tools.sleep(1.0)
    rec.shot("after")
    failed = [why for c in s.checks if (why := check(tools, c)) is not None]
    status_word = str(final.get("event") or "failed")
    message = str(final.get("message") or "The operator ended without a result.")
    if status_word == "unavailable":
        status, failed = "failed", [*failed, "the local AI could not run"]
    else:
        if status_word == "failed" and not failed:
            # The screen is right, but the operator never saw it was done (D-106).
            failed = ["the result is there, but the local AI never said it was done"]
        status = "passed" if not failed and status_word != "stopped" else "failed"
    cleanup(s, tools, before)
    remove_files(s)
    # The operator said done but the check from outside disagrees (D-098).
    details["false_done"] = status_word == "done" and bool(failed)
    if s.interfere:
        details["interfered"] = interfered.is_set()
    think = [t.get("think_ms", 0) for t in timings]
    return Result(
        s.id,
        s.title,
        s.kind,
        status,
        message,
        seconds,
        steps,
        model,
        failed,
        {
            "total_s": seconds,
            "first_step_s": first_step,
            "model_calls": len(timings),
            "think_ms_total": sum(think),
            "think_ms_max": max(think, default=0),
            "perceive_ms_total": sum(t.get("perceive_ms", 0) for t in timings),
            "act_ms_total": sum(t.get("act_ms", 0) for t in timings),
            "screenshots_sent": sum(1 for t in timings if t.get("screenshot_sent")),
            "per_step": timings,
        },
        {**details, "operator_status": status_word, "link": tools.link.name},
    )


def remove_files(s: Scenario) -> None:
    from relay_screen.agent.bench import expand

    for name in s.cleanup_files:
        with contextlib.suppress(OSError):
            expand(name).unlink(missing_ok=True)


def cleanup(s: Scenario, tools: Tools, before: set[int]) -> None:
    """Close what the scenario opened: new windows, and its browser tab."""
    if s.close_tab:
        titles = tools.desktop.titles()
        match = next((t for t in titles.values() if s.close_tab.lower() in t.lower()), None)
        if match:
            with contextlib.suppress(Exception):
                tools.hands.run("focus_window", {"window": match})
                tools.hands.run("key", {"text": "ctrl+w"})
    close_test_docs(tools)
    for handle in set(tools.desktop.titles()) - before:
        with contextlib.suppress(Exception):
            tools.desktop.close(handle)
    tools.sleep(0.5)


# Notepad tabs the scenarios write: their titles start with the test text (D-106).
TEST_DOC = re.compile(
    r"^\*?(?:(?:Relay diagnostics|First line|Cloud AI sent this|Relay saved this) \d{6}"
    r"|Nimbus 14\b|relay-diag-\d{6}\.txt)"
)
MAX_TEST_DOCS = 40


def close_test_docs(tools: Tools) -> int:
    """Close the Notepad tabs test runs wrote, without saving them.

    Closing a whole window left Notepad's "Save changes?" prompt open, and the next
    scenarios worked in those leftover tabs: four of round 9's six failures (D-106).
    """
    closed = 0
    tried: dict[str, int] = {}
    for _ in range(MAX_TEST_DOCS):
        titles = tools.desktop.titles().values()
        doc = next(
            (
                t
                for t in titles
                if t.endswith(" - Notepad") and TEST_DOC.match(t) and tried.get(t, 0) < 2
            ),
            None,
        )
        if doc is None:
            break
        tried[doc] = tried.get(doc, 0) + 1
        try:
            tools.hands.run("focus_window", {"window": doc})
            # A Save as dialog left by a failed save blocks the tab; Escape shuts it.
            tools.hands.run("key", {"text": "Escape"})
            tools.hands.run("key", {"text": "ctrl+w"})
            tools.sleep(0.6)
            found = tools.hands.run("uia_find", {"query": "Don", "role": "Button"}).text or ""
            button = re.search(r"^\[(e\d+)\] Button \"Don.t save", found, re.M)
            if button:
                tools.hands.run("uia_invoke", {"element": button.group(1)})
                tools.sleep(0.4)
        except Exception:
            break
        closed += 1
    return closed


CURSOR = re.compile(r"X=(-?\d+), Y=(-?\d+)")


def run_displays(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    """Every monitor: its screenshot has the reported size and the pointer lands where
    it is sent (screenshot pixels to physical and back, D-012)."""
    start = tools.clock()
    found = tools.displays()
    original = tools.active_display()
    rec.event({"event": "displays", "displays": found})
    failed: list[str] = []
    checked: list[dict[str, Any]] = []
    back: tuple[int, int] | None = None
    with contextlib.suppress(Exception):
        where = CURSOR.search(tools.hands.run("cursor_position").text or "")
        back = (int(where.group(1)), int(where.group(2))) if where else None
    for d in found:
        did = int(d["id"])
        entry: dict[str, Any] = {"id": did, "name": d.get("name")}
        try:
            tools.select_display(did)
            began = tools.clock()
            shot = tools.hands.run("screenshot")
            entry["screenshot_ms"] = round((tools.clock() - began) * 1000)
            want = d.get("screenshot_size")
            if shot.image is not None:
                size = _png_size(shot.image)
                entry["screenshot_size"] = size
                if want and size and list(size) != list(want):
                    failed.append(f"display {did}: screenshot {size}, expected {want}")
                rec.save(f"display-{did}.png", shot.image)
            else:
                failed.append(f"display {did}: the screenshot came back empty")
            if want:
                target = [int(want[0]) // 2, int(want[1]) // 2]
                tools.hands.run("mouse_move", {"coordinate": target})
                got = CURSOR.search(tools.hands.run("cursor_position").text or "")
                point = [int(got.group(1)), int(got.group(2))] if got else None
                entry["pointer"] = {"sent": target, "read_back": point}
                if point is None or abs(point[0] - target[0]) > 2 or abs(point[1] - target[1]) > 2:
                    failed.append(f"display {did}: pointer sent to {target}, read back {point}")
        except Exception as exc:
            failed.append(f"display {did}: {type(exc).__name__}: {exc}")
        checked.append(entry)
        rec.event({"event": "display_checked", **entry})
    with contextlib.suppress(Exception):
        tools.select_display(original)
        if back is not None:
            tools.hands.run("mouse_move", {"coordinate": list(back)})
    if not found:
        failed.append("no displays found")
    seconds = round(tools.clock() - start, 2)
    message = f"{len(found)} display(s) checked"
    emit(progress(s, {"event": "displays", "message": message}))
    return Result(
        s.id,
        s.title,
        s.kind,
        "passed" if not failed else "failed",
        message,
        seconds,
        failed_checks=failed,
        details={"displays": checked},
    )


ROUTES: tuple[tuple[str, bool, str], ...] = (
    ("open Notepad", True, "operator"),
    ("go to dice.com and search python jobs", True, "operator"),
    ("click the Save button", True, "operator"),
    ("write a cover letter for the job on screen", True, "cloud"),
    ("what is the capital of France?", True, "cloud"),
    ("summarize this page", True, "cloud"),
    ("write a cover letter for the job on screen", False, "operator"),
)


def run_route(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    del emit
    start = tools.clock()
    failed: list[str] = []
    rows: list[dict[str, Any]] = []
    for text, ready, want in ROUTES:
        began = time.perf_counter()
        to, why = route(text, ready)
        micros = round((time.perf_counter() - began) * 1_000_000)
        row = {"text": text, "cloud_ready": ready, "to": to, "why": why, "us": micros}
        rows.append(row)
        rec.event({"event": "route", **row})
        if to != want:
            failed.append(
                f"{text!r} (Cloud AI {'on' if ready else 'off'}) went to {to}, not {want}"
            )
    worst = max((r["us"] for r in rows), default=0)
    return Result(
        s.id,
        s.title,
        s.kind,
        "passed" if not failed else "failed",
        f"{len(rows) - len(failed)}/{len(rows)} routed as expected, slowest {worst} µs",
        round(tools.clock() - start, 2),
        failed_checks=failed,
        details={"routes": rows},
    )


def run_cloud_text(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    del emit
    if tools.cloud is None:
        return _skipped(s, "Cloud AI is not installed")
    start = tools.clock()
    status = tools.cloud.status()
    rec.event({"event": "cloud_status", **status})
    if not status.get("logged_in"):
        return _skipped(s, "Cloud AI is not signed in", details={"cloud": status})
    try:
        answer = tools.cloud.ask("Reply with the single word READY.")
    except Exception as exc:
        seconds = round(tools.clock() - start, 2)
        rec.event({"event": "error", "message": str(exc)})
        return Result(s.id, s.title, s.kind, "failed", str(exc)[:300], seconds)
    seconds = round(tools.clock() - start, 2)
    rec.event({"event": "answer", "text": answer[:500], "seconds": seconds})
    ok = "READY" in answer.upper()
    return Result(
        s.id,
        s.title,
        s.kind,
        "passed" if ok else "failed",
        f"Cloud AI answered in {seconds} s",
        seconds,
        failed_checks=[] if ok else [f"unexpected answer {answer[:80]!r}"],
        timings={"round_trip_s": seconds},
        details={"cloud": status, "answer": answer[:500]},
    )


def run_cloud_ui(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    """The whole cloud path: Cloud AI, relay-screen's ui_task, the local operator."""
    if tools.cloud is None:
        return _skipped(s, "Cloud AI is not installed")
    status = tools.cloud.status()
    if not status.get("logged_in"):
        return _skipped(s, "Cloud AI is not signed in", details={"cloud": status})
    before = set(tools.desktop.titles())
    rec.shot("before")
    prompt = (
        "This is Relay's diagnostics run on the owner's PC. Call the relay-screen ui_task "
        f"tool exactly once with this task: {s.task}\n"
        "Then reply with one line: DONE, or FAILED and why."
    )
    start = tools.clock()

    def on_line(event: dict[str, Any]) -> None:
        rec.event(event)
        if event.get("event") == "tool_use":
            emit(progress(s, event))

    try:
        run = tools.cloud.run_task(prompt, rec.folder, on_line)
    except Exception as exc:
        run = {"error": f"{type(exc).__name__}: {exc}"}
    seconds = round(tools.clock() - start, 2)
    tools.sleep(1.0)
    rec.shot("after")
    failed = [why for c in s.checks if (why := check(tools, c)) is not None]
    if run.get("error"):
        failed.insert(0, str(run["error"])[:300])
    if "mcp__relay-screen__ui_task" not in run.get("tools", []):
        failed.append("Cloud AI did not call ui_task")
    cleanup(s, tools, before)
    return Result(
        s.id,
        s.title,
        s.kind,
        "passed" if not failed else "failed",
        str(run.get("result") or run.get("error") or "")[:300],
        seconds,
        failed_checks=failed,
        timings={
            "total_s": seconds,
            "cloud_ms": run.get("duration_ms"),
            "cloud_api_ms": run.get("duration_api_ms"),
            "first_tool_s": run.get("first_tool_s"),
            "turns": run.get("num_turns"),
        },
        details={"task": s.task, "cloud": {k: v for k, v in run.items() if k != "result"}},
    )


def run_app(s: Scenario, tools: Tools, rec: Recorder, emit: Event) -> Result:
    """One app-function script on Relay's own window (D-101), no model involved."""
    del emit
    if tools.app is None:
        return _skipped(s, "Relay's window can't be driven here")
    signed_in = False
    if tools.cloud is not None:
        status = tools.cloud.status()
        signed_in = bool(status.get("logged_in"))
    if s.id in app_tests.NEEDS_CLOUD and not signed_in:
        return _skipped(s, "Cloud AI is not signed in")
    why = tools.app.ready()
    if why:
        return _skipped(s, why)
    start = tools.clock()
    ctx = app_tests.Ctx(
        tools.app,
        tools.clock,
        tools.sleep,
        rec.shot,
        rec.event,
        tools.settings_file,
        tools.stamp or time.strftime("%H%M%S"),
        signed_in,
    )
    rec.shot("before")
    try:
        app_tests.SCRIPTS[s.id](ctx)
    finally:
        rec.shot("after")
    checks = ctx.checks
    passed = len(checks) - len(ctx.failed)
    return Result(
        s.id,
        s.title,
        s.kind,
        "passed" if checks and not ctx.failed else "failed",
        f"{passed}/{len(checks)} checks passed" if checks else "nothing was checked",
        round(tools.clock() - start, 2),
        failed_checks=ctx.failed,
        details={"checks": checks},
    )


def _skipped(s: Scenario, why: str, details: dict[str, Any] | None = None) -> Result:
    return Result(s.id, s.title, s.kind, "skipped", why, 0.0, details=details or {})


RUNNERS: dict[str, Callable[[Scenario, Tools, Recorder, Event], Result]] = {
    "operator": run_operator,
    "displays": run_displays,
    "route": run_route,
    "cloud-text": run_cloud_text,
    "cloud-ui": run_cloud_ui,
    "app": run_app,
}


def run_all(
    chosen: Sequence[Scenario],
    tools: Tools,
    folder: Path,
    emit: Event,
    watch: ResourceWatch | None = None,
) -> list[Result]:
    results: list[Result] = []
    gone = close_test_docs(tools)
    if gone:
        emit({"event": "note", "text": f"Closed {gone} Notepad tab(s) left by earlier runs"})
    for n, s in enumerate(chosen, 1):
        if watch is not None:
            watch.scenario = s.id
        emit({"event": "scenario", "scenario": s.id, "title": s.title, "n": n, "of": len(chosen)})
        rec = Recorder(folder / f"{n:02d}-{s.id}", tools.capture, tools.clock)
        try:
            result = RUNNERS[s.kind](s, tools, rec, emit)
        except Exception as exc:  # one broken scenario never ends the run
            result = Result(s.id, s.title, s.kind, "failed", f"{type(exc).__name__}: {exc}", 0.0)
        finally:
            rec.close()
        result.folder = rec.folder.name
        result.area = area_of(s)
        result.screenshots = rec.shots
        (rec.folder / "result.json").write_text(json.dumps(asdict(result), indent=2), "utf-8")
        results.append(result)
        emit({"event": "result", **asdict(result)})
        if result.details.get("operator_status") == "stopped" and _owner_stopped(result):
            break  # the owner stopped Relay (kill switch or Stop): don't keep going
    return results


APPROVAL_STOPS = ("No answer from the owner", "The owner denied", "Relay could not show")


def _owner_stopped(result: Result) -> bool:
    """Stop or the kill switch, not an approval prompt (D-106): an unanswered or
    refused prompt fails its own scenario and the run goes on."""
    return not result.message.startswith(APPROVAL_STOPS)


# -- RAM and VRAM -------------------------------------------------------------------

MB = 1024 * 1024
# What Relay may use while it works, besides the model's weights in VRAM: the app and
# its window, the operator service and relay-screen. llama-server gets its own budget
# because it holds the model's working buffers.
RAM_BUDGET_MB = {"relay": 1500, "llama-server": 2500}
PYTHONS = ("python.exe", "pythonw.exe")


def group_processes(
    procs: Sequence[Mapping[str, Any]],
    vram: Mapping[int, int],
    me: int,
    python_dir: str | None = None,
) -> dict[str, dict[str, int]]:
    """Relay's processes by role, with their RAM (working set), private bytes and VRAM.

    `python_dir` is the folder of Relay's own Python: other programs' Python (the
    reference chat app's MCP servers, say) is not Relay's and isn't counted.
    """
    by_pid = {int(p["pid"]): p for p in procs}

    def relays(p: Mapping[str, Any]) -> bool:
        if python_dir is None or not p.get("path"):
            return True
        return str(p["path"]).casefold().startswith(python_dir.casefold())

    def ancestors(pid: int) -> list[str]:
        names: list[str] = []
        seen: set[int] = set()
        while pid in by_pid and pid not in seen and len(names) < 12:
            seen.add(pid)
            pid = int(by_pid[pid]["parent"])
            if pid in by_pid:
                names.append(str(by_pid[pid]["name"]))
        return names

    groups: dict[str, dict[str, int]] = {}
    for p in procs:
        pid, name = int(p["pid"]), str(p["name"])
        up = ancestors(pid)
        if pid == me:
            role = "this test"
        elif name == "relay.exe":
            role = "app"
        elif name == "msedgewebview2.exe" and "relay.exe" in up:
            role = "app window (WebView2)"
        elif name in PYTHONS and not relays(p):
            continue
        elif name in PYTHONS and "relay.exe" in up:
            role = "local AI service"
        elif name in PYTHONS and (cli_names.WINDOWS_EXE in up or me in _pids(p, by_pid)):
            role = "relay-screen for Cloud AI"
        elif name.startswith("llama-server"):
            role = "llama-server"
        else:
            continue
        g = groups.setdefault(role, {"processes": 0, "ram_mb": 0, "private_mb": 0, "vram_mb": 0})
        g["processes"] += 1
        g["ram_mb"] += int(p.get("ram") or 0) // MB
        g["private_mb"] += int(p.get("private") or 0) // MB
        g["vram_mb"] += int(vram.get(pid, 0)) // MB
    return groups


def _pids(p: Mapping[str, Any], by_pid: Mapping[int, Mapping[str, Any]]) -> set[int]:
    """The pids above `p`."""
    found: set[int] = set()
    pid = int(p["parent"])
    while pid in by_pid and pid not in found and len(found) < 12:
        found.add(pid)
        pid = int(by_pid[pid]["parent"])
    return found


class ResourceWatch:
    """Samples Relay's RAM and VRAM every few seconds while the run goes."""

    def __init__(
        self,
        sample: Callable[[], dict[str, dict[str, int]]],
        out: Path,
        every: float = 2.0,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.sample = sample
        self.out = out
        self.every = every
        self.clock = clock
        self.start = clock()
        self.peaks: dict[str, dict[str, int]] = {}
        self.peak_relay_ram = 0
        self.samples = 0
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self.scenario = ""

    def take(self) -> None:
        try:
            groups = self.sample()
        except Exception as exc:
            groups = {}
            line = {"t": round(self.clock() - self.start, 1), "error": str(exc)}
        else:
            line = {"t": round(self.clock() - self.start, 1), "scenario": self.scenario}
            line["groups"] = groups
        self.samples += 1
        relay = sum(
            g["ram_mb"] for k, g in groups.items() if k not in ("llama-server", "this test")
        )
        self.peak_relay_ram = max(self.peak_relay_ram, relay)
        for role, g in groups.items():
            peak = self.peaks.setdefault(role, {k: 0 for k in g})
            for k, v in g.items():
                peak[k] = max(peak.get(k, 0), v)
        with self.out.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line) + "\n")

    def begin(self) -> None:
        def loop() -> None:
            while not self._stop.is_set():
                self.take()
                self._stop.wait(self.every)

        self._thread = threading.Thread(target=loop, name="relay-resources", daemon=True)
        self._thread.start()

    def end(self) -> dict[str, Any]:
        self._stop.set()
        if self._thread is not None:
            self._thread.join(5)
        llama = self.peaks.get("llama-server", {}).get("ram_mb", 0)
        over = []
        if self.peak_relay_ram > RAM_BUDGET_MB["relay"]:
            over.append(
                f"Relay used {self.peak_relay_ram} MB of RAM at its peak "
                f"(budget {RAM_BUDGET_MB['relay']} MB, not counting the model)"
            )
        if llama > RAM_BUDGET_MB["llama-server"]:
            over.append(
                f"llama-server used {llama} MB of RAM at its peak "
                f"(budget {RAM_BUDGET_MB['llama-server']} MB)"
            )
        return {
            "samples": self.samples,
            "peak_relay_ram_mb": self.peak_relay_ram,
            "budget_mb": RAM_BUDGET_MB,
            "over_budget": over,
            "peaks": self.peaks,
        }


# -- the summary and the report ---------------------------------------------------


def summarize(
    results: Sequence[Result],
    environment: Mapping[str, Any],
    folder: Path,
    started: str,
    seconds: float,
    resources: Mapping[str, Any] | None = None,
) -> dict[str, Any]:
    counts = {k: sum(1 for r in results if r.status == k) for k in ("passed", "failed", "skipped")}
    problems = [
        f"{r.title}: {'; '.join(r.failed_checks) or r.message}"
        for r in results
        if r.status == "failed"
    ]
    problems += list((resources or {}).get("over_budget", []))
    return {
        "schema": SCHEMA,
        "overview": overview(results),
        "resources": dict(resources or {}),
        "started": started,
        "seconds": round(seconds, 1),
        "folder": str(folder),
        "report": str(folder / "report.html"),
        **counts,
        "total": len(results),
        "ok": counts["failed"] == 0 and counts["passed"] > 0,
        "problems": problems,
        "areas": [
            {"area": a, **{k: sum(1 for r in mine if r.status == k) for k in counts}}
            for a in AREAS
            if (mine := [r for r in results if r.area == a])
        ],
        "environment": dict(environment),
        "results": [asdict(r) for r in results],
    }


def overview(results: Sequence[Result]) -> dict[str, Any]:
    """The whole run in a few numbers: where the time went and which models did the work."""
    operator = [r for r in results if r.kind == "operator" and r.status != "skipped"]
    calls = sum(int(r.timings.get("model_calls") or 0) for r in operator)
    think = sum(int(r.timings.get("think_ms_total") or 0) for r in operator)
    perceive = sum(int(r.timings.get("perceive_ms_total") or 0) for r in operator)
    act = sum(int(r.timings.get("act_ms_total") or 0) for r in operator)
    slowest = max(results, key=lambda r: r.seconds, default=None)
    cloud = {r.id: r.seconds for r in results if r.kind.startswith("cloud") and r.passed}
    return {
        "models": sorted({r.model for r in operator if r.model}),
        "operator_tasks": len(operator),
        "operator_passed": sum(1 for r in operator if r.passed),
        "operator_steps": sum(r.steps for r in operator),
        "said_done_but_was_not": [r.id for r in operator if r.details.get("false_done")],
        "model_calls": calls,
        "model_ms_per_call": round(think / calls) if calls else None,
        "time_split_ms": {"perceive": perceive, "think": think, "act": act},
        "slowest": {"id": slowest.id, "seconds": slowest.seconds} if slowest else None,
        "cloud_seconds": cloud,
    }


def report_html(summary: Mapping[str, Any]) -> str:
    env = summary.get("environment", {})
    e = html.escape
    head = (
        f"{summary['passed']} passed, {summary['failed']} failed, {summary['skipped']} skipped "
        f"in {summary['seconds']} s"
    )
    rows = []
    area = None
    for r in summary["results"]:
        if r.get("area") and r["area"] != area:
            area = r["area"]
            got = next((a for a in summary.get("areas", []) if a["area"] == area), {})
            rows.append(
                f"<h2 class=area>{e(area)} <small>{got.get('passed', 0)} passed, "
                f"{got.get('failed', 0)} failed, {got.get('skipped', 0)} skipped</small></h2>"
            )
        shots = "".join(
            f'<a href="{e(r["folder"])}/{e(x)}"><img src="{e(r["folder"])}/{e(x)}" '
            f'title="{e(x)}"></a>'
            for x in r["screenshots"]
        )
        checks = "".join(f"<li>{e(c)}</li>" for c in r["failed_checks"])
        timing = {k: v for k, v in r["timings"].items() if k != "per_step" and v is not None}
        rows.append(
            f'<section class="{e(r["status"])}"><h2>{e(r["title"])} '
            f"<span>{e(r['status'])}</span></h2>"
            f"<p>{e(r['message'])}</p>"
            f"<p class=meta>{r['seconds']} s · {r['steps']} steps"
            f"{' · ' + e(r['model']) if r['model'] else ''} · "
            f'<a href="{e(r["folder"])}/events.jsonl">events</a></p>'
            f"{'<ul>' + checks + '</ul>' if checks else ''}"
            f"<pre>{e(json.dumps(timing))}</pre><div class=shots>{shots}</div></section>"
        )
    return f"""<!doctype html><html><head><meta charset="utf-8">
<title>Relay diagnostics {e(summary["started"])}</title><style>
:root{{color-scheme:light dark}}body{{font:15px system-ui;margin:32px;max-width:1200px}}
section{{border:1px solid #8884;border-radius:12px;padding:12px 18px;margin:14px 0}}
h2{{font-size:17px;margin:4px 0}}h2 span{{font-size:13px;padding:2px 8px;border-radius:8px}}
.passed h2 span{{background:#2a7a3a;color:#fff}}.failed h2 span{{background:#b3261e;color:#fff}}
.skipped h2 span{{background:#777;color:#fff}}.meta{{color:#888}}pre{{white-space:pre-wrap}}
h2.area{{font-size:20px;margin-top:28px}}h2.area small{{font-size:13px;color:#888}}
.shots img{{height:120px;margin:4px;border:1px solid #8884;border-radius:6px}}
</style></head><body><h1>Relay diagnostics</h1><p>{e(head)}</p>
<pre>{e(json.dumps(summary.get("overview", {}), indent=2))}</pre>
<h2>RAM and VRAM (peaks)</h2><pre>{e(json.dumps(summary.get("resources", {}), indent=2))}</pre>
<details><summary>Environment</summary><pre>{e(json.dumps(env, indent=2))}</pre></details>
{"".join(rows)}</body></html>"""


def write_summary(summary: Mapping[str, Any], folder: Path) -> None:
    (folder / "summary.json").write_text(json.dumps(summary, indent=2), "utf-8")
    (folder / "report.html").write_text(report_html(summary), "utf-8")
    latest = {"folder": str(folder), "summary": str(folder / "summary.json")}
    (folder.parent / "latest.json").write_text(json.dumps(latest, indent=2), "utf-8")


def latest(root: Path) -> dict[str, Any] | None:
    """The newest run's summary under `root`, or None."""
    try:
        pointer = json.loads((root / "latest.json").read_text("utf-8"))
        return json.loads(Path(pointer["summary"]).read_text("utf-8"))
    except (OSError, ValueError, KeyError, TypeError):
        return None


# -- the real links -----------------------------------------------------------------


class ServiceLink:
    """The operator service the app runs (D-086), over its localhost port: the same path
    the owner's tasks take."""

    name = "service"

    def __init__(self, port: int, token: str, home: Path | None = None) -> None:
        self.port = port
        self.token = token
        self.home = home  # re-read before each task: the app tests restart Relay

    @classmethod
    def find(cls, home: Path) -> ServiceLink | None:
        try:
            info = json.loads(record_path(home).read_text("utf-8"))
            link = cls(int(info["port"]), str(info["token"]), home)
        except (OSError, ValueError, KeyError, TypeError):
            return None
        try:
            socket.create_connection(("127.0.0.1", link.port), timeout=2).close()
        except OSError:
            return None
        return link

    def _send(self, command: dict[str, Any]) -> socket.socket:
        conn = socket.create_connection(("127.0.0.1", self.port), timeout=5)
        conn.settimeout(TASK_TIMEOUT)
        conn.sendall((json.dumps({"token": self.token, **command}) + "\n").encode("utf-8"))
        return conn

    def run(
        self, task: str, on_event: Event, *, offline: bool, max_steps: int, answer: str = ANSWER
    ) -> None:
        if self.home is not None and (fresh := ServiceLink.find(self.home)) is not None:
            self.port, self.token = fresh.port, fresh.token
        command = {"op": "task", "text": task, "source": "diagnostics", "offline": offline}
        command["max_steps"] = max_steps
        with self._send(command) as conn, conn.makefile("rb") as stream:
            for raw in stream:  # until the service closes it, after the "model" line
                try:
                    event = json.loads(raw)
                except ValueError:
                    continue
                if not isinstance(event, dict):
                    continue
                if event.get("event") == "question":
                    say = {"op": "say", "id": event.get("id"), "text": answer}
                    with contextlib.suppress(OSError):
                        self._send(say).close()
                on_event(event)


class LocalLink:
    """A local operator in this process, for runs while Relay itself is closed."""

    name = "in-process"

    def __init__(self, hands: Hands, grant: Callable[[str | None], None], ask: Callable) -> None:
        self.hands = hands
        self.grant = grant
        self.ask = ask
        self._operator: Any = None

    def run(
        self, task: str, on_event: Event, *, offline: bool, max_steps: int, answer: str = ANSWER
    ) -> None:
        from relay_screen.agent.operator import LocalOperator
        from relay_screen.runtime.supervisor import RuntimeUnavailable

        if self._operator is None:
            self._operator = LocalOperator()
        answers: queue.Queue[str] = queue.Queue()

        def emit(event: dict[str, Any]) -> None:
            if event.get("event") == "question":
                answers.put(answer)
            on_event(event)

        def inbox() -> list[str]:
            got: list[str] = []
            while not answers.empty():
                got.append(answers.get_nowait())
            return got

        self.grant(task)  # the owner started the run, so the task text is theirs (D-091)
        try:
            model, _ = self._operator.run(
                task,
                self.hands,
                max_steps,
                emit,
                inbox=inbox,
                cancelled=lambda: False,
                ask=None if offline else (lambda q: self.ask(q, task)),
            )
            on_event({"event": "model", "model": model})
        except RuntimeUnavailable as exc:
            on_event({"event": "unavailable", "message": str(exc)})
        finally:
            self.grant(None)

    def close(self) -> None:
        if self._operator is not None:
            self._operator.supervisor.stop_all()


class CloudCli:
    """Cloud AI through the owner's Cloud AI CLI (D-002): no API, no tokens."""

    def __init__(self, exe: str, python: str, run: Callable[..., Any] = subprocess.run) -> None:
        self.exe = exe
        self.python = python
        self._run = run
        self._status: dict[str, Any] | None = None

    def _flags(self) -> int:
        return 0x08000000 if sys.platform == "win32" else 0  # CREATE_NO_WINDOW

    def status(self) -> dict[str, Any]:
        if self._status is not None:
            return self._status
        found: dict[str, Any] = {"exe": self.exe}
        with contextlib.suppress(Exception):
            done = self._run(
                [self.exe, "--version"], capture_output=True, timeout=30,
                creationflags=self._flags(), check=False,
            )  # fmt: skip
            found["version"] = done.stdout.decode("utf-8", "replace").strip()
        with contextlib.suppress(Exception):
            done = self._run(
                [self.exe, "auth", "status", "--json"], capture_output=True, timeout=30,
                creationflags=self._flags(), check=False,
            )  # fmt: skip
            auth = json.loads(done.stdout.decode("utf-8", "replace"))
            found["logged_in"] = bool(auth.get("loggedIn"))
            for key in ("authMethod", "subscriptionType", "apiProvider"):
                if isinstance(auth.get(key), str):
                    found[key] = auth[key]
        self._status = found
        return found

    def ask(self, question: str) -> str:
        from relay_screen.agent.service import ask_cloud

        return ask_cloud(question, "Relay's diagnostics run", run=self._run)

    def run_task(self, prompt: str, folder: Path, on_line: Event) -> dict[str, Any]:
        config = {
            "mcpServers": {
                "relay-screen": {
                    "command": self.python,
                    "args": ["-m", "relay_screen.server"],
                    "env": {
                        "RELAY_APPROVAL": os.environ.get("RELAY_APPROVAL", "risky"),
                        "RELAY_DIAGNOSTICS": "1",  # its ui_task stays out of the chat
                        "PYTHONUTF8": "1",
                    },
                }
            }
        }
        config_file = folder / "mcp-config.json"
        config_file.write_text(json.dumps(config, indent=2), "utf-8")
        cmd = [
            self.exe, "-p", "--output-format", "stream-json", "--verbose",
            "--strict-mcp-config", "--mcp-config", str(config_file),
            "--allowedTools", "mcp__relay-screen__ui_task",
            "--disallowedTools", NO_TOOLS,
        ]  # fmt: skip
        start = time.monotonic()
        found: dict[str, Any] = {"tools": []}
        transcript = (folder / "cloud.jsonl").open("w", encoding="utf-8")
        proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            creationflags=self._flags(),
        )
        timed_out = threading.Event()

        def kill() -> None:
            timed_out.set()
            proc.kill()

        killer = threading.Timer(CLOUD_TIMEOUT, kill)
        killer.start()
        try:
            assert proc.stdin and proc.stdout and proc.stderr
            proc.stdin.write(prompt.encode("utf-8"))
            proc.stdin.close()
            for raw in proc.stdout:
                text = raw.decode("utf-8", "replace").strip()
                if not text:
                    continue
                transcript.write(text + "\n")
                try:
                    line = json.loads(text)
                except ValueError:
                    continue
                if isinstance(line, dict):
                    parse_cloud_line(line, found, round(time.monotonic() - start, 2), on_line)
            proc.wait()
            error = proc.stderr.read().decode("utf-8", "replace").strip()
        finally:
            killer.cancel()
            transcript.close()
        if proc.returncode != 0 and not found.get("result"):
            found["error"] = error[:500] or f"Cloud AI exited with {proc.returncode}"
        if timed_out.is_set():
            found["error"] = f"Cloud AI did not finish within {CLOUD_TIMEOUT:.0f} s"
        return found


def parse_cloud_line(line: Mapping[str, Any], found: dict[str, Any], t: float, on: Event) -> None:
    """Pick what matters out of one `cloud-cli -p --output-format stream-json` line."""
    kind = line.get("type")
    if kind == "system" and line.get("subtype") == "init":
        found["model"] = line.get("model")
        servers = line.get("mcp_servers")
        found["mcp_servers"] = servers
        on({"event": "cloud_init", "model": line.get("model"), "mcp_servers": servers})
    elif kind == "assistant":
        message = line.get("message")
        for block in message.get("content", []) if isinstance(message, dict) else []:
            if not isinstance(block, dict):
                continue
            if block.get("type") == "tool_use":
                name = str(block.get("name"))
                found["tools"].append(name)
                found.setdefault("first_tool_s", t)
                on({"event": "tool_use", "tool": name, "input": block.get("input")})
            elif block.get("type") == "text":
                on({"event": "cloud_text", "text": str(block.get("text"))[:2000]})
    elif kind == "user":
        message = line.get("message")
        for block in message.get("content", []) if isinstance(message, dict) else []:
            if isinstance(block, dict) and block.get("type") == "tool_result":
                content = block.get("content")
                if isinstance(content, list):
                    content = " ".join(
                        str(c.get("text")) for c in content if isinstance(c, dict) and "text" in c
                    )
                on({"event": "tool_result", "is_error": block.get("is_error"), "text": content})
    elif kind == "result":
        for key in ("subtype", "is_error", "duration_ms", "duration_api_ms", "num_turns", "result"):
            if key in line:
                found[key] = line[key]
        on({"event": "cloud_result", **{k: found.get(k) for k in ("is_error", "duration_ms")}})


# -- the environment ---------------------------------------------------------------


def environment(home: Path, link: str, cloud: CloudLink | None) -> dict[str, Any]:
    """What the run ran on. Never secrets: no tokens, no settings beyond Local AI and
    the approval mode."""
    found: dict[str, Any] = {
        "relay_screen": __version__,
        "relay": relay_version(),
        "python": sys.version.split()[0],
        "python_exe": sys.executable,
        "os": platform.platform(),
        "operator_link": link,
        "approval_mode": os.environ.get("RELAY_APPROVAL"),
    }
    with contextlib.suppress(Exception):
        from relay_screen.runtime import gpu

        info = gpu.query()
        if info is not None:
            found["gpu"] = {
                "name": info.name,
                "driver": info.driver,
                "vram_total_gb": round(info.total / gpu.GIB, 1),
                "vram_free_gb": round(info.free / gpu.GIB, 1),
            }
    with contextlib.suppress(Exception):
        from relay_screen.runtime.paths import RelayPaths
        from relay_screen.runtime.store import Store

        installed = Store(RelayPaths.default()).installed()
        runtime = installed.runtime
        found["runtime"] = f"llama.cpp {runtime.tag} (CUDA {runtime.cuda})" if runtime else None
        found["models"] = sorted(installed.models)
    with contextlib.suppress(Exception):
        from relay_screen.runtime.paths import local_ai_settings, settings_file

        local_ai = local_ai_settings(settings_file())
        found["local_ai_settings"] = {
            k: v for k, v in local_ai.items() if k in ("agent", "describe", "grounding")
        }
    with contextlib.suppress(Exception):
        servers = home / "servers"
        found["warm_models"] = sorted(p.stem for p in servers.glob("*.json"))
    if cloud is not None:
        found["cloud"] = cloud.status()
    return found


def relay_version() -> str | None:
    if sys.platform != "win32":
        return None
    import winreg

    key = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay"
    for hive in (winreg.HKEY_LOCAL_MACHINE, winreg.HKEY_CURRENT_USER):
        with contextlib.suppress(OSError), winreg.OpenKey(hive, key) as k:
            return str(winreg.QueryValueEx(k, "DisplayVersion")[0])
    return None


def _png_size(data: bytes) -> list[int] | None:
    if data[:8] != b"\x89PNG\r\n\x1a\n" or len(data) < 24:
        return None
    return [int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")]


def foreground_capture(width: int = SHOT_WIDTH) -> Callable[[], bytes | None]:
    """A JPEG of the monitor holding the window in front (Windows only)."""
    from PIL import Image

    from relay_screen import displays
    from relay_screen.capture import MssGrabber

    grabber = MssGrabber()

    def capture() -> bytes | None:
        found = displays.enumerate_displays()
        if not found:
            return None
        display = displays.foreground_display(found) or next(
            (d for d in found if d.primary), found[0]
        )
        image = grabber.grab(display.rect)
        if image.width > width:
            height = round(image.height * width / image.width)
            image = image.resize((width, height), Image.Resampling.LANCZOS)
        out = io.BytesIO()
        image.save(out, format="JPEG", quality=72)
        return out.getvalue()

    return capture


class LogTail:
    """The lines a log file gains during the run."""

    def __init__(self, path: Path) -> None:
        self.path = path
        try:
            self.offset = path.stat().st_size
        except OSError:
            self.offset = 0

    def copy_to(self, target: Path) -> bool:
        try:
            with self.path.open("rb") as f:
                f.seek(self.offset)
                data = f.read(LOG_TAIL)
        except OSError:
            return False
        if data:
            target.write_bytes(data)
        return bool(data)
