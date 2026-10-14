from __future__ import annotations

import hashlib
import io
import json
import zipfile
from pathlib import Path
from typing import Any

import pytest

from relay_screen.runtime import catalog, doctor, sources
from relay_screen.runtime.download import DownloadError, download
from relay_screen.runtime.gpu import GIB, Gpu
from relay_screen.runtime.paths import RelayPaths, models_folder_setting
from relay_screen.runtime.store import Store
from relay_screen.runtime.supervisor import RuntimeUnavailable, Supervisor


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class FakeResponse:
    def __init__(self, data: bytes, status: int) -> None:
        self.body = io.BytesIO(data)
        self.status = status
        self.length = len(data)

    def read(self, amt: int = -1, /) -> bytes:
        return self.body.read(amt)

    def getheader(self, name: str, default: str | None = None) -> str | None:
        return str(self.length) if name == "Content-Length" else default

    def close(self) -> None:
        pass


class FakeWeb:
    """Serves files by URL; honours Range unless `ranges` is False."""

    def __init__(self, files: dict[str, bytes], ranges: bool = True) -> None:
        self.files = files
        self.ranges = ranges
        self.requests: list[tuple[str, int]] = []

    def __call__(self, url: str, offset: int) -> FakeResponse:
        self.requests.append((url, offset))
        data = self.files[url]
        if offset and self.ranges:
            return FakeResponse(data[offset:], 206)
        return FakeResponse(data, 200)


DATA = bytes(range(256)) * 9000  # a few chunks


# --- download ---------------------------------------------------------------


def test_download_verifies_and_renames(tmp_path: Path) -> None:
    web = FakeWeb({"u": DATA})
    seen: list[int] = []
    dest = tmp_path / "f.bin"
    got = download(
        "u", dest, sha(DATA), size=len(DATA), opener=web, progress=lambda d, t: seen.append(d)
    )
    assert got == sha(DATA)
    assert dest.read_bytes() == DATA
    assert not (tmp_path / "f.bin.part").exists()
    assert seen[0] == 0 and seen[-1] == len(DATA)


def test_download_resumes_a_partial_file(tmp_path: Path) -> None:
    dest = tmp_path / "f.bin"
    (tmp_path / "f.bin.part").write_bytes(DATA[:1000])
    web = FakeWeb({"u": DATA})
    download("u", dest, sha(DATA), size=len(DATA), opener=web)
    assert web.requests == [("u", 1000)]
    assert dest.read_bytes() == DATA


def test_download_starts_over_when_the_server_ignores_range(tmp_path: Path) -> None:
    dest = tmp_path / "f.bin"
    (tmp_path / "f.bin.part").write_bytes(DATA[:1000])
    download("u", dest, sha(DATA), opener=FakeWeb({"u": DATA}, ranges=False))
    assert dest.read_bytes() == DATA


def test_download_mismatch_deletes_the_partial_file(tmp_path: Path) -> None:
    dest = tmp_path / "f.bin"
    with pytest.raises(DownloadError, match="SHA-256 mismatch"):
        download("u", dest, "0" * 64, opener=FakeWeb({"u": DATA}))
    assert not dest.exists()
    assert not (tmp_path / "f.bin.part").exists()


def test_download_skips_a_verified_file(tmp_path: Path) -> None:
    dest = tmp_path / "f.bin"
    dest.write_bytes(DATA)
    web = FakeWeb({"u": DATA})
    download("u", dest, sha(DATA), opener=web)
    assert web.requests == []


# --- sources ----------------------------------------------------------------


def asset(name: str, data: bytes) -> dict[str, Any]:
    return {
        "name": name,
        "size": len(data),
        "digest": f"sha256:{sha(data)}",
        "browser_download_url": f"https://gh/{name}",
    }


def release(tag: str, files: dict[str, bytes]) -> dict[str, Any]:
    return {"tag_name": tag, "assets": [asset(n, d) for n, d in files.items()]}


