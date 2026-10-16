"""Start llama-server on demand, one process per model, and stop it when idle.

D-006 and D-042: a model loads when a task that may need it starts (or on its first
call) and its process stops 60 s after the task ends, which frees all of its VRAM.
Before a load, NVML must report enough free VRAM; Relay never lets a model spill into
system RAM, so with no NVIDIA GPU or too little VRAM it refuses and Claude does the
looking instead. D-036: several models can be resident at once.

D-084: the local operator runs inside relay-screen, and Claude starts a new
relay-screen for every chat turn, so a model owned by one process reloaded on each
turn. With `shared` set, the server is started outside the process's job, its port
and pid go in a small record file, and later processes adopt the running server
instead of loading the model again. Whoever finds it idle past `idle` stops it.
"""

from __future__ import annotations

import contextlib
import functools
import json
import os
import re
import signal
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
CREATE_NEW_PROCESS_GROUP = 0x00000200
CREATE_BREAKAWAY_FROM_JOB = 0x01000000


class RuntimeUnavailable(RuntimeError):
    """The model can't run locally right now; the message says why."""


class Process(Protocol):
    def poll(self) -> int | None: ...

    def terminate(self) -> None: ...

    def kill(self) -> None: ...

    def wait(self, timeout: float | None = None) -> int: ...


Launch = Callable[[list[str], Path], Process]
"""Start a command, sending its output to the log file at the given path."""


# Pinned (page-locked) host buffers count fully against RAM and can't be paged out;
# with every layer in VRAM they only speed up a few small copies (D-107).
SERVER_ENV = {"GGML_CUDA_NO_PINNED": "1"}


def _server_env() -> dict[str, str]:
    return {**os.environ, **SERVER_ENV}


def _open_log(log: Path) -> Any:
    """Append, never truncate: a second load must not empty the log of a server that
    is still running (round 5 found it at 0 bytes)."""
    log.parent.mkdir(parents=True, exist_ok=True)
    out = log.open("ab")
    out.write(f"\n--- started {time.strftime('%Y-%m-%d %H:%M:%S')} ---\n".encode())
    out.flush()
    return out


def popen(command: list[str], log: Path) -> Process:
    out = _open_log(log)
    flags = CREATE_NO_WINDOW if sys.platform == "win32" else 0
    try:
        return subprocess.Popen(
            command,
            stdout=out,
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            creationflags=flags,
            env=_server_env(),
        )
    finally:
        out.close()


@functools.lru_cache(maxsize=8)
def server_help(exe: str) -> str:
    """`llama-server --help`, to pass only flags this build knows (D-102)."""
    flags = CREATE_NO_WINDOW if sys.platform == "win32" else 0
    try:
        done = subprocess.run(
            [exe, "--help"], capture_output=True, timeout=20, creationflags=flags, check=False
        )
    except (OSError, subprocess.SubprocessError):
        return ""
    return (done.stdout + done.stderr).decode("utf-8", "replace")


def ram_flags(help_text: str) -> list[str]:
    """Keep the weights out of RAM and the host prompt cache off (D-100), in the words
    of this llama.cpp build: newer ones replaced --no-mmap with --load-mode.

    On --load-mode builds "none" reads the whole file into a heap buffer first, which
    the round-5 run measured at 9.8 GB private; DirectIO ("dio") streams it to the GPU
    without that copy or the file cache (D-105). RELAY_LLAMA_LOAD_MODE overrides it so
    a run on the PC can compare modes without a new build."""
    flags: list[str] = []
    if "--load-mode" in help_text:
        mode = os.environ.get("RELAY_LLAMA_LOAD_MODE") or ("dio" if "dio:" in help_text else "mmap")
        flags += ["--load-mode", mode]
    elif "--no-mmap" in help_text:
        flags.append("--no-mmap")
    if "--cache-ram" in help_text:
        flags += ["--cache-ram", "0"]
    return flags


def log_flags(help_text: str, log: Path) -> list[str]:
    """llama.cpp's own log file, flushed line by line, beside the captured output."""
    if "--log-file" not in help_text:
        return []
    return ["--log-file", str(log.with_suffix(".server.log"))]


def popen_detached(command: list[str], log: Path) -> Process:
    """Like `popen`, but the server outlives the relay-screen process that started it.

    On Windows it leaves the caller's job object when the job allows that, so closing
    a chat turn's relay-screen doesn't take the loaded model with it (D-084).
    """
    if sys.platform != "win32":
        return popen(command, log)
    out = _open_log(log)
    base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
    try:
        for flags in (base | CREATE_BREAKAWAY_FROM_JOB, base):
            try:
                return subprocess.Popen(
                    command,
                    stdout=out,
                    stderr=subprocess.STDOUT,
                    stdin=subprocess.DEVNULL,
                    creationflags=flags,
                    env=_server_env(),
                )
            except OSError:  # the job doesn't allow breakaway
                if flags == base:
                    raise
        raise AssertionError("unreachable")
    finally:
        out.close()


def kill_pid(pid: int) -> None:
    with contextlib.suppress(OSError):
        os.kill(pid, signal.SIGTERM)  # TerminateProcess on Windows


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


