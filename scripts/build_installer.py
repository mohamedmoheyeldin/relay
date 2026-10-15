"""Build Relay-Setup.exe, the one installer (D-044). No GitHub Actions (D-050).

    uv run python scripts/build_installer.py           # bundle relay-screen, build the installer
    uv run python scripts/build_installer.py --bundle  # only refresh relay-ui/resources/python

Steps:
1. Bundle relay-screen: the official CPython 3.12 for Windows (python.org's NuGet
   package, checked by SHA-256) goes into relay-ui/resources/python, and relay-screen
   with its Windows wheels is installed into its site-packages with uv. This replaces
   the PyInstaller freeze of D-044 (see the PR notes): it can be built from any OS.
2. Build the Tauri app and its NSIS installer with pnpm. On Windows this uses the
   default MSVC target; elsewhere it cross-compiles with the MinGW target
   (x86_64-pc-windows-gnu), which needs mingw-w64 and nsis installed.
3. Copy the installer to dist/Relay-Setup.exe (and a versioned copy).

The installer is unsigned for now, so Windows SmartScreen warns on first run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "relay-ui"
PYTHON_DIR = UI / "resources" / "python"
CACHE = ROOT / ".cache" / "installer"
DIST = ROOT / "dist"

PYTHON_VERSION = "3.12.10"
PYTHON_URL = (
    f"https://api.nuget.org/v3-flatcontainer/python/{PYTHON_VERSION}/python.{PYTHON_VERSION}.nupkg"
)
PYTHON_SHA256 = "0eb85c2dfccccf1b17352de4c397f69194035b7d37149eacc16f1147d93de3b8"

# Parts of the standard library relay-screen never uses.
PRUNE = [
    "Lib/site-packages/pip",
    "Lib/site-packages/pip-*.dist-info",
    "Lib/lib2to3",
    "Lib/idlelib",
    "Lib/turtledemo",
    "Lib/ensurepip",
    "include",
    "libs",
]


def run(cmd: list[str], cwd: Path = ROOT) -> None:
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=cwd, check=True)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch_python() -> Path:
    CACHE.mkdir(parents=True, exist_ok=True)
    pkg = CACHE / f"python.{PYTHON_VERSION}.nupkg"
    if not pkg.exists() or sha256(pkg) != PYTHON_SHA256:
        print(f"Downloading CPython {PYTHON_VERSION} for Windows", flush=True)
        urllib.request.urlretrieve(PYTHON_URL, pkg)
    if sha256(pkg) != PYTHON_SHA256:
        pkg.unlink()
        sys.exit("The Python package failed its SHA-256 check.")
    return pkg


def pip_target(site: Path) -> list[str]:
    return [
        "--target",
        str(site),
        "--python-platform",
        "x86_64-pc-windows-msvc",
        "--python-version",
        "3.12",
        "--no-compile",
    ]


def install_relay_screen() -> None:
    """(Re)install relay-screen itself into the bundle. Its version rarely changes
    between builds, so --reinstall makes sure the current source is what ships."""
    site = PYTHON_DIR / "Lib" / "site-packages"
    run(
        [
            "uv",
            "pip",
            "install",
            *pip_target(site),
            "--no-deps",
            "--reinstall",
            str(ROOT / "relay-screen"),
        ]
    )


def bundle_python() -> None:
    pkg = fetch_python()
    if PYTHON_DIR.exists():
        shutil.rmtree(PYTHON_DIR)
    PYTHON_DIR.mkdir(parents=True)
    with zipfile.ZipFile(pkg) as z:
        for info in z.infolist():
            if not info.filename.startswith("tools/") or info.is_dir():
                continue
            target = PYTHON_DIR / info.filename.removeprefix("tools/")
            target.parent.mkdir(parents=True, exist_ok=True)
            with z.open(info) as src, target.open("wb") as dst:
                shutil.copyfileobj(src, dst)
    for pattern in PRUNE:
        for p in PYTHON_DIR.glob(pattern):
            shutil.rmtree(p) if p.is_dir() else p.unlink()
    site = PYTHON_DIR / "Lib" / "site-packages"
    site.mkdir(parents=True, exist_ok=True)
    # relay-screen's dependencies exactly as uv.lock pins them, then relay-screen itself,
    # installed for Windows x64 / CPython 3.12.
    requirements = CACHE / "relay-screen-requirements.txt"
    run(
        [
            "uv",
            "export",
            "--frozen",
            "--no-dev",
            "--no-emit-project",
            "--no-emit-workspace",
            "--package",
            "relay-screen",
            "--output-file",
            str(requirements),
        ]
    )
    run(["uv", "pip", "install", *pip_target(site), "--require-hashes", "-r", str(requirements)])
    install_relay_screen()
    for cache in PYTHON_DIR.rglob("__pycache__"):
        shutil.rmtree(cache, ignore_errors=True)
    (PYTHON_DIR / ".gitkeep").touch()
    size = sum(f.stat().st_size for f in PYTHON_DIR.rglob("*") if f.is_file())
    print(f"Bundled Python + relay-screen: {size / 1e6:.0f} MB in {PYTHON_DIR}", flush=True)


def app_version() -> str:
    conf = json.loads((UI / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    return conf["version"]


def build_installer() -> Path:
    pnpm = shutil.which("pnpm") or "pnpm"
    run([pnpm, "install", "--frozen-lockfile"], cwd=UI)
    cmd = [pnpm, "tauri", "build", "--bundles", "nsis"]
    target_dir = ROOT / "target"
    if sys.platform != "win32":
        triple = "x86_64-pc-windows-gnu"
        cmd += ["--target", triple]
        target_dir = target_dir / triple
    run(cmd, cwd=UI)
    nsis = target_dir / "release" / "bundle" / "nsis"
    built = sorted(nsis.glob("*-setup.exe"), key=lambda p: p.stat().st_mtime)
    if not built:
        sys.exit(f"No installer found in {nsis}")
    DIST.mkdir(exist_ok=True)
    version = app_version()
    out = DIST / "Relay-Setup.exe"
    shutil.copy2(built[-1], out)
    shutil.copy2(built[-1], DIST / f"Relay-Setup-{version}.exe")
    print(f"Installer: {out} ({out.stat().st_size / 1e6:.0f} MB, sha256 {sha256(out)})", flush=True)
    return out


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--bundle", action="store_true", help="only bundle Python and relay-screen")
    parser.add_argument(
        "--skip-bundle",
        action="store_true",
        help="reuse relay-ui/resources/python, refreshing only relay-screen itself",
    )
    args = parser.parse_args()
    if args.skip_bundle:
        install_relay_screen()
    else:
        bundle_python()
    if not args.bundle:
        build_installer()


if __name__ == "__main__":
    main()
