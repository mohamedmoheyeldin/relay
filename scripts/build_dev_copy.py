"""Package a Relay build for the dev loop (D-096): a zip that scripts/dev_loop.ps1
unpacks into Downloads\\Relay-dev\\current and runs from there, with no installer and
no admin prompt.

    uv run python scripts/build_dev_copy.py            # the whole app, from the newest
                                                       # dist/Relay-Setup-nsis-*.exe
    uv run python scripts/build_dev_copy.py --code     # only relay-screen's code, from
                                                       # this checkout (small, for Python fixes)

Writes dist/Relay-dev-<ver>.zip (or dist/Relay-dev-code.zip) and prints its path and
SHA-256. The full copy is the installer's own files, unpacked with 7-Zip.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
SOURCE = ROOT / "relay-screen" / "src" / "relay_screen"
SITE = "python/Lib/site-packages/relay_screen"
SKIP = {"$PLUGINSDIR", "uninstall.exe"}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def add_tree(archive: zipfile.ZipFile, folder: Path, prefix: str) -> None:
    for path in sorted(folder.rglob("*")):
        if path.is_file() and "__pycache__" not in path.parts:
            archive.write(path, f"{prefix}/{path.relative_to(folder).as_posix()}")


def full_copy() -> Path:
    installers = sorted(DIST.glob("Relay-Setup-nsis-*.exe"), key=lambda p: p.stat().st_mtime)
    if not installers:
        sys.exit("No dist/Relay-Setup-nsis-*.exe: run scripts/build_installer.py first.")
    nsis = installers[-1]
    seven = shutil.which("7z")
    if seven is None:
        sys.exit("7-Zip (7z) is needed to unpack the installer.")
    version = nsis.stem.removeprefix("Relay-Setup-nsis-")
    out = DIST / f"Relay-dev-{version}.zip"
    with tempfile.TemporaryDirectory() as tmp:
        subprocess.run([seven, "x", "-y", f"-o{tmp}", str(nsis)], check=True, capture_output=True)
        with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
            for path in sorted(Path(tmp).iterdir()):
                if path.name in SKIP:
                    continue
                if path.is_dir():
                    add_tree(archive, path, path.name)
                else:
                    archive.write(path, path.name)
            archive.write(ROOT / "scripts" / "dev_loop.ps1", "dev_loop.ps1")
    return out


def code_copy() -> Path:
    out = DIST / "Relay-dev-code.zip"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
        add_tree(archive, SOURCE, SITE)
        archive.write(ROOT / "scripts" / "dev_loop.ps1", "dev_loop.ps1")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--code", action="store_true", help="only relay-screen's code")
    args = parser.parse_args()
    DIST.mkdir(exist_ok=True)
    out = code_copy() if args.code else full_copy()
    size = out.stat().st_size / 1e6
    print(f"{out} ({size:.1f} MB, sha256 {sha256(out)})")


if __name__ == "__main__":
    main()
