"""Start llama-server on demand, one process per model, and stop it when idle.

D-006 and D-042: a model loads when a task that may need it starts (or on its first
call) and its process stops 60 s after the task ends, which frees all of its VRAM.
Before a load, NVML must report enough free VRAM; Relay never lets a model spill into
system RAM, so with no NVIDIA GPU or too little VRAM it refuses and Claude does the
looking instead. D-036: several models can be resident at once.
"""

from __future__ import annotations

import json
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

from relay_screen.runtime import gpu
from relay_screen.runtime.store import Store

IDLE_SECONDS = 60.0
LOAD_TIMEOUT = 180.0
REAP_EVERY = 5.0
# KV cache, CUDA context and compute buffers on top of the weights.
VRAM_MARGIN = int(1.5 * gpu.GIB)
CREATE_NO_WINDOW = 0x08000000


class RuntimeUnavailable(RuntimeError):
    """The model can't run locally right now; the message says why."""


class Process(Protocol):
    def poll(self) -> int | None: ...

    def terminate(self) -> None: ...

    def kill(self) -> None: ...

    def wait(self, timeout: float | None = None) -> int: ...


Launch = Callable[[list[str], Path], Process]
"""Start a command, sending its output to the log file at the given path."""


def popen(command: list[str], log: Path) -> Process:
    log.parent.mkdir(parents=True, exist_ok=True)
    out = log.open("wb")
    flags = CREATE_NO_WINDOW if sys.platform == "win32" else 0
    try:
        return subprocess.Popen(
            command,
            stdout=out,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            creationflags=flags,
        )
    finally:
        out.close()


def http_health(port: int) -> bool:
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
            return r.status == 200
    except (urllib.error.URLError, OSError):
        return False


def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@dataclass
class Server:
    model_id: str
    port: int
    process: Process
    started: float
    last_used: float

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"


@dataclass(frozen=True)
class Endpoint:
    model_id: str
    url: str
    cold_start: float | None
    """Seconds this call spent loading the model; None when it was already loaded."""