class Adopted:
    """A llama-server another relay-screen process started, reached by its port (D-084)."""

    def __init__(
        self, pid: int, port: int, health: Callable[[int], bool], kill: Callable[[int], None]
    ) -> None:
        self.pid = pid
        self.port = port
        self._health = health
        self._kill = kill
        self._gone = False

    def poll(self) -> int | None:
        return 0 if self._gone or not self._health(self.port) else None

    def terminate(self) -> None:
        self._kill(self.pid)
        self._gone = True

    def kill(self) -> None:
        self.terminate()

    def wait(self, timeout: float | None = None) -> int:
        return 0


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
        shared: Path | None = None,
        wall: Callable[[], float] = time.time,
        kill: Callable[[int], None] = kill_pid,
        help: Callable[[str], str] = server_help,
    ) -> None:
        self.store = store
        self.help = help
        self.shared = shared
        self.wall = wall
        self.kill = kill
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
                self._touch(server)
                return Endpoint(model_id, server.url, None)
            self.servers.pop(model_id, None)
            server = self._adopt(model_id)
            if server is not None:
                self.servers[model_id] = server
                self._touch(server)
                self.record({"event": "adopt", "model": model_id, "port": server.port})
                return Endpoint(model_id, server.url, None)
            start = self.clock()
            server = self._load(model_id)
            self.servers[model_id] = server
            self._touch(server)
            took = self.clock() - start
            self.record({"event": "load", "model": model_id, "seconds": round(took, 2)})
            return Endpoint(model_id, server.url, took)

    def _record_file(self, model_id: str) -> Path | None:
        if self.shared is None:
            return None
        return self.shared / f"{model_id}.json"

    def _read_record(self, model_id: str) -> dict[str, Any] | None:
        path = self._record_file(model_id)
        if path is None:
            return None
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None
        return data if isinstance(data, dict) else None

    def _adopt(self, model_id: str) -> Server | None:
        """The server another process left running for `model_id`, if it still answers."""
        data = self._read_record(model_id)
        if data is None:
            return None
        port, pid = data.get("port"), data.get("pid")
        if not isinstance(port, int) or not isinstance(pid, int) or not self.health(port):
            self._forget(model_id)
            return None
        now = self.clock()
        return Server(model_id, port, Adopted(pid, port, self.health, self.kill), now, now)

    def _touch(self, server: Server) -> None:
        path = self._record_file(server.model_id)
        pid = getattr(server.process, "pid", None)
        if path is None or not isinstance(pid, int):
            return
        record = {"model": server.model_id, "port": server.port, "pid": pid, "used": self.wall()}
        with contextlib.suppress(OSError):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(record), encoding="utf-8")

    def _forget(self, model_id: str) -> None:
        path = self._record_file(model_id)
        if path is not None:
            with contextlib.suppress(OSError):
                path.unlink()

    def _shared_idle(self, model_id: str) -> bool:
        """With a shared server, idle means no process used it for `idle` seconds."""
        data = self._read_record(model_id)
        used = data.get("used") if data else None
        return not isinstance(used, int | float) or self.wall() - used >= self.idle

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
                for model_id in model_ids:
                    if model_id in self.servers:
                        self._touch(self.servers[model_id])

    def reap(self) -> list[str]:
        """Stop servers that have been idle long enough, or that died; returns their ids."""
        stopped: list[str] = []
        with self._lock:
            now = self.clock()
            for model_id, server in list(self.servers.items()):
                died = server.process.poll() is not None
                idle = self.tasks == 0 and now - max(server.last_used, self.task_ended) >= self.idle
                if idle and self.shared is not None:
                    idle = self._shared_idle(model_id)
                if died or idle:
                    self._stop(server)
                    del self.servers[model_id]
                    self._forget(model_id)
                    stopped.append(model_id)
                    self.record({"event": "exited" if died else "unload", "model": model_id})
            stopped.extend(self._reap_left_behind())
        return stopped

    def _reap_left_behind(self) -> list[str]:
        """Stop shared servers no running process holds once they are idle (D-084)."""
        if self.shared is None or self.tasks or not self.shared.is_dir():
            return []
        stopped: list[str] = []
        for path in sorted(self.shared.glob("*.json")):
            model_id = path.stem
            if model_id in self.servers or not self._shared_idle(model_id):
                continue
            data = self._read_record(model_id) or {}
            if isinstance(data.get("pid"), int):
                self.kill(data["pid"])
            self._forget(model_id)
            stopped.append(model_id)
            self.record({"event": "unload", "model": model_id})
        return stopped

    def stop_all(self) -> None:
        self._stopping.set()
        with self._lock:
            for server in self.servers.values():
                self._stop(server)
                self._forget(server.model_id)
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
        log = self.store.paths.home / "logs" / f"llama-{model_id}.log"
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
            "--jinja",  # the model's own chat template, so enable_thinking applies
            # RAM (D-100): the weights live in VRAM, so don't also keep the file mapped in
            # RAM, and turn off the host-memory prompt cache (several GB by default).
            *ram_flags(self.help(str(exe))),
            *log_flags(self.help(str(exe)), log),
        ]  # fmt: skip
        process = self.launch(command, log)
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


def chat(url: str, prompt: str, max_tokens: int = 32, timeout: float = 120) -> str:
    """A one-shot text chat completion against a llama-server endpoint.

    Thinking is turned off the same way the operator does it (D-087): a thinking
    model otherwise spends the whole budget inside <think> and answers nothing.
    """
    body = json.dumps(
        {
            "messages": [{"role": "user", "content": prompt}],
            "max_tokens": max_tokens,
            "temperature": 0,
            "chat_template_kwargs": {"enable_thinking": False},
        }
    ).encode()
    request = urllib.request.Request(
        f"{url}/v1/chat/completions", body, {"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        data = json.load(response)
    content = data["choices"][0]["message"].get("content") or ""
    return re.sub(r"<think>.*?(</think>|$)", "", content, flags=re.DOTALL).strip()


def jsonl_recorder(path: Path) -> Callable[[dict[str, Any]], None]:
    """Append runtime events to a JSON-lines log with a wall-clock timestamp."""

    def record(event: dict[str, Any]) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        line = {"time": time.strftime("%Y-%m-%dT%H:%M:%S"), **event}
        with path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(line) + "\n")

    return record