def zip_bytes(entries: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as z:
        for name, data in entries.items():
            z.writestr(name, data)
    return buffer.getvalue()


LLAMA = zip_bytes({"llama-server.exe": b"MZ server", "ggml-cuda.dll": b"dll"})
CUDART = zip_bytes({"cudart64_13.dll": b"cudart"})
RELEASE = release(
    "b7000",
    {
        "llama-b7000-bin-win-cuda-12.4-x64.zip": b"old",
        "llama-b7000-bin-win-cuda-12.8-x64.zip": b"mid",
        "llama-b7000-bin-win-cuda-13.1-x64.zip": LLAMA,
        "cudart-llama-bin-win-cuda-13.1-x64.zip": CUDART,
        "llama-b7000-bin-win-cpu-x64.zip": b"cpu",
    },
)


RELEASES = [release("v0.6.0", {"llama-v0.6.0.tar.gz": b"src"}), RELEASE]
"""What GitHub lists: a newer tag without Windows binaries, then the bNNNN build."""


def test_llama_release_picks_the_newest_cuda_the_driver_supports() -> None:
    got = sources.llama_release(lambda url: RELEASES)
    assert got.tag == "b7000" and got.cuda == (13, 1)
    assert [f.name for f in got.files] == [
        "llama-b7000-bin-win-cuda-13.1-x64.zip",
        "cudart-llama-bin-win-cuda-13.1-x64.zip",
    ]
    older = sources.llama_release(lambda url: RELEASES, driver_cuda=(12, 9))
    assert older.cuda == (12, 8)
    with pytest.raises(sources.SourceError, match=r"12\.8"):
        sources.llama_release(lambda url: RELEASES, driver_cuda=(12, 6))


def test_llama_release_by_tag() -> None:
    urls: list[str] = []

    def get(url: str) -> Any:
        urls.append(url)
        return RELEASE

    assert sources.llama_release(get, "b7000").tag == "b7000"
    assert urls == ["https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b7000"]
    with pytest.raises(sources.SourceError, match=r"llama\.cpp v0\.6\.0 has no Windows"):
        sources.llama_release(lambda url: RELEASES[0], "v0.6.0")


def test_llama_release_needs_a_published_digest() -> None:
    bare = {"tag_name": "b1", "assets": [{**asset("llama-b1-bin-win-cuda-13.0-x64.zip", b"x")}]}
    del bare["assets"][0]["digest"]
    with pytest.raises(sources.SourceError, match="SHA-256"):
        sources.llama_release(lambda url: [bare])


WEIGHTS = b"gguf weights" * 100
PROJECTOR = b"gguf mmproj" * 10


def hf_entry(path: str, data: bytes) -> dict[str, Any]:
    return {
        "type": "file",
        "path": path,
        "size": len(data),
        "lfs": {"oid": sha(data), "size": len(data)},
    }


def hf_api(repo: str) -> dict[str, Any]:
    tree = [
        {"type": "file", "path": "README.md", "size": 10},
        hf_entry("Model-Q4_K_M.gguf", WEIGHTS),
        hf_entry("Model-Q8_0.gguf", b"q8"),
        hf_entry("mmproj-Model-Q8_0.gguf", b"p8"),
        hf_entry("mmproj-Model-F16.gguf", PROJECTOR),
    ]
    return {
        f"https://huggingface.co/api/models/{repo}/revision/main": {"sha": "abc123"},
        f"https://huggingface.co/api/models/{repo}/tree/abc123?recursive=true": tree,
    }


def test_hf_model_files_pick_the_quant_and_a_full_precision_projector() -> None:
    api = hf_api("Org/Model-GGUF")
    found = sources.hf_model_files(api.__getitem__, "Org/Model-GGUF", "Q4_K_M")
    assert found.revision == "abc123"
    assert found.model.name == "Model-Q4_K_M.gguf" and found.model.sha256 == sha(WEIGHTS)
    assert (
        found.model.url == "https://huggingface.co/Org/Model-GGUF/resolve/abc123/Model-Q4_K_M.gguf"
    )
    assert found.mmproj is not None and found.mmproj.name == "mmproj-Model-F16.gguf"
    with pytest.raises(sources.SourceError, match="Q5_K_M"):
        sources.hf_model_files(api.__getitem__, "Org/Model-GGUF", "Q5_K_M")


# --- store ------------------------------------------------------------------


def test_install_runtime_and_model(tmp_path: Path) -> None:
    paths = RelayPaths.at(tmp_path)
    repo = "Org/Model-GGUF"
    api = {
        "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=30": RELEASES,
        **hf_api(repo),
    }
    web = FakeWeb(
        {
            "https://gh/llama-b7000-bin-win-cuda-13.1-x64.zip": LLAMA,
            "https://gh/cudart-llama-bin-win-cuda-13.1-x64.zip": CUDART,
            f"https://huggingface.co/{repo}/resolve/abc123/Model-Q4_K_M.gguf": WEIGHTS,
            f"https://huggingface.co/{repo}/resolve/abc123/mmproj-Model-F16.gguf": PROJECTOR,
        }
    )
    store = Store(paths, api.__getitem__, web)

    runtime = store.install_runtime()
    assert runtime.tag == "b7000" and runtime.cuda == "13.1"
    exe = store.server_exe()
    assert exe is not None and exe.read_bytes() == b"MZ server"
    assert (tmp_path / "runtime" / "b7000" / "cudart64_13.dll").is_file()
    assert not list((tmp_path / "runtime").glob(".*download"))

    model = store.install_model(catalog.custom(repo, "Q4_K_M"))
    assert model.id == "model" and model.revision == "abc123"
    files = store.model_files("model")
    assert files is not None
    assert files[0].read_bytes() == WEIGHTS and files[1] is not None
    assert store.verify() == []

    # Survives a reload from installed.json.
    again = Store(paths).installed()
    assert again.runtime is not None and again.runtime.tag == "b7000"
    assert again.models["model"].size == len(WEIGHTS) + len(PROJECTOR)

    files[0].write_bytes(b"tampered")
    assert store.verify() == ["model/Model-Q4_K_M.gguf"]
    assert store.remove_model("model")
    assert store.model_files("model") is None


def test_install_runtime_refuses_a_zip_that_escapes(tmp_path: Path) -> None:
    evil = zip_bytes({"../evil.exe": b"x", "llama-server.exe": b"MZ"})
    rel = release("b1", {"llama-b1-bin-win-cuda-13.0-x64.zip": evil})
    web = FakeWeb({"https://gh/llama-b1-bin-win-cuda-13.0-x64.zip": evil})
    store = Store(RelayPaths.at(tmp_path / "home"), lambda url: [rel], web)
    with pytest.raises(DownloadError, match="unsafe path"):
        store.install_runtime()
    assert not (tmp_path / "home" / "runtime" / "evil.exe").exists()


def test_catalog_models_without_a_source_are_refused(tmp_path: Path) -> None:
    spec = catalog.find("UI-Venus-1.5-8B")
    assert spec is not None and spec.id == catalog.DEFAULT_LOCATE
    with pytest.raises(DownloadError, match="no download source"):
        Store(RelayPaths.at(tmp_path)).install_model(spec)


def test_models_folder_comes_from_settings(tmp_path: Path) -> None:
    settings = tmp_path / "settings.json"
    assert models_folder_setting(settings) is None
    settings.write_text(json.dumps({"models_folder": r"D:\Models"}))
    assert models_folder_setting(settings) == Path(r"D:\Models")
    settings.write_text(json.dumps({"local_ai": {"models_dir": r"E:\AI", "grounding": None}}))
    assert models_folder_setting(settings) == Path(r"E:\AI")
    settings.write_text(json.dumps({"local_ai": {"models_dir": None}}))
    assert models_folder_setting(settings) is None
    settings.write_text("not json")
    assert models_folder_setting(settings) is None


# --- supervisor -------------------------------------------------------------


def installed_store(tmp_path: Path, model_size: int = 1000) -> Store:
    """A store with a fake runtime and one model `m`, without any downloads."""
    paths = RelayPaths.at(tmp_path)
    exe = tmp_path / "runtime" / "b1" / "llama-server.exe"
    exe.parent.mkdir(parents=True)
    exe.write_bytes(b"MZ")
    (tmp_path / "models" / "m").mkdir(parents=True)
    (tmp_path / "models" / "m" / "m.gguf").write_bytes(b"w")
    (tmp_path / "models" / "m" / "p.gguf").write_bytes(b"p")
    lock = {
        "runtime": {
            "tag": "b1",
            "cuda": "13.1",
            "server": "runtime/b1/llama-server.exe",
            "files": [],
        },
        "models": {
            "m": {
                "id": "m",
                "repo": "Org/M",
                "revision": "abc",
                "quant": "Q4_K_M",
                "model": {"path": "m/m.gguf", "url": "u", "sha256": "s", "size": model_size},
                "mmproj": {"path": "m/p.gguf", "url": "u", "sha256": "s", "size": 0},
            }
        },
    }
    paths.lock.write_text(json.dumps(lock))
    return Store(paths)


class FakeProcess:
    def __init__(self) -> None:
        self.code: int | None = None
        self.terminated = False

    def poll(self) -> int | None:
        return self.code

    def terminate(self) -> None:
        self.terminated = True
        self.code = 0

    def kill(self) -> None:
        self.code = -9

    def wait(self, timeout: float | None = None) -> int:
        return self.code or 0


class Clock:
    def __init__(self) -> None:
        self.now = 100.0

    def __call__(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.now += seconds


class Harness:
    def __init__(
        self, tmp_path: Path, free: int | None = 8 * GIB, ready_after: float = 2.0
    ) -> None:
        self.clock = Clock()
        self.launched: list[list[str]] = []
        self.processes: list[FakeProcess] = []
        self.events: list[dict[str, Any]] = []
        self.ready_at: float | None = None
        self.ready_after = ready_after
        self.supervisor = Supervisor(
            installed_store(tmp_path),
            launch=self.launch,
            health=lambda port: self.ready_at is not None and self.clock.now >= self.ready_at,
            vram=lambda: free,
            port=lambda: 5555,
            clock=self.clock,
            sleep=self.clock.sleep,
            idle=60,
            load_timeout=30,
            record=self.events.append,
        )

    def launch(self, command: list[str], log: Path) -> FakeProcess:
        self.launched.append(command)
        self.ready_at = self.clock.now + self.ready_after
        process = FakeProcess()
        self.processes.append(process)
        return process


def test_ensure_starts_llama_server_on_the_gpu(tmp_path: Path) -> None:
    h = Harness(tmp_path)
    endpoint = h.supervisor.ensure("m")
    assert endpoint.url == "http://127.0.0.1:5555"
    assert endpoint.cold_start == pytest.approx(2.0)
    command = h.launched[0]
    assert command[0].endswith("llama-server.exe")
    assert command[command.index("--n-gpu-layers") + 1] == "999"
    assert command[command.index("--host") + 1] == "127.0.0.1"
    assert "--mmproj" in command
    assert h.events[0]["event"] == "load"
    # Already loaded: no second process.
    assert h.supervisor.ensure("m").cold_start is None
    assert len(h.launched) == 1


@pytest.mark.parametrize(
    ("free", "message"), [(None, "No NVIDIA GPU"), (GIB, "Not enough free VRAM")]
)
def test_ensure_refuses_without_enough_vram(tmp_path: Path, free: int | None, message: str) -> None:
    h = Harness(tmp_path, free=free)
    with pytest.raises(RuntimeUnavailable, match=message):
        h.supervisor.ensure("m")
    assert h.launched == []


def test_ensure_gives_up_on_a_slow_load(tmp_path: Path) -> None:
    h = Harness(tmp_path, ready_after=999)
    with pytest.raises(RuntimeUnavailable, match="did not load"):
        h.supervisor.ensure("m")
    assert h.processes[0].terminated


def test_ensure_reports_a_missing_model(tmp_path: Path) -> None:
    h = Harness(tmp_path)
    with pytest.raises(RuntimeUnavailable, match="not downloaded"):
        h.supervisor.ensure("other")


def test_idle_server_unloads_after_60_seconds(tmp_path: Path) -> None:
    h = Harness(tmp_path)
    h.supervisor.ensure("m")
    h.clock.now += 59
    assert h.supervisor.reap() == []
    h.clock.now += 1
    assert h.supervisor.reap() == ["m"]
    assert h.processes[0].terminated
    assert h.events[-1] == {"event": "unload", "model": "m"}
    # The next call loads it again.
    assert h.supervisor.ensure("m").cold_start is not None


def test_task_keeps_models_loaded_until_60_seconds_after_it_ends(tmp_path: Path) -> None:
    h = Harness(tmp_path)
    with h.supervisor.task("m"):
        assert len(h.launched) == 1
        h.clock.now += 600
        assert h.supervisor.reap() == []
    h.clock.now += 59
    assert h.supervisor.reap() == []
    h.clock.now += 1
    assert h.supervisor.reap() == ["m"]


def test_a_crashed_server_is_dropped_and_restarted(tmp_path: Path) -> None:
    h = Harness(tmp_path)
    h.supervisor.ensure("m")
    h.processes[0].code = 1
    assert h.supervisor.ensure("m").cold_start is not None
    assert len(h.launched) == 2


# --- doctor -----------------------------------------------------------------


GPU = Gpu("NVIDIA GeForce RTX 5080", "581.57", (13, 0), 16 * GIB, 13 * GIB)


def fake_run(outputs: dict[str, str | None]):
    return lambda command: outputs.get(" ".join(command))


def test_doctor_reports_a_ready_pc(tmp_path: Path) -> None:
    store = installed_store(tmp_path)
    run = fake_run(
        {
            "claude --version": "2.1.300 (Claude Code)\n",
            "claude auth status": json.dumps({"loggedIn": True, "authMethod": "oauth_token"}),
        }
    )
    results = {c.name: c for c in doctor.checks(store, GPU, run)}
    assert results["GPU"].detail == "NVIDIA GeForce RTX 5080, driver 581.57"
    assert results["CUDA"].level == "ok"
    assert results["VRAM"].detail == "13.0 GB free of 16.0 GB"
    # The fake runtime is CUDA 13.1, newer than the driver's 13.0.
    assert results["Runtime"].level == "fail"
    assert results["Model m"].level == "ok"
    assert results["Model UI-Venus-1.5-8B"].level == "warn"
    assert results["Claude Code"].detail == "2.1.300 (Claude Code)"
    assert results["Claude sign-in"].detail == "signed in (oauth token)"
    text = doctor.render(list(results.values()))
    assert text.endswith("1 problem(s) to fix.")


def test_doctor_without_gpu_or_claude(tmp_path: Path) -> None:
    results = {c.name: c for c in doctor.checks(Store(RelayPaths.at(tmp_path)), None, fake_run({}))}
    assert results["GPU"].level == "fail"
    assert results["Runtime"].level == "warn"
    assert results["Claude Code"].level == "fail"
    assert "Claude sign-in" not in results


def test_doctor_reads_only_the_signed_in_flag(tmp_path: Path) -> None:
    run = fake_run(
        {"claude --version": "2.1.300", "claude auth status": json.dumps({"loggedIn": False})}
    )
    results = {c.name: c for c in doctor.checks(Store(RelayPaths.at(tmp_path)), GPU, run)}
    assert results["Claude sign-in"].level == "fail"


# --- app interface ----------------------------------------------------------


def test_models_list_json_for_the_app(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    from relay_screen.runtime import cli

    installed_store(tmp_path)
    monkeypatch.setenv("RELAY_HOME", str(tmp_path))
    monkeypatch.delenv("APPDATA", raising=False)
    assert cli.main(["models", "list", "--json"]) == 0
    data = json.loads(capsys.readouterr().out)
    assert data["runtime"]["tag"] == "b1"
    assert [m["id"] for m in data["installed"]] == ["m"]
    assert data["installed"][0]["ready"] is True
    assert {m["id"] for m in data["catalog"]} >= {catalog.DEFAULT_LOCATE, catalog.DEFAULT_CHECK}
    venus = next(m for m in data["catalog"] if m["id"] == catalog.DEFAULT_LOCATE)
    assert venus["repo"] is None


def test_json_progress_counts_files(capsys: pytest.CaptureFixture[str]) -> None:
    from relay_screen.runtime.cli import _JsonProgress

    progress = _JsonProgress()
    progress(0, 10)
    progress(10, 10)
    progress(0, 4)
    lines = [json.loads(line) for line in capsys.readouterr().out.splitlines()]
    assert [line["file"] for line in lines] == [0, 0, 1]
    assert lines[1] == {"done": 10, "total": 10, "file": 0}