class Supervisor:
    def __init__(
        self,
        store: Store,
        *,
        launch: Launch = popen,
        health: Callable[[int], bool] = http_health,
        vram: Callable[[], int | None] = gpu.free_vram,
        port: Callable[[], int] = free_port,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
        idle: float = IDLE_SECONDS,
        load_timeout: float = LOAD_TIMEOUT,
        record: Callable[[dict[str, Any]], None] | None = None,
    ) -> None:
        self.store = store
        self.launch = launch
        self.health = health
        self.vram = vram
        self.port = port
        self.clock = clock
        self.sleep = sleep
        self.idle = idle
        self.load_timeout = load_timeout
        self.record = record or (lambda event: None)
        self.servers: dict[str, Server] = {}
        self.tasks = 0
        self.task_ended = 0.0
        self._lock = threading.RLock()
        self._reaper: threading.Thread | None = None
        self._stopping = threading.Event()

    def ensure(self, model_id: str) -> Endpoint:
        """The running server for `model_id`, loading it first if needed."""
        with self._lock:
            server = self.servers.get(model_id)
            if server is not None and server.process.poll() is None:
                server.last_used = self.clock()
                return Endpoint(model_id, server.url, None)
            self.servers.pop(model_id, None)
            start = self.clock()
            server = self._load(model_id)
            self.servers[model_id] = server
            took = self.clock() - start
            self.record({"event": "load", "model": model_id, "seconds": round(took, 2)})
            return Endpoint(model_id, server.url, took)

    @contextmanager
    def task(self, *model_ids: str) -> Iterator[None]:
        """Hold models loaded for a whole task; they unload `idle` s after it ends (D-042)."""
        with self._lock:
            self.tasks += 1
        try:
            for model_id in model_ids:
                self.ensure(model_id)
            yield
        finally:
            with self._lock:
                self.tasks -= 1
                self.task_ended = self.clock()

    def reap(self) -> list[str]:
        """Stop servers that have been idle long enough, or that died; returns their ids."""
        stopped: list[str] = []
        with self._lock:
            now = self.clock()
            for model_id, server in list(self.servers.items()):
                died = server.process.poll() is not None
                idle = self.tasks == 0 and now - max(server.last_used, self.task_ended) >= self.idle
                if died or idle:
                    self._stop(server)
                    del self.servers[model_id]
                    stopped.append(model_id)
                    self.record({"event": "exited" if died else "unload", "model": model_id})
        return stopped

    def stop_all(self) -> None:
        self._stopping.set()
        with self._lock:
            for server in self.servers.values():
                self._stop(server)
                self.record({"event": "unload", "model": server.model_id})
            self.servers.clear()

    def start_reaper(self) -> None:
        """Check for idle servers every few seconds on a daemon thread."""
        if self._reaper is not None:
            return

        def loop() -> None:
            while not self._stopping.wait(REAP_EVERY):
                self.reap()

        self._reaper = threading.Thread(target=loop, name="relay-model-reaper", daemon=True)
        self._reaper.start()

    def _load(self, model_id: str) -> Server:
        exe = self.store.server_exe()
        if exe is None:
            raise RuntimeUnavailable("The local model runtime is not installed")
        files = self.store.model_files(model_id)
        model = self.store.installed().models.get(model_id)
        if files is None or model is None:
            raise RuntimeUnavailable(f"Model {model_id} is not downloaded")
        weights, mmproj = files

        free = self.vram()
        need = model.size + VRAM_MARGIN
        if free is None:
            raise RuntimeUnavailable("No NVIDIA GPU found; local models only run in VRAM")
        if free < need:
            raise RuntimeUnavailable(
                f"Not enough free VRAM for {model_id}: needs {gpu.gib(need)}, {gpu.gib(free)} free"
            )

        port = self.port()
        command = [
            str(exe),
            "--model", str(weights),
            *(["--mmproj", str(mmproj)] if mmproj else []),
            "--n-gpu-layers", "999",
            "--flash-attn", "on",
            "--ctx-size", str(model.context),
            "--cache-type-k", "q8_0",
            "--cache-type-v", "q8_0",
            "--parallel", "1",
            "--host", "127.0.0.1",
            "--port", str(port),
            "--alias", model_id,
        ]  # fmt: skip
        process = self.launch(command, self.store.paths.home / "logs" / f"llama-{model_id}.log")
        started = self.clock()
        while self.clock() - started < self.load_timeout:
            if process.poll() is not None:
                raise RuntimeUnavailable(
                    f"llama-server exited while loading {model_id}; see logs/llama-{model_id}.log"
                )
            if self.health(port):
                now = self.clock()
                return Server(model_id, port, process, now, now)
            self.sleep(0.25)
        self._stop_process(process)
        raise RuntimeUnavailable(f"{model_id} did not load within {self.load_timeout:.0f} s")

    def _stop(self, server: Server) -> None:
        self._stop_process(server.process)

    @staticmethod
    def _stop_process(process: Process) -> None:
        if process.poll() is not None:
            return
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def chat(url: str, prompt: str, max_tokens: int = 16, timeout: float = 120) -> str:
    """A one-shot text chat completion against a llama-server endpoint."""
    body = json.dumps(
        {
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": 0,
        }
    ).encode()
    request = urllib.request.Request(
        f"{url}/v1/chat/completions", body, {"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        data = json.load(response)
    return data["choices"][0]["message"]["content"]


def jsonl_recorder(path: Path) -> Callable[[dict[str, Any]], None]:
    """Append runtime events to a JSON-lines log with a wall-clock timestamp."""

    def record(event: dict[str, Any]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        line = {"time": time.strftime("%Y-%m-%dT%H:%M:%S"), **event}
        with path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line) + "\n")

    return record
