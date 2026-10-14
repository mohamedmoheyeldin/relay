"""Command line for the local model runtime.

    relay-screen doctor [--verify]
    relay-screen runtime install [--tag bNNNN]
    relay-screen runtime try MODEL [--no-wait]
    relay-screen models [list]
    relay-screen models download MODEL|HF_REPO [--quant Q4_K_M]
    relay-screen models remove MODEL

Files go under RELAY_HOME (default %LOCALAPPDATA%\\Relay); models go to the models
folder from Settings (D-046). Nothing here opens a window.
"""

from __future__ import annotations

import argparse
import sys
import time

from relay_screen.runtime import catalog, doctor, gpu
from relay_screen.runtime.catalog import ModelSpec
from relay_screen.runtime.download import DownloadError
from relay_screen.runtime.paths import RelayPaths
from relay_screen.runtime.sources import SourceError
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import (
    IDLE_SECONDS,
    RuntimeUnavailable,
    Supervisor,
    chat,
    jsonl_recorder,
)

COMMANDS = ("doctor", "runtime", "models")


class _Progress:
    def __init__(self) -> None:
        self.last = 0.0

    def __call__(self, done: int, total: int | None) -> None:
        now = time.monotonic()
        if now - self.last < 0.5 and done != total:
            return
        self.last = now
        size = f"{done / 1e9:.2f}" + (f" / {total / 1e9:.2f} GB" if total else " GB")
        pct = f" ({100 * done / total:.0f}%)" if total else ""
        end = "\n" if total and done >= total else ""
        print(f"\r  {size}{pct}   ", end=end, flush=True)


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="relay-screen", description="Relay's local model runtime.")
    sub = p.add_subparsers(dest="command", required=True)
    d = sub.add_parser("doctor", help="check GPU, runtime, models and Claude Code")
    d.add_argument("--verify", action="store_true", help="re-hash every model file")

    r = sub.add_parser("runtime", help="the llama.cpp runtime").add_subparsers(
        dest="action", required=True
    )
    install = r.add_parser("install", help="download and verify llama.cpp (CUDA)")
    install.add_argument("--tag", help="a llama.cpp release tag such as b6500 (default: latest)")
    try_ = r.add_parser("try", help="load a model, time the first answer, wait for the unload")
    try_.add_argument("model")
    try_.add_argument("--no-wait", action="store_true", help="stop right after the answer")

    m = sub.add_parser("models", help="local vision models")
    ms = m.add_subparsers(dest="action")
    ms.add_parser("list", help="the catalog and what is downloaded")
    dl = ms.add_parser("download", help="download a catalog model or a Hugging Face GGUF repo")
    dl.add_argument("model")
    dl.add_argument("--quant", help="e.g. Q4_K_M, Q5_K_M, Q8_0")
    rm = ms.add_parser("remove", help="delete a downloaded model")
    rm.add_argument("model")
    return p


def main(argv: list[str]) -> int:
    args = parser().parse_args(argv)
    paths = RelayPaths.default()
    store = Store(paths, progress=_Progress())
    try:
        if args.command == "doctor":
            return _doctor(store, args.verify)
        if args.command == "runtime" and args.action == "install":
            info = gpu.query()
            runtime = store.install_runtime(args.tag, info.cuda if info else None)
            print(f"Installed llama.cpp {runtime.tag} (CUDA {runtime.cuda}) in {paths.runtime}")
            return 0
        if args.command == "runtime" and args.action == "try":
            return _try(store, args.model, wait=not args.no_wait)
        if args.command == "models" and args.action == "download":
            spec = _spec(args.model, args.quant)
            model = store.install_model(spec, args.quant)
            print(f"Downloaded {model.id} ({model.quant}, {gpu.gib(model.size)}) to {paths.models}")
            return 0
        if args.command == "models" and args.action == "remove":
            removed = store.remove_model(args.model)
            print(f"Removed {args.model}." if removed else f"{args.model} is not downloaded.")
            return 0 if removed else 1
        return _list(store)
    except (DownloadError, SourceError, RuntimeUnavailable, OSError) as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


def _spec(name: str, quant: str | None) -> ModelSpec:
    spec = catalog.find(name)
    if spec is not None:
        return spec
    if "/" in name:
        return catalog.custom(name, quant or "Q4_K_M")
    known = ", ".join(m.id for m in catalog.CATALOG)
    raise SourceError(f"Unknown model {name!r}; use one of {known} or a Hugging Face repo")


def _doctor(store: Store, verify: bool) -> int:
    results = doctor.checks(store, gpu.query())
    if verify:
        bad = store.verify()
        results.append(
            doctor.Check(
                "Model files",
                "fail" if bad else "ok",
                f"changed on disk: {', '.join(bad)}" if bad else "every file matches its SHA-256",
            )
        )
    print(doctor.render(results))
    return 1 if any(c.level == "fail" for c in results) else 0


def _list(store: Store) -> int:
    installed = store.installed().models
    defaults = {catalog.DEFAULT_LOCATE, catalog.DEFAULT_CHECK}
    for spec in catalog.CATALOG:
        have = installed.get(spec.id)
        state = f"downloaded {have.quant}, {gpu.gib(have.size)}" if have else "not downloaded"
        if spec.repo is None and not have:
            state += " (source picked in M1-11)"
        mark = " · default" if spec.id in defaults else ""
        print(f"{spec.id:24} {catalog.ROLE_LABEL[spec.role]:18} {state}{mark}")
    for model_id in sorted(set(installed) - {m.id for m in catalog.CATALOG}):
        have = installed[model_id]
        print(f"{model_id:24} {'custom':18} downloaded {have.quant}, {gpu.gib(have.size)}")
    return 0


def _try(store: Store, model_id: str, wait: bool) -> int:
    """M1-09 acceptance: cold start and first answer timed, then VRAM back after idle."""
    record = jsonl_recorder(store.paths.log)
    supervisor = Supervisor(store, record=record)
    before = gpu.free_vram()
    start = time.monotonic()
    endpoint = supervisor.ensure(model_id)
    loaded = gpu.free_vram()
    answer = chat(endpoint.url, "Reply with the single word OK.")
    first = time.monotonic() - start
    record({"event": "first_answer", "model": model_id, "seconds": round(first, 2)})
    cold = endpoint.cold_start or 0
    print(f"Loaded {model_id} in {cold:.1f} s; first answer after {first:.1f} s")
    print(f"Answer: {answer.strip()!r}")
    if before is not None and loaded is not None:
        print(f"VRAM used by the model: {gpu.gib(before - loaded)}")
    if not wait:
        supervisor.stop_all()
        return 0
    print(f"Waiting for the {IDLE_SECONDS:.0f} s idle unload...")
    supervisor.start_reaper()
    while supervisor.servers:
        time.sleep(1)
    after = gpu.free_vram()
    supervisor.stop_all()
    if before is not None and after is not None:
        back = after >= before - int(0.2 * gpu.GIB)
        print(f"Unloaded; {gpu.gib(after)} free ({'all VRAM back' if back else 'VRAM not back'})")
        return 0 if back else 1
    print("Unloaded.")
    return 0
