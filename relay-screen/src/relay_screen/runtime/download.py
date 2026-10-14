"""Resumable downloads with SHA-256 verification."""

from __future__ import annotations

import hashlib
import shutil
import urllib.request
from collections.abc import Callable
from pathlib import Path
from typing import BinaryIO, Protocol

CHUNK = 1 << 20
USER_AGENT = "Relay/0.1 (+https://github.com/mohamedmoheyeldin/relay)"

Progress = Callable[[int, int | None], None]
"""Called with (bytes done, total bytes or None) as a download moves."""


class DownloadError(RuntimeError):
    pass


class Response(Protocol):
    status: int

    def read(self, amt: int = ..., /) -> bytes: ...

    def getheader(self, name: str, default: str | None = None) -> str | None: ...

    def close(self) -> None: ...


Opener = Callable[[str, int], Response]
"""Open `url` asking for bytes from `offset` on; returns a 200 or 206 response."""


def http_open(url: str, offset: int) -> Response:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    if offset:
        request.add_header("Range", f"bytes={offset}-")
    return urllib.request.urlopen(request, timeout=60)


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(CHUNK):
            digest.update(chunk)
    return digest.hexdigest()


def download(
    url: str,
    dest: Path,
    sha256: str | None,
    *,
    size: int | None = None,
    progress: Progress | None = None,
    opener: Opener = http_open,
) -> str:
    """Download `url` to `dest`, resuming a `.part` file left by an earlier try.

    Verifies the SHA-256 when one is given (a mismatch deletes the partial file) and
    returns the file's SHA-256. `dest` only appears once the whole file is verified.
    """
    expected = sha256.lower() if sha256 else None
    if dest.is_file() and (expected is None or sha256_of(dest) == expected):
        return expected or sha256_of(dest)
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + ".part")
    digest = hashlib.sha256()
    done = 0
    if part.is_file():
        with part.open("rb") as f:
            while chunk := f.read(CHUNK):
                digest.update(chunk)
                done += len(chunk)
    if size is not None and done > size:
        part.unlink()
        digest, done = hashlib.sha256(), 0

    response = opener(url, done)
    try:
        if done and response.status != 206:
            # The server ignored the range: start over.
            digest, done = hashlib.sha256(), 0
        total = size
        if total is None and (length := response.getheader("Content-Length")):
            total = done + int(length)
        with part.open("ab" if done else "wb") as out:
            _copy(response, out, digest, done, total, progress)
    finally:
        response.close()

    got = digest.hexdigest()
    if expected is not None and got != expected:
        part.unlink(missing_ok=True)
        raise DownloadError(f"{dest.name}: SHA-256 mismatch (got {got}, expected {expected})")
    if size is not None and part.stat().st_size != size:
        raise DownloadError(f"{dest.name}: got {part.stat().st_size} bytes, expected {size}")
    part.replace(dest)
    return got


def _copy(
    response: Response,
    out: BinaryIO,
    digest: hashlib._Hash,
    done: int,
    total: int | None,
    progress: Progress | None,
) -> None:
    if progress:
        progress(done, total)
    while chunk := response.read(CHUNK):
        out.write(chunk)
        digest.update(chunk)
        done += len(chunk)
        if progress:
            progress(done, total)


def free_space(path: Path) -> int:
    """Free bytes on the drive that holds `path` (or its nearest existing parent)."""
    while not path.exists() and path != path.parent:
        path = path.parent
    return shutil.disk_usage(path).free
