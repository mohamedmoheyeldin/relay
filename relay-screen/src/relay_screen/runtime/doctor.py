"""`relay-screen doctor`: is this PC ready to run Relay's local models and Cloud AI?"""

from __future__ import annotations

import json
import shutil
import subprocess
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal

from relay_screen import cli_names
from relay_screen.runtime import catalog, gpu
from relay_screen.runtime.download import free_space
from relay_screen.runtime.gpu import Gpu
from relay_screen.runtime.sources import MIN_CUDA
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import VRAM_MARGIN

Level = Literal["ok", "warn", "fail"]


@dataclass(frozen=True)
class Check:
    name: str
    level: Level
    detail: str


Run = Callable[[list[str]], str | None]
"""Run a command and return its stdout, or None if it is missing or fails."""


def run(command: list[str]) -> str | None:
    exe = shutil.which(command[0])
    if exe is None:
        return None
    try:
        done = subprocess.run(
            [exe, *command[1:]], capture_output=True, text=True, timeout=30, check=False
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done.stdout if done.returncode == 0 else None


def checks(store: Store, gpu_info: Gpu | None, run_command: Run = run) -> list[Check]:
    out: list[Check] = []
    installed = store.installed()

    if gpu_info is None:
        out.append(Check("GPU", "fail", "No NVIDIA GPU or driver found (NVML did not load)"))
    else:
        cuda = f"{gpu_info.cuda[0]}.{gpu_info.cuda[1]}"
        out.append(Check("GPU", "ok", f"{gpu_info.name}, driver {gpu_info.driver}"))
        cuda_ok = gpu_info.cuda >= MIN_CUDA
        out.append(
            Check(
                "CUDA",
                "ok" if cuda_ok else "fail",
                f"driver supports CUDA {cuda}"
                + ("" if cuda_ok else "; RTX 50 needs 12.8 or newer, update the driver"),
            )
        )
        out.append(
            Check("VRAM", "ok", f"{gpu.gib(gpu_info.free)} free of {gpu.gib(gpu_info.total)}")
        )

    runtime = installed.runtime
    if runtime is None:
        hint = "llama.cpp not installed (relay-screen runtime install)"
        out.append(Check("Runtime", "warn", hint))
    elif store.server_exe() is None:
        out.append(Check("Runtime", "fail", f"llama.cpp {runtime.tag} is recorded but missing"))
    else:
        major, minor = (int(p) for p in runtime.cuda.split("."))
        too_new = gpu_info is not None and (major, minor) > gpu_info.cuda
        out.append(
            Check(
                "Runtime",
                "fail" if too_new else "ok",
                f"llama.cpp {runtime.tag}, CUDA {runtime.cuda}"
                + ("; newer than the driver supports" if too_new else ""),
            )
        )

    out.append(
        Check(
            "Models folder",
            "ok",
            f"{store.paths.models} ({free_space(store.paths.models) / 1e9:.0f} GB free on disk)",
        )
    )
    known = {m.id for m in catalog.CATALOG}
    for spec in catalog.CATALOG:
        out.append(_model_check(store, spec.id, spec.name, gpu_info))
    for model_id in sorted(set(installed.models) - known):
        out.append(_model_check(store, model_id, model_id, gpu_info))

    pair = [installed.models.get(m) for m in (catalog.DEFAULT_LOCATE, catalog.DEFAULT_CHECK)]
    if gpu_info is not None and all(pair):
        need = sum(m.size for m in pair if m) + 2 * VRAM_MARGIN
        fits = need <= gpu_info.free
        out.append(
            Check(
                "Resident pair",
                "ok" if fits else "warn",
                f"both models need about {gpu.gib(need)}; {gpu.gib(gpu_info.free)} free now"
                + ("" if fits else ", so only the grounding model will load"),
            )
        )

    out.extend(_cloud(run_command))
    return out


def _model_check(store: Store, model_id: str, name: str, gpu_info: Gpu | None) -> Check:
    model = store.installed().models.get(model_id)
    label = f"Model {name}"
    if model is None:
        return Check(label, "warn", "not downloaded")
    if store.model_files(model_id) is None:
        return Check(label, "fail", "files missing; download it again")
    detail = f"{model.quant}, {gpu.gib(model.size)}"
    if gpu_info is not None and model.size + VRAM_MARGIN > gpu_info.total:
        return Check(label, "fail", detail + ", larger than this GPU's VRAM")
    return Check(label, "ok", detail)


def _cloud(run_command: Run) -> list[Check]:
    version = run_command([cli_names.COMMAND, "--version"])
    if version is None:
        return [Check("Cloud AI", "fail", "the Cloud AI command was not found")]
    out = [Check("Cloud AI", "ok", version.strip().splitlines()[0])]
    # Only the logged-in flag and method are read; Relay never touches tokens (D-002).
    status = run_command([cli_names.COMMAND, "auth", "status"])
    try:
        data = json.loads(status or "")
    except ValueError:
        data = None
    if not isinstance(data, dict):
        out.append(Check("Cloud AI sign-in", "warn", "could not read the sign-in state"))
    elif data.get("loggedIn"):
        method = str(data.get("authMethod", "")).replace("_", " ")
        detail = f"signed in ({method})" if method else "signed in"
        out.append(Check("Cloud AI sign-in", "ok", detail))
    else:
        out.append(
            Check("Cloud AI sign-in", "fail", "not signed in (sign in from Relay's Settings)")
        )
    return out


def render(results: list[Check]) -> str:
    marks = {"ok": "ok  ", "warn": "warn", "fail": "FAIL"}
    width = max(len(c.name) for c in results)
    lines = [f"{marks[c.level]}  {c.name.ljust(width)}  {c.detail}" for c in results]
    failed = sum(c.level == "fail" for c in results)
    lines.append("Ready." if not failed else f"{failed} problem(s) to fix.")
    return "\n".join(lines)
