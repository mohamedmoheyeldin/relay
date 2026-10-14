"""Install llama.cpp and models, and remember what is installed (`installed.json`)."""

from __future__ import annotations

import json
import shutil
import zipfile
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

from relay_screen.runtime import sources
from relay_screen.runtime.catalog import ModelSpec
from relay_screen.runtime.download import (
    DownloadError,
    Opener,
    Progress,
    download,
    free_space,
    http_open,
    sha256_of,
)
from relay_screen.runtime.paths import RelayPaths
from relay_screen.runtime.sources import GetJson, RemoteFile, http_json

SERVER_EXE = "llama-server.exe"


@dataclass
class InstalledFile:
    path: str
    """Relative to the Relay home (runtime) or the models folder (models)."""
    url: str
    sha256: str
    size: int


@dataclass
class InstalledRuntime:
    tag: str
    cuda: str
    server: str
    """llama-server, relative to the Relay home."""
    files: list[InstalledFile] = field(default_factory=list)


@dataclass
class InstalledModel:
    id: str
    repo: str
    revision: str
    quant: str
    model: InstalledFile
    mmproj: InstalledFile | None
    context: int = 8192

    @property
    def size(self) -> int:
        return self.model.size + (self.mmproj.size if self.mmproj else 0)


@dataclass
class Installed:
    runtime: InstalledRuntime | None = None
    models: dict[str, InstalledModel] = field(default_factory=dict)

    @classmethod
    def load(cls, paths: RelayPaths) -> Installed:
        try:
            data = json.loads(paths.lock.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return cls()
        runtime = data.get("runtime")
        return cls(
            _runtime(runtime) if runtime else None,
            {k: _model(v) for k, v in data.get("models", {}).items()},
        )

    def save(self, paths: RelayPaths) -> None:
        paths.lock.parent.mkdir(parents=True, exist_ok=True)
        tmp = paths.lock.with_suffix(".tmp")
        tmp.write_text(json.dumps(asdict(self), indent=2), encoding="utf-8")
        tmp.replace(paths.lock)


def _file(d: dict[str, Any]) -> InstalledFile:
    return InstalledFile(d["path"], d["url"], d["sha256"], d["size"])


def _runtime(d: dict[str, Any]) -> InstalledRuntime:
    return InstalledRuntime(d["tag"], d["cuda"], d["server"], [_file(f) for f in d["files"]])


def _model(d: dict[str, Any]) -> InstalledModel:
    mmproj = d.get("mmproj")
    return InstalledModel(
        d["id"],
        d["repo"],
        d["revision"],
        d["quant"],
        _file(d["model"]),
        _file(mmproj) if mmproj else None,
        d.get("context", 8192),
    )


class Store:
    def __init__(
        self,
        paths: RelayPaths,
        get_json: GetJson = http_json,
        opener: Opener = http_open,
        progress: Progress | None = None,
    ) -> None:
        self.paths = paths
        self.get_json = get_json
        self.opener = opener
        self.progress = progress

    def installed(self) -> Installed:
        return Installed.load(self.paths)

    def server_exe(self) -> Path | None:
        runtime = self.installed().runtime
        if runtime is None:
            return None
        exe = self.paths.home / runtime.server
        return exe if exe.is_file() else None

    def model_files(self, model_id: str) -> tuple[Path, Path | None] | None:
        model = self.installed().models.get(model_id)
        if model is None:
            return None
        weights = self.paths.models / model.model.path
        mmproj = self.paths.models / model.mmproj.path if model.mmproj else None
        if not weights.is_file() or (mmproj is not None and not mmproj.is_file()):
            return None
        return weights, mmproj

    def install_runtime(
        self, tag: str | None = None, driver_cuda: tuple[int, int] | None = None
    ) -> InstalledRuntime:
        """Download, verify and unpack a llama.cpp CUDA build (D-004)."""
        release = sources.llama_release(self.get_json, tag, driver_cuda)
        target = self.paths.runtime / release.tag
        self._need_space(target, sum(f.size for f in release.files) * 3)
        zips = target.parent / f".{release.tag}-download"
        files: list[InstalledFile] = []
        for remote in release.files:
            archive = zips / remote.name
            self._fetch(remote, archive)
            _unzip(archive, target)
            files.append(_installed(remote, Path(remote.name)))
        server = next(target.rglob(SERVER_EXE), None)
        if server is None:
            raise DownloadError(f"llama.cpp {release.tag} has no {SERVER_EXE}")
        shutil.rmtree(zips, ignore_errors=True)
        runtime = InstalledRuntime(
            release.tag,
            f"{release.cuda[0]}.{release.cuda[1]}",
            server.relative_to(self.paths.home).as_posix(),
            files,
        )
        installed = self.installed()
        old = installed.runtime
        installed.runtime = runtime
        installed.save(self.paths)
        if old is not None and old.tag != release.tag:
            shutil.rmtree(self.paths.runtime / old.tag, ignore_errors=True)
        return runtime

    def install_model(self, spec: ModelSpec, quant: str | None = None) -> InstalledModel:
        """Download a model's GGUF and mmproj into the models folder (D-046)."""
        if spec.repo is None:
            raise DownloadError(f"{spec.name} has no download source yet (picked in M1-11)")
        quant = quant or spec.quant
        found = sources.hf_model_files(self.get_json, spec.repo, quant)
        folder = Path(spec.id)
        need = found.model.size + (found.mmproj.size if found.mmproj else 0)
        self._need_space(self.paths.models / folder, need)
        self._fetch(found.model, self.paths.models / folder / found.model.name)
        if found.mmproj:
            self._fetch(found.mmproj, self.paths.models / folder / found.mmproj.name)
        model = InstalledModel(
            spec.id,
            spec.repo,
            found.revision,
            quant,
            _installed(found.model, folder / found.model.name),
            _installed(found.mmproj, folder / found.mmproj.name) if found.mmproj else None,
            spec.context,
        )
        installed = self.installed()
        installed.models[spec.id] = model
        installed.save(self.paths)
        return model

    def remove_model(self, model_id: str) -> bool:
        installed = self.installed()
        if installed.models.pop(model_id, None) is None:
            return False
        shutil.rmtree(self.paths.models / model_id, ignore_errors=True)
        installed.save(self.paths)
        return True

    def verify(self) -> list[str]:
        """Re-hash every installed model file; returns the ones that no longer match."""
        bad: list[str] = []
        for model in self.installed().models.values():
            for f in filter(None, (model.model, model.mmproj)):
                path = self.paths.models / f.path
                if not path.is_file() or sha256_of(path) != f.sha256:
                    bad.append(f.path)
        return bad

    def _fetch(self, remote: RemoteFile, dest: Path) -> None:
        download(
            remote.url,
            dest,
            remote.sha256,
            size=remote.size,
            progress=self.progress,
            opener=self.opener,
        )

    @staticmethod
    def _need_space(path: Path, size: int) -> None:
        free = free_space(path)
        if free < size:
            raise DownloadError(
                f"Not enough disk space in {path}: need {size / 1e9:.1f} GB, "
                f"{free / 1e9:.1f} GB free"
            )


def _installed(remote: RemoteFile, path: Path) -> InstalledFile:
    return InstalledFile(path.as_posix(), remote.url, remote.sha256, remote.size)


def _unzip(archive: Path, target: Path) -> None:
    """Extract `archive` into `target`, refusing entries that would land outside it."""
    target.mkdir(parents=True, exist_ok=True)
    root = target.resolve()
    with zipfile.ZipFile(archive) as z:
        for member in z.infolist():
            dest = (target / member.filename).resolve()
            if not dest.is_relative_to(root):
                raise DownloadError(f"{archive.name}: unsafe path {member.filename!r}")
        z.extractall(target)
