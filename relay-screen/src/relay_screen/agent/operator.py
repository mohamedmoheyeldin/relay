"""The local operator as a service Claude can hand whole UI tasks to (D-069).

relay-screen's `ui_task` tool keeps one of these for the life of the MCP server: the
model loads on the first task and unloads after the idle time, like every local model
(D-042). Without a downloaded model it raises RuntimeUnavailable and Claude carries on
with the direct tools.
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from typing import Any, Protocol

from relay_screen.agent.local import Event, Hands, LocalAgent, Model, Outcome
from relay_screen.runtime import catalog, gpu
from relay_screen.runtime.paths import RelayPaths, local_ai_settings, settings_file
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import RuntimeUnavailable, Supervisor, jsonl_recorder

SETUP_MODEL = "qwen3-vl-8b-instruct"  # the planner the operator is tuned for (D-072)


class Operator(Protocol):
    def run(
        self, task: str, hands: Hands, max_steps: int, on_event: Event | None = None
    ) -> tuple[str, Outcome]: ...


class Setup:
    """Gets the local operator ready on its own: the llama.cpp runtime and one model,
    downloaded once in the background so a UI task never waits on the owner (D-079)."""

    def __init__(
        self,
        store: Store,
        model_id: str = SETUP_MODEL,
        record: Callable[[dict[str, Any]], None] = lambda event: None,
        spawn: Callable[[Callable[[], None]], None] | None = None,
    ) -> None:
        self.store = store
        self.model_id = model_id
        self.record = record
        self.spawn = spawn or (lambda work: threading.Thread(target=work, daemon=True).start())
        self.running = False
        self.error: str | None = None
        self._lock = threading.Lock()

    def missing(self) -> list[str]:
        installed = self.store.installed()
        need = [] if installed.runtime is not None else ["the local model runtime"]
        if not installed.models:
            need.append(f"the {self.model_id} model")
        return need

    def start(self) -> str:
        """Start the downloads if they aren't running; say what is happening."""
        need = self.missing()
        if not need:
            return "ready"
        with self._lock:
            begin = not self.running
            if begin:
                self.running, self.error = True, None
        if begin:
            self.record({"event": "operator_setup", "status": "started", "need": need})
            self.spawn(self._work)
        return "Relay is downloading " + " and ".join(need) + " for the local operator"

    def _work(self) -> None:
        try:
            if self.store.installed().runtime is None:
                info = gpu.query()
                self.store.install_runtime(None, info.cuda if info else None)
            if not self.store.installed().models:
                spec = catalog.find(self.model_id)
                if spec is None:
                    raise RuntimeUnavailable(f"{self.model_id} is not in the model catalog")
                self.store.install_model(spec)
            self.record({"event": "operator_setup", "status": "done"})
        except Exception as exc:  # reported on the next ui_task
            self.error = f"{exc.__class__.__name__}: {exc}"
            self.record({"event": "operator_setup", "status": "failed", "error": self.error})
        finally:
            with self._lock:
                self.running = False


class LocalOperator:
    """Picks the local model, keeps it loaded across tasks and runs `LocalAgent`."""

    def __init__(self, connect: Callable[[str], Model] | None = None) -> None:
        from relay_screen.agent.cli import idle_seconds, llama_model

        self.paths = RelayPaths.default()
        self.settings: dict[str, Any] = local_ai_settings(settings_file())
        self.record = jsonl_recorder(self.paths.log)
        self.store = Store(self.paths)
        self.supervisor = Supervisor(
            self.store, idle=idle_seconds(self.settings), record=self.record
        )
        self.supervisor.start_reaper()
        self.connect = connect or llama_model
        self.setup = Setup(self.store, record=self.record)
        self._lock = threading.Lock()  # one UI task at a time: there is one mouse

    def run(
        self, task: str, hands: Hands, max_steps: int, on_event: Event | None = None
    ) -> tuple[str, Outcome]:
        from relay_screen.agent.cli import pick_model

        if self.setup.missing():
            failed = self.setup.error
            status = self.setup.start()
            note = f" (the last try failed: {failed})" if failed else ""
            raise RuntimeUnavailable(
                f"{status}{note}. Do this task now with the direct tools; ui_task works "
                "once the download finishes."
            )
        with self._lock:
            model_id = pick_model(self.store.installed().models, self.settings, None)
            spec = catalog.find(model_id)
            with self.supervisor.task(model_id):
                model = self.connect(self.supervisor.ensure(model_id).url)
                agent = LocalAgent(
                    hands,
                    model,
                    coords=spec.coords if spec else "rel1000",
                    max_steps=max_steps,
                    on_event=on_event,
                )
                outcome = agent.run(task)
            self.record({"event": "ui_task", "model": model_id, "status": outcome.status})
            return model_id, outcome
