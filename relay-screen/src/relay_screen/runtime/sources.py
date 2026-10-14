"""Find the files to download, with the SHA-256 their publisher declares (D-052).

llama.cpp comes from its GitHub releases, whose assets carry a `sha256:` digest.
Models come from Hugging Face, whose API gives each LFS file's SHA-256 and size.
"""

from __future__ import annotations

import json
import re
import urllib.parse
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from relay_screen.runtime.download import USER_AGENT

LLAMA_REPO = "ggml-org/llama.cpp"
HF = "https://huggingface.co"
# Blackwell (RTX 50) needs CUDA 12.8 or newer (D-004).
MIN_CUDA = (12, 8)

GetJson = Callable[[str], Any]


def http_json(url: str) -> Any:
    request = urllib.request.Request(
        url, headers={"User-Agent": USER_AGENT, "Accept": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


class SourceError(RuntimeError):
    pass


@dataclass(frozen=True)
class RemoteFile:
    name: str
    url: str
    sha256: str
    size: int


@dataclass(frozen=True)
class RuntimeRelease:
    tag: str
    cuda: tuple[int, int]
    files: tuple[RemoteFile, ...]
    """The llama.cpp zip, then the matching CUDA runtime DLL zip."""


_LLAMA_ZIP = re.compile(
    r"^llama-(?P<tag>b\d+)-bin-win-cuda-(?P<major>\d+)\.(?P<minor>\d+)-x64\.zip$"
)
_CUDART_ZIP = "cudart-llama-bin-win-cuda-{major}.{minor}-x64.zip"


def llama_release(
    get_json: GetJson, tag: str | None = None, driver_cuda: tuple[int, int] | None = None
) -> RuntimeRelease:
    """The Windows CUDA build of llama.cpp `tag` (default: the newest release that has one).

    Picks the newest CUDA build the driver supports (`driver_cuda`, from NVML), never
    older than CUDA 12.8. Without a tag it walks the recent releases newest first:
    llama.cpp marks its `bNNNN` builds as pre-releases, and GitHub's "latest" release
    can be a tag with no Windows binaries at all.
    """
    base = f"https://api.github.com/repos/{LLAMA_REPO}/releases"
    if tag:
        releases = [get_json(f"{base}/tags/{tag}")]
    else:
        releases = [r for r in get_json(f"{base}?per_page=30") if not r.get("draft")]
    for release in releases:
        if picked := _windows_cuda(release, driver_cuda):
            return picked
    have = f" (driver supports CUDA {driver_cuda[0]}.{driver_cuda[1]})" if driver_cuda else ""
    which = f"llama.cpp {tag}" if tag else "No recent llama.cpp release"
    raise SourceError(f"{which} has no Windows CUDA 12.8+ build{have}")


def _windows_cuda(
    release: dict[str, Any], driver_cuda: tuple[int, int] | None
) -> RuntimeRelease | None:
    assets = {a["name"]: a for a in release.get("assets", [])}
    builds: list[tuple[tuple[int, int], dict[str, Any]]] = []
    for name, asset in assets.items():
        if m := _LLAMA_ZIP.match(name):
            cuda = (int(m["major"]), int(m["minor"]))
            if cuda >= MIN_CUDA and (driver_cuda is None or cuda <= driver_cuda):
                builds.append((cuda, asset))
    if not builds:
        return None
    cuda, asset = max(builds, key=lambda b: b[0])
    files = [_github_file(asset)]
    if cudart := assets.get(_CUDART_ZIP.format(major=cuda[0], minor=cuda[1])):
        files.append(_github_file(cudart))
    return RuntimeRelease(release["tag_name"], cuda, tuple(files))


def _github_file(asset: dict[str, Any]) -> RemoteFile:
    digest = asset.get("digest") or ""
    if not digest.startswith("sha256:"):
        raise SourceError(f"{asset['name']} has no published SHA-256")
    return RemoteFile(
        asset["name"], asset["browser_download_url"], digest.removeprefix("sha256:"), asset["size"]
    )


@dataclass(frozen=True)
class ModelFiles:
    repo: str
    revision: str
    model: RemoteFile
    mmproj: RemoteFile | None


def hf_model_files(get_json: GetJson, repo: str, quant: str, revision: str = "main") -> ModelFiles:
    """The GGUF for `quant` in a Hugging Face repo and its vision projector (mmproj).

    `revision` is resolved to a commit so the lock file records exactly what was used.
    """
    quoted = urllib.parse.quote(repo, safe="/")
    info = get_json(f"{HF}/api/models/{quoted}/revision/{revision}")
    commit = info.get("sha") or revision
    tree = get_json(f"{HF}/api/models/{quoted}/tree/{commit}?recursive=true")
    ggufs = [f for f in tree if f.get("type") == "file" and f["path"].lower().endswith(".gguf")]
    projectors = [f for f in ggufs if "mmproj" in f["path"].lower()]
    weights = [f for f in ggufs if f not in projectors and _has_quant(f["path"], quant)]
    if not weights:
        raise SourceError(f"{repo} has no {quant} GGUF")
    if len(weights) > 1:
        # Split GGUFs (-00001-of-00002) are not supported yet; take the plainest name.
        weights.sort(key=lambda f: len(f["path"]))
    projector = min(projectors, key=_projector_rank) if projectors else None

    def remote(entry: dict[str, Any]) -> RemoteFile:
        lfs = entry.get("lfs") or {}
        sha = lfs.get("oid") or lfs.get("sha256")
        if not sha:
            raise SourceError(f"{repo}/{entry['path']} has no published SHA-256")
        url = f"{HF}/{quoted}/resolve/{commit}/{urllib.parse.quote(entry['path'])}"
        name = entry["path"].rsplit("/", 1)[-1]
        return RemoteFile(name, url, sha, lfs.get("size", entry["size"]))

    return ModelFiles(repo, commit, remote(weights[0]), remote(projector) if projector else None)


def _has_quant(path: str, quant: str) -> bool:
    return re.search(rf"(?<![A-Za-z0-9]){re.escape(quant)}(?![A-Za-z0-9])", path, re.I) is not None


def _projector_rank(entry: dict[str, Any]) -> int:
    """Prefer a full-precision projector: grounding loses accuracy with a quantized one."""
    name = entry["path"].lower()
    for rank, kind in enumerate(("f16", "bf16", "f32", "q8_0")):
        if kind in name:
            return rank
    return 9
