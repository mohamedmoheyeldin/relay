"""The local operator as a service Cloud AI can hand whole UI tasks to (D-069, D-084).

relay-screen's `ui_task` tool keeps one of these for the life of the MCP server.
One-move tasks ("go to dice.com", "open Notepad") run at once with no model. For the
rest the model loads on the first task and stays loaded across chat turns and
relay-screen processes for at least WARM_SECONDS (D-084). Without a downloaded model
it starts the download (D-079) and raises RuntimeUnavailable saying so.
"""

from __future__ import annotations

import threading
from collections.abc import Callable
from typing import Any, Protocol

from relay_screen.agent.local import Event, Hands, LocalAgent, Model, Outcome, run_instant
from relay_screen.runtime import catalog, gpu
from relay_screen.runtime.paths import RelayPaths, local_ai_settings, settings_file
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import (
    RuntimeUnavailable,
    Supervisor,
    jsonl_recorder,
    popen_detached,
)

SETUP_MODEL = "holo-3.1-9b"  # the operator's model (D-087)
WARM_SECONDS = 900.0  # the operator's model stays loaded at least this long (D-084)


class Operator(Protocol):
    def run(
        self,
        task: str,
        hands: Hands,
        max_steps: int,
        on_event: Event | None = None,
        *,
        inbox: Callable[[], list[str]] | None = None,
        cancelled: Callable[[], bool] | None = None,
        ask: Callable[[str], str] | None = None,
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
            self.store,
            idle=max(idle_seconds(self.settings), WARM_SECONDS),
            record=self.record,
            shared=self.paths.home / "servers",
            launch=popen_detached,
        )
        self.supervisor.start_reaper()
        self.connect = connect or llama_model
        self.setup = Setup(self.store, record=self.record)
        self._lock = threading.Lock()  # one UI task at a time: there is one mouse

    def warm(self) -> None:
        """Load the model before the first task, when it is downloaded (D-088)."""
        from relay_screen.agent.cli import pick_model

        if self.setup.missing():
            self.setup.start()
            return
        try:
            model_id = pick_model(self.store.installed().models, self.settings, None)
            with self._lock:
                self.supervisor.ensure(model_id)
        except RuntimeUnavailable as exc:
            self.record({"event": "warm", "status": "failed", "error": str(exc)})

    def run(
        self,
        task: str,
        hands: Hands,
        max_steps: int,
        on_event: Event | None = None,
        *,
        inbox: Callable[[], list[str]] | None = None,
        cancelled: Callable[[], bool] | None = None,
        ask: Callable[[str], str] | None = None,
    ) -> tuple[str, Outcome]:
        from relay_screen.agent.cli import pick_model

        emit = on_event or (lambda event: None)
        with self._lock:  # one-move tasks need no model, so no download either (D-084)
            done = run_instant(task, hands, emit)
        if done is not None:
            self.record({"event": "ui_task", "model": "instant", "status": done.status})
            return "instant", done
        if self.setup.missing():
            failed = self.setup.error
            status = self.setup.start()
            note = f" (the last try failed: {failed})" if failed else ""
            raise RuntimeUnavailable(
                f"{status}{note}. Tell the owner the local AI will be ready once the "
                "download finishes, then hand this task to ui_task again."
            )
        with self._lock:
            model_id = pick_model(self.store.installed().models, self.settings, None)
            spec = catalog.find(model_id)
            if model_id not in self.supervisor.servers:
                emit({"event": "note", "text": f"Loading the local model {model_id}…"})
            with self.supervisor.task(model_id):
                model = self.connect(self.supervisor.ensure(model_id).url)
                agent = LocalAgent(
                    hands,
                    model,
                    coords=spec.coords if spec else "rel1000",
                    max_steps=max_steps,
                    on_event=on_event,
                    instant=False,
                    inbox=inbox,
                    cancelled=cancelled,
                    ask=ask,
                )
                outcome = agent.run(task)
            self.record({"event": "ui_task", "model": model_id, "status": outcome.status})
            return model_id, outcome
