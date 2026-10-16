"""`relay-screen agent "task"`: run a UI task with the local model only (D-067).

    relay-screen agent "open Notepad and type hello" [--model ID] [--max-steps N] [--json]

With --json every event is one JSON line on stdout, for the app's chat:
    {"event": "loading", "model": ...}  {"event": "ready", "model": ..., "seconds": ...}
    {"event": "step", "step": n, "thought": ..., "action": ..., "result": ...}
    {"event": "bad_reply", ...}  then one of
    {"event": "done" | "failed" | "stopped", "message": ..., "steps": n}
    {"event": "unavailable", "message": ...}  the local model can't run at all
Exit code: 0 done, 1 failed or stopped, 2 unavailable.

    relay-screen agent-bench [--only ID ...] [--repeat N] [--tasks FILE] [--json]

runs the fixed benchmark (D-068) and writes the results under %LOCALAPPDATA%\\Relay\\bench.
Exit code: 0 at or above the 95% target, 1 below it, 2 unavailable.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.request
from collections.abc import Callable, Mapping
from dataclasses import asdict
from pathlib import Path
from typing import Any

from relay_screen.agent.local import MAX_STEPS, REPLY_SCHEMA, LocalAgent, Model
from relay_screen.runtime import catalog
from relay_screen.runtime.paths import RelayPaths, local_ai_settings, settings_file
from relay_screen.runtime.store import InstalledModel, Store
from relay_screen.runtime.supervisor import (
    IDLE_SECONDS,
    RuntimeUnavailable,
    Supervisor,
    jsonl_recorder,
)

# Models that can plan as well as point, best first (D-067). Grounding-only models
# (role "locate") can still be picked by hand with --model.
PLANNER_ORDER = (
    "holo-3.1-9b",  # D-087: the operator's model
    "qwen3-vl-8b-instruct",
    "holo2-8b",
    "holo2-4b",
    "ui-venus-1.5-8b",
)
REPLY_TOKENS = 700  # a batch of 6 actions with its thought fits (D-084)
REPLY_TIMEOUT = 180.0


def pick_model(
    installed: Mapping[str, InstalledModel], settings: Mapping[str, Any], wanted: str | None
) -> str:
    """The downloaded model to plan with: --model, then Settings, then the best installed."""
    if wanted:
        if wanted not in installed:
            raise RuntimeUnavailable(f"Model {wanted} is not downloaded")
        return wanted

    def chosen(key: str) -> str | None:
        choice = settings.get(key)
        model_id = choice.get("model_id") if isinstance(choice, dict) else None
        return model_id if isinstance(model_id, str) and model_id in installed else None

    # An explicit operator choice wins; the describe/grounding picks are for looking,
    # so a downloaded planner comes before them (D-087).
    if found := chosen("agent"):
        return found
    for model_id in PLANNER_ORDER:
        if model_id in installed:
            return model_id
    for key in ("describe", "grounding"):
        if found := chosen(key):
            return found
    if installed:
        return sorted(installed)[0]
    raise RuntimeUnavailable(
        "No local model is downloaded. Download one in Settings > Local AI "
        "(relay-screen models download holo-3.1-9b)."
    )


def idle_seconds(settings: Mapping[str, Any]) -> float:
    value = settings.get("idle_unload_secs")
    return float(value) if isinstance(value, int | float) and value > 0 else IDLE_SECONDS


def llama_model(url: str) -> Model:
    """Ask a llama-server endpoint for the next action (OpenAI-style chat with an image)."""

    def ask(messages: list[dict[str, Any]]) -> str:
        body = json.dumps(
            {
                "messages": messages,
                "max_tokens": REPLY_TOKENS,
                "temperature": 0,
                # One JSON object, no thinking pass: Qwen3.5-based models like Holo3.1
                # think first by default, which costs seconds per step (D-087).
                "response_format": {"type": "json_object", "schema": REPLY_SCHEMA},
                "chat_template_kwargs": {"enable_thinking": False},
                "cache_prompt": True,
            }
        ).encode()
        request = urllib.request.Request(
            f"{url}/v1/chat/completions", body, {"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(request, timeout=REPLY_TIMEOUT) as response:
            data = json.load(response)
        return data["choices"][0]["message"]["content"] or ""

    return ask


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="relay-screen agent", description="Run a UI task with the local model only."
    )
    p.add_argument("task", nargs="+", help="what to do, in plain words")
    p.add_argument("--model", help="a downloaded model id (default: from Settings)")
    p.add_argument("--max-steps", type=int, default=MAX_STEPS)
    p.add_argument("--json", action="store_true", help="events as JSON lines")
    return p


class Loaded:
    """A local model that is running, and how to reach it."""

    def __init__(self, emit: Callable[[dict[str, Any]], None], wanted: str | None) -> None:
        self.paths = RelayPaths.default()
        settings = local_ai_settings(settings_file())
        self.record = jsonl_recorder(self.paths.log)
        store = Store(self.paths)
        self.supervisor = Supervisor(store, idle=idle_seconds(settings), record=self.record)
        try:
            self.model_id = pick_model(store.installed().models, settings, wanted)
            spec = catalog.find(self.model_id)
            self.coords: catalog.Coords = spec.coords if spec else "rel1000"
            emit({"event": "loading", "model": self.model_id})
            start = time.monotonic()
            self.model = llama_model(self.supervisor.ensure(self.model_id).url)
            seconds = round(time.monotonic() - start, 1)
            emit({"event": "ready", "model": self.model_id, "seconds": seconds})
        except RuntimeUnavailable:
            self.supervisor.stop_all()
            raise


def emitter(as_json: bool) -> Callable[[dict[str, Any]], None]:
    def emit(event: dict[str, Any]) -> None:
        print(json.dumps(event) if as_json else _human(event), flush=True)

    return emit


def main(argv: list[str]) -> int:
    args = parser().parse_args(argv)
    task = " ".join(args.task).strip()
    emit = emitter(args.json)
    try:
        loaded = Loaded(emit, args.model)
    except RuntimeUnavailable as exc:
        emit({"event": "unavailable", "message": str(exc)})
        return 2

    try:
        with loaded.supervisor.task(loaded.model_id):
            agent = LocalAgent(
                _hands(),
                loaded.model,
                coords=loaded.coords,
                max_steps=args.max_steps,
                on_event=emit,
            )
            outcome = agent.run(task)
        loaded.record({"event": "agent", "model": loaded.model_id, "status": outcome.status})
        return 0 if outcome.status == "done" else 1
    except Exception as exc:  # report, never leave the model loaded
        emit({"event": "failed", "message": f"{type(exc).__name__}: {exc}", "steps": 0})
        return 1
    finally:
        # This process ends with the task, so nothing would unload the model later.
        loaded.supervisor.stop_all()


def bench_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="relay-screen agent-bench",
        description="Measure the local operator on the fixed task set (opens windows).",
    )
    p.add_argument("--only", nargs="+", metavar="ID", help="run only these task ids")
    p.add_argument("--repeat", type=int, default=1, help="run every task this many times")
    p.add_argument("--tasks", type=Path, help="a task file instead of the built-in one")
    p.add_argument("--model", help="a downloaded model id (default: from Settings)")
    p.add_argument("--json", action="store_true", help="events as JSON lines")
    return p


def bench_main(argv: list[str]) -> int:
    from relay_screen.agent import bench

    args = bench_parser().parse_args(argv)
    emit = emitter(args.json)
    tasks = bench.load_tasks(args.tasks)
    if args.only:
        unknown = sorted(set(args.only) - {t.id for t in tasks})
        if unknown:
            emit({"event": "unavailable", "message": f"Unknown task ids: {', '.join(unknown)}"})
            return 2
        tasks = [t for t in tasks if t.id in args.only]
    try:
        loaded = Loaded(emit, args.model)
    except RuntimeUnavailable as exc:
        emit({"event": "unavailable", "message": str(exc)})
        return 2

    def on_result(result: bench.TaskResult) -> None:
        emit({"event": "task", **asdict(result)})

    try:
        hands = _hands()
        with loaded.supervisor.task(loaded.model_id):
            results = bench.run_bench(
                tasks,
                hands,
                loaded.model,
                bench.windows_desktop(hands),
                coords=loaded.coords,
                repeat=max(1, args.repeat),
                on_result=on_result,
            )
    finally:
        loaded.supervisor.stop_all()
    report = {"model": loaded.model_id, **bench.summary(results)}
    out = loaded.paths.home / "bench" / time.strftime("%Y%m%d-%H%M%S.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2), encoding="utf-8")
    message = (
        f"{report['passed']}/{report['runs']} passed ({report['rate']:.0%}, "
        f"target {bench.TARGET:.0%}). Results: {out}"
    )
    emit({"event": "summary", "message": message, "file": str(out), **report})
    return 0 if report["meets_target"] else 1


def _hands() -> Any:
    """The same Computer Claude drives: kill switch, approval prompt and audit log."""
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    from relay_screen.server import default_services

    return default_services().computer()


def _human(event: dict[str, Any]) -> str:
    kind = event.get("event")
    if kind == "step":
        return f"{event['step']:>2}. {event['action']} -> {event['result']}"
    if kind == "loading":
        return f"Loading {event['model']}..."
    if kind == "ready":
        return f"{event['model']} ready in {event['seconds']} s"
    if kind == "task":
        mark = "pass" if event["passed"] else "FAIL"
        why = "; ".join(event["failed_checks"]) or event["message"]
        return f"{mark} {event['id']} ({event['steps']} steps, {event['seconds']} s): {why}"
    if kind == "summary":
        return str(event["message"])
    if kind == "bad_reply":
        return f"    (unusable reply: {event['error']})"
    return f"{str(kind).capitalize()}: {event.get('message', '')}"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
