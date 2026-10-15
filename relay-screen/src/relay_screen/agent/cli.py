"""`relay-screen agent "task"`: run a UI task with the local model only (D-067).

    relay-screen agent "open Notepad and type hello" [--model ID] [--max-steps N] [--json]

With --json every event is one JSON line on stdout, for the app's chat:
    {"event": "loading", "model": ...}  {"event": "ready", "model": ..., "seconds": ...}
    {"event": "step", "step": n, "thought": ..., "action": ..., "result": ...}
    {"event": "bad_reply", ...}  then one of
    {"event": "done" | "failed" | "stopped", "message": ..., "steps": n}
    {"event": "unavailable", "message": ...}  the local model can't run at all
Exit code: 0 done, 1 failed or stopped, 2 unavailable.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.request
from collections.abc import Mapping
from typing import Any

from relay_screen.agent.local import MAX_STEPS, LocalAgent, Model
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
PLANNER_ORDER = ("qwen3-vl-8b-instruct", "holo2-8b", "holo2-4b", "ui-venus-1.5-8b")
REPLY_TOKENS = 400
REPLY_TIMEOUT = 180.0


def pick_model(
    installed: Mapping[str, InstalledModel], settings: Mapping[str, Any], wanted: str | None
) -> str:
    """The downloaded model to plan with: --model, then Settings, then the best installed."""
    if wanted:
        if wanted not in installed:
            raise RuntimeUnavailable(f"Model {wanted} is not downloaded")
        return wanted
    for key in ("agent", "describe", "grounding"):
        choice = settings.get(key)
        model_id = choice.get("model_id") if isinstance(choice, dict) else None
        if isinstance(model_id, str) and model_id in installed:
            return model_id
    for model_id in PLANNER_ORDER:
        if model_id in installed:
            return model_id
    if installed:
        return sorted(installed)[0]
    raise RuntimeUnavailable(
        "No local model is downloaded. Download one in Settings > Local AI "
        "(relay-screen models download qwen3-vl-8b-instruct)."
    )


def idle_seconds(settings: Mapping[str, Any]) -> float:
    value = settings.get("idle_unload_secs")
    return float(value) if isinstance(value, int | float) and value > 0 else IDLE_SECONDS


def llama_model(url: str) -> Model:
    """Ask a llama-server endpoint for the next action (OpenAI-style chat with an image)."""

    def ask(messages: list[dict[str, Any]]) -> str:
        body = json.dumps(
            {"messages": messages, "max_tokens": REPLY_TOKENS, "temperature": 0}
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


def main(argv: list[str]) -> int:
    args = parser().parse_args(argv)
    task = " ".join(args.task).strip()
    as_json: bool = args.json

    def emit(event: dict[str, Any]) -> None:
        if as_json:
            print(json.dumps(event), flush=True)
        else:
            print(_human(event), flush=True)

    paths = RelayPaths.default()
    store = Store(paths)
    settings = local_ai_settings(settings_file())
    record = jsonl_recorder(paths.log)
    supervisor = Supervisor(store, idle=idle_seconds(settings), record=record)
    try:
        model_id = pick_model(store.installed().models, settings, args.model)
        spec = catalog.find(model_id)
        emit({"event": "loading", "model": model_id})
        start = time.monotonic()
        endpoint = supervisor.ensure(model_id)
        emit({"event": "ready", "model": model_id, "seconds": round(time.monotonic() - start, 1)})
    except RuntimeUnavailable as exc:
        supervisor.stop_all()
        emit({"event": "unavailable", "message": str(exc)})
        return 2

    try:
        with supervisor.task(model_id):
            agent = LocalAgent(
                _hands(),
                llama_model(endpoint.url),
                coords=spec.coords if spec else "rel1000",
                max_steps=args.max_steps,
                on_event=emit,
            )
            outcome = agent.run(task)
        record({"event": "agent", "model": model_id, "status": outcome.status})
        return 0 if outcome.status == "done" else 1
    except Exception as exc:  # report, never leave the model loaded
        emit({"event": "failed", "message": f"{type(exc).__name__}: {exc}", "steps": 0})
        return 1
    finally:
        # This process ends with the task, so nothing would unload the model later.
        supervisor.stop_all()


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
    if kind == "bad_reply":
        return f"    (unusable reply: {event['error']})"
    return f"{str(kind).capitalize()}: {event.get('message', '')}"


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
