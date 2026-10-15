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
from relay_screen.runtime import catalog
from relay_screen.runtime.paths import RelayPaths, local_ai_settings, settings_file
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import Supervisor, jsonl_recorder


class Operator(Protocol):
    def run(
        self, task: str, hands: Hands, max_steps: int, on_event: Event | None = None
    ) -> tuple[str, Outcome]: ...


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
        self._lock = threading.Lock()  # one UI task at a time: there is one mouse

    def run(
        self, task: str, hands: Hands, max_steps: int, on_event: Event | None = None
    ) -> tuple[str, Outcome]:
        from relay_screen.agent.cli import pick_model

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
