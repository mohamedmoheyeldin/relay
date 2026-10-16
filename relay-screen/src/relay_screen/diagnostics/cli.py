"""`relay-screen diagnostics`: the full test run from the command line (D-096).

    relay-screen diagnostics                   run everything (opens windows)
    relay-screen diagnostics --only notepad-type web-form
    relay-screen diagnostics --list            the scenario ids
    relay-screen diagnostics --latest          print the newest run's summary.json

With --json every event is one JSON line on stdout (for the app):
    {"event": "start", "folder": ..., "scenarios": [{"id", "title"}...]}
    {"event": "scenario", "scenario": id, "title": ..., "n": 1, "of": 10}
    {"event": "progress", "scenario": id, "kind": "step", ...the operator's event...}
    {"event": "result", ...one scenario's result...}
    {"event": "summary", ...summary.json...}
Exit code: 0 when nothing failed, 1 when something failed, 2 when it could not run.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any

from relay_screen.diagnostics import run as diag


def default_root() -> Path:
    """Downloads\\Relay-diagnostics: everything Relay makes on the PC goes in Downloads."""
    return Path.home() / "Downloads" / "Relay-diagnostics"


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="relay-screen diagnostics",
        description="Run Relay's full test on this PC and record everything (opens windows).",
    )
    p.add_argument("--only", nargs="+", metavar="ID", help="run only these scenarios")
    p.add_argument("--skip", nargs="+", metavar="ID", default=[], help="leave these out")
    p.add_argument("--out", type=Path, help="folder for the runs (default Downloads)")
    p.add_argument("--json", action="store_true", help="events as JSON lines")
    p.add_argument("--list", action="store_true", help="list the scenarios and exit")
    p.add_argument("--latest", action="store_true", help="print the newest summary and exit")
    p.add_argument("--in-process", action="store_true", help="don't use the app's operator")
    p.add_argument("--no-claude", action="store_true", help="skip everything that uses Claude")
    return p


def cli_python() -> str:
    """python.exe next to the running interpreter (the app may start pythonw.exe)."""
    exe = Path(sys.executable)
    console = exe.with_name("python.exe")
    return str(console) if exe.name.lower() == "pythonw.exe" and console.is_file() else str(exe)


def relay_window(desktop: Any) -> Any:
    """Relay's own window for the app tests, or None where UI Automation can't load."""
    try:
        return desktop.RelayWindow()
    except Exception:
        return None


def emitter(as_json: bool, log: list[Path]) -> Callable[[dict[str, Any]], None]:
    def emit(event: dict[str, Any]) -> None:
        line = json.dumps(event, ensure_ascii=False, default=str)
        if log:
            with log[0].open("a", encoding="utf-8") as f:
                f.write(line + "\n")
        text = line if as_json else human(event)
        if text:
            print(text, flush=True)

    return emit


def human(event: dict[str, Any]) -> str | None:
    kind = event.get("event")
    if kind == "start":
        return f"Relay diagnostics: {len(event['scenarios'])} scenarios -> {event['folder']}"
    if kind == "scenario":
        return f"[{event['n']}/{event['of']}] {event['title']}"
    if kind == "progress":
        if event.get("kind") == "step":
            return f"    {event.get('step')}. {event.get('say') or event.get('action')}"
        if event.get("kind") == "tool_use":
            return f"    Claude called {event.get('tool')}"
        return None
    if kind == "result":
        why = "; ".join(event["failed_checks"]) or event["message"]
        return f"    {event['status'].upper()} in {event['seconds']} s: {why}"
    if kind == "summary":
        return (
            f"{event['passed']} passed, {event['failed']} failed, {event['skipped']} skipped. "
            f"Report: {event['report']}"
        )
    if kind == "unavailable":
        return f"Could not run: {event['message']}"
    return None


def main(argv: list[str]) -> int:
    args = parser().parse_args(argv)
    root = args.out or default_root()
    if args.latest:
        found = diag.latest(root)
        print(json.dumps(found, indent=2) if found else "{}")
        return 0 if found else 2
    if args.list:
        for s in diag.scenarios("000000", "http://127.0.0.1"):
            print(f"{s.id:20} {s.title}")
        return 0

    log: list[Path] = []
    emit = emitter(args.json, log)
    form = diag.FormServer()
    stamp = time.strftime("%H%M%S")
    chosen = diag.scenarios(stamp, form.base)
    known = {s.id for s in chosen}
    unknown = sorted((set(args.only or []) | set(args.skip)) - known)
    if unknown:
        emit({"event": "unavailable", "message": f"Unknown scenarios: {', '.join(unknown)}"})
        form.close()
        return 2
    chosen = [s for s in chosen if (not args.only or s.id in args.only) and s.id not in args.skip]
    if args.no_claude:
        chosen = [s for s in chosen if not s.kind.startswith("claude") and s.id != "app-stop"]

    started = time.strftime("%Y-%m-%d %H:%M:%S")
    folder = root / time.strftime("%Y-%m-%d_%H-%M-%S")
    folder.mkdir(parents=True, exist_ok=True)
    log.append(folder / "run.jsonl")
    # The owner started the run, so it acts like their own task: ask only before acts
    # that can't be undone, and their task words grant those (D-073, D-091).
    os.environ.setdefault("RELAY_APPROVAL", "risky")

    from relay_screen.agent.bench import windows_desktop
    from relay_screen.agent.service import ask_claude, claude_exe
    from relay_screen.dpi import set_dpi_awareness
    from relay_screen.runtime.paths import RelayPaths, settings_file
    from relay_screen.server import default_services

    set_dpi_awareness()
    services = default_services()
    hands = services.computer()
    paths = RelayPaths.default()
    tails = {"runtime-log.jsonl": diag.LogTail(paths.log)}
    if services.audit is not None:
        tails["audit-log.jsonl"] = diag.LogTail(services.audit.path)

    gate = services.gate
    link: Any = None if args.in_process else diag.ServiceLink.find(paths.home)
    local: diag.LocalLink | None = None
    if link is None:
        local = diag.LocalLink(hands, gate.grant if gate else (lambda task: None), ask_claude)
        link = local
    exe = None if args.no_claude else claude_exe()
    claude = diag.ClaudeCli(exe, cli_python()) if exe else None

    state = services.state

    def displays() -> list[dict[str, Any]]:
        found = []
        for d in state.displays():
            geometry = state.geometry(d)
            found.append({**d.describe(), "screenshot_size": [geometry.width, geometry.height]})
        return found

    from relay_screen.diagnostics import desktop

    def setup(s: diag.Scenario) -> str | None:
        if s.setup == "explorer-other-display":
            return desktop.explorer_on_other_display(root / diag.EXPLORER_TEST)
        return f"unknown setup {s.setup!r}"

    def sample() -> dict[str, dict[str, int]]:
        return diag.group_processes(
            desktop.processes(),
            desktop.gpu_processes(),
            os.getpid(),
            str(Path(sys.executable).parent),
        )

    watch = diag.ResourceWatch(sample, folder / "resources.jsonl")
    tools = diag.Tools(
        hands=hands,
        desktop=windows_desktop(hands),
        link=link,
        claude=claude,
        capture=diag.foreground_capture(),
        displays=displays,
        select_display=state.select,
        active_display=lambda: state.active().id,
        form=form,
        setup=setup,
        interfere=lambda: desktop.owner_interrupts(Path.home() / "Downloads"),
        app=relay_window(desktop),
        settings_file=settings_file(),
        stamp=stamp,
    )
    emit(
        {
            "event": "start",
            "folder": str(folder),
            "link": link.name,
            "scenarios": [{"id": s.id, "title": s.title, "area": diag.area_of(s)} for s in chosen],
        }
    )
    clock = time.monotonic()
    env = diag.environment(paths.home, link.name, claude)
    (folder / "environment.json").write_text(json.dumps(env, indent=2), "utf-8")
    watch.begin()
    try:
        results = diag.run_all(chosen, tools, folder, emit, watch)
    finally:
        resources = watch.end()
        form.close()
        if local is not None:
            local.close()
        for name, tail in tails.items():
            tail.copy_to(folder / name)
    summary = diag.summarize(results, env, folder, started, time.monotonic() - clock, resources)
    diag.write_summary(summary, folder)
    emit({"event": "summary", **summary})
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
