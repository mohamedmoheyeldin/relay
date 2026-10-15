"""Build Relay-Setup.exe, the one installer (D-044). No GitHub Actions (D-050).

    uv run python scripts/build_installer.py               # bundle relay-screen, build it all
    uv run python scripts/build_installer.py --skip-bundle # reuse the bundled Python
    uv run python scripts/build_installer.py --bundle      # only refresh relay-ui/resources/python

Steps:
1. Bundle relay-screen: the official CPython 3.12 for Windows (python.org's NuGet
   package, checked by SHA-256) goes into relay-ui/resources/python, and relay-screen
   with its Windows wheels is installed into its site-packages with uv. This replaces
   the PyInstaller freeze of D-044 (see the PR notes): it can be built from any OS.
2. Build the Tauri app and its NSIS installer with pnpm. On Windows this uses the
   default MSVC target; elsewhere it cross-compiles with the MinGW target
   (x86_64-pc-windows-gnu), which needs mingw-w64 and nsis installed.
3. Build Relay Setup (relay-setup/), the small app with Relay's look that the owner
   runs. It embeds the NSIS installer from step 2 (build.rs reads RELAY_SETUP_PAYLOAD)
   and runs it silently, elevated, to install, update or repair; it also uninstalls.
4. Write dist/Relay-Setup.exe and dist/Relay-Setup-<ver>.exe (Relay Setup), and
   dist/Relay-Setup-nsis-<ver>.exe (the raw NSIS installer, for silent installs:
   `Relay-Setup-nsis-<ver>.exe /S`).

The installer is unsigned for now, so Windows SmartScreen warns on first run.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tomllib
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


def workspace_version() -> str:
    """The version relay-setup embeds as the payload's (Cargo workspace version)."""
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return cargo["workspace"]["package"]["version"]


def target_args() -> tuple[list[str], Path]:
    """The cargo/tauri --target flags and the matching target directory."""
    target_dir = ROOT / "target"
    if sys.platform == "win32":
        return [], target_dir
    triple = "x86_64-pc-windows-gnu"
    return ["--target", triple], target_dir / triple


def build_nsis() -> Path:
    pnpm = shutil.which("pnpm") or "pnpm"
    run([pnpm, "install", "--frozen-lockfile"], cwd=UI)
    flags, target_dir = target_args()
    run([pnpm, "tauri", "build", "--bundles", "nsis", *flags], cwd=UI)
    nsis = target_dir / "release" / "bundle" / "nsis"
    built = sorted(nsis.glob("*-setup.exe"), key=lambda p: p.stat().st_mtime)
    if not built:
        sys.exit(f"No installer found in {nsis}")
    return built[-1]


def build_setup(payload: Path) -> Path:
    """Build relay-setup with the NSIS installer embedded. On MinGW it also embeds
    WebView2Loader.dll (tauri-build copies it into the target dir), so the exe is a
    single file; MSVC builds link the loader statically."""
    flags, target_dir = target_args()
    env = {**os.environ, "RELAY_SETUP_PAYLOAD": str(payload.resolve())}
    if flags:
        loader = target_dir / "release" / "WebView2Loader.dll"
        if not loader.exists():
            sys.exit(f"No WebView2Loader.dll in {loader.parent}")
        env["RELAY_SETUP_WEBVIEW2_LOADER"] = str(loader)
    cmd = ["cargo", "build", "--release", "-p", "relay-setup", *flags]
    print("+", " ".join(cmd), f"(RELAY_SETUP_PAYLOAD={payload})", flush=True)
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)
    exe = target_dir / "release" / "relay-setup.exe"
    if not exe.exists():
        sys.exit(f"relay-setup was not built: {exe}")
    imports = pe_imports(exe)
    if "webview2loader.dll" in imports:
        sys.exit(f"{exe} imports WebView2Loader.dll, so it would not start on its own.")
    return exe


def pe_imports(path: Path) -> set[str]:
    """The DLL names in a PE file's import table, lowercased."""
    data = path.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    import_rva = struct.unpack_from("<I", data, optional + 120)[0]  # PE32+ data dir 1

    def offset(rva: int) -> int:
        table = optional + optional_size
        for i in range(sections):
            entry = table + 40 * i
            size, address, _, raw = struct.unpack_from("<IIII", data, entry + 8)
            if address <= rva < address + size:
                return rva - address + raw
        raise ValueError(f"RVA {rva:#x} is in no section")

    names: set[str] = set()
    entry = offset(import_rva)
    while True:
        name_rva = struct.unpack_from("<I", data, entry + 12)[0]
        if name_rva == 0:
            return names
        start = offset(name_rva)
        names.add(data[start : data.index(b"\0", start)].decode("ascii").lower())
        entry += 20


def describe(path: Path) -> str:
    return f"{path} ({path.stat().st_size / 1e6:.1f} MB, sha256 {sha256(path)})"


def build_installer() -> Path:
    version = app_version()
    if workspace_version() != version:
        sys.exit(
            f"Version mismatch: Cargo.toml says {workspace_version()}, "
            f"tauri.conf.json says {version}. Relay Setup must match the installer inside it."
        )
    nsis = build_nsis()
    DIST.mkdir(exist_ok=True)
    raw = DIST / f"Relay-Setup-nsis-{version}.exe"
    shutil.copy2(nsis, raw)
    setup = build_setup(raw)
    out = DIST / "Relay-Setup.exe"
    shutil.copy2(setup, out)
    shutil.copy2(setup, DIST / f"Relay-Setup-{version}.exe")
    print(f"NSIS installer: {describe(raw)}", flush=True)
    print(f"Relay Setup:    {describe(out)}", flush=True)
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
