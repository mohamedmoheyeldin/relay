"""Windows-only helpers for the harder scenarios (D-096): a window put on another
monitor before a task, and the owner using the PC in the middle of one."""

from __future__ import annotations

import contextlib
import ctypes
import subprocess
import time
from ctypes import wintypes
from pathlib import Path
from typing import Any

from relay_screen import displays

KEYEVENTF_KEYUP = 0x0002
VK_Z = 0x5A


def _user32() -> ctypes.WinDLL:
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    user32.MoveWindow.argtypes = (
        wintypes.HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.BOOL,
    )  # fmt: skip
    user32.SetCursorPos.argtypes = (ctypes.c_int, ctypes.c_int)
    return user32


def _windows() -> dict[int, str]:
    from relay_screen.apps_windows import WinAppBackend

    return {w.handle: w.title for w in WinAppBackend().windows()}


def explorer_on_other_display(folder: Path) -> str | None:
    """Open File Explorer on `folder` (with an `incoming` folder in it) and move it to a
    monitor other than the one in front. Returns why it can't, or None."""
    found = displays.enumerate_displays()
    if len(found) < 2:
        return "only one monitor is connected"
    front = displays.foreground_display(found)
    target = next((d for d in found if front is None or d.handle != front.handle), found[-1])
    (folder / "incoming").mkdir(parents=True, exist_ok=True)
    before = set(_windows())
    subprocess.Popen(["explorer.exe", str(folder)])
    handle = None
    until = time.monotonic() + 10
    while handle is None and time.monotonic() < until:
        time.sleep(0.3)
        handle = next(
            (h for h, t in _windows().items() if h not in before and folder.name in t), None
        )
    if handle is None:
        return f"File Explorer did not open {folder}"
    r = target.rect
    width, height = min(1200, r.width - 160), min(800, r.height - 160)
    _user32().MoveWindow(handle, r.left + 80, r.top + 80, width, height, True)
    return None


def owner_interrupts(downloads: Path) -> str:
    """What an owner does while Relay works: the mouse moves around, another window
    comes in front, and two keys are pressed in it."""
    user32 = _user32()
    found = displays.enumerate_displays()
    main = next((d for d in found if d.primary), found[0]).rect
    cx, cy = main.left + main.width // 2, main.top + main.height // 2
    for dx, dy in ((-300, -150), (250, 120), (-100, 200), (0, 0)):
        user32.SetCursorPos(cx + dx, cy + dy)
        time.sleep(0.15)
    subprocess.Popen(["explorer.exe", str(downloads)])
    time.sleep(1.5)
    for _ in range(2):
        user32.keybd_event(VK_Z, 0, 0, 0)
        user32.keybd_event(VK_Z, 0, KEYEVENTF_KEYUP, 0)
        time.sleep(0.05)
    return "moved the mouse, opened File Explorer in front and pressed Z twice"


# -- memory per process ------------------------------------------------------------

TH32CS_SNAPPROCESS = 0x00000002
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROCESS_VM_READ = 0x0010


class _Entry(ctypes.Structure):
    _fields_ = (
        ("dwSize", wintypes.DWORD),
        ("cntUsage", wintypes.DWORD),
        ("th32ProcessID", wintypes.DWORD),
        ("th32DefaultHeapID", ctypes.c_size_t),
        ("th32ModuleID", wintypes.DWORD),
        ("cntThreads", wintypes.DWORD),
        ("th32ParentProcessID", wintypes.DWORD),
        ("pcPriClassBase", ctypes.c_long),
        ("dwFlags", wintypes.DWORD),
        ("szExeFile", ctypes.c_wchar * 260),
    )


class _Counters(ctypes.Structure):
    _fields_ = (
        ("cb", wintypes.DWORD),
        ("PageFaultCount", wintypes.DWORD),
        ("PeakWorkingSetSize", ctypes.c_size_t),
        ("WorkingSetSize", ctypes.c_size_t),
        ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPagedPoolUsage", ctypes.c_size_t),
        ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
        ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
        ("PagefileUsage", ctypes.c_size_t),
        ("PeakPagefileUsage", ctypes.c_size_t),
        ("PrivateUsage", ctypes.c_size_t),
    )


def processes() -> list[dict[str, int | str]]:
    """Every process: pid, parent, exe name, working set and private bytes."""
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    kernel32.CreateToolhelp32Snapshot.argtypes = (wintypes.DWORD, wintypes.DWORD)
    kernel32.Process32FirstW.argtypes = (wintypes.HANDLE, ctypes.POINTER(_Entry))
    kernel32.Process32NextW.argtypes = (wintypes.HANDLE, ctypes.POINTER(_Entry))
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
    psapi = ctypes.WinDLL("psapi", use_last_error=True)
    psapi.GetProcessMemoryInfo.argtypes = (
        wintypes.HANDLE, ctypes.POINTER(_Counters), wintypes.DWORD,
    )  # fmt: skip
    snap = kernel32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    found: list[dict[str, int | str]] = []
    entry = _Entry()
    entry.dwSize = ctypes.sizeof(_Entry)
    try:
        ok = kernel32.Process32FirstW(snap, ctypes.byref(entry))
        while ok:
            item: dict[str, int | str] = {
                "pid": int(entry.th32ProcessID),
                "parent": int(entry.th32ParentProcessID),
                "name": entry.szExeFile.lower(),
                "ram": 0,
                "private": 0,
            }
            access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ
            handle = kernel32.OpenProcess(access, False, entry.th32ProcessID)
            if handle:
                counters = _Counters()
                counters.cb = ctypes.sizeof(_Counters)
                if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
                    item["ram"] = int(counters.WorkingSetSize)
                    item["private"] = int(counters.PrivateUsage)
                kernel32.CloseHandle(handle)
            found.append(item)
            ok = kernel32.Process32NextW(snap, ctypes.byref(entry))
    finally:
        kernel32.CloseHandle(snap)
    return found


def gpu_processes() -> dict[int, int]:
    """VRAM in bytes per pid, from nvidia-smi (empty when it isn't there)."""
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
            capture_output=True, timeout=10, creationflags=0x08000000, check=False,
        ).stdout.decode("utf-8", "replace")  # fmt: skip
    except (OSError, subprocess.SubprocessError):
        return {}
    found: dict[int, int] = {}
    for line in out.splitlines():
        parts = [p.strip() for p in line.split(",")]
        if len(parts) == 2 and parts[0].isdigit() and parts[1].isdigit():
            found[int(parts[0])] = int(parts[1]) * 1024 * 1024
    return found


# -- Relay's own window, for the app-function tests (D-101) ------------------------


class RelayWindow:
    """diagnostics.app.AppUi over UI Automation: Relay's main window, found by its
    process, read in one cached call per look and pressed through Invoke."""

    def __init__(self) -> None:
        from relay_screen.uia_windows import WinUiaBackend

        self._backend = WinUiaBackend()
        self._pids: set[int] = set()

    def _relay_pids(self) -> set[int]:
        if not self._pids:
            self._pids = {int(p["pid"]) for p in processes() if p["name"] == "relay.exe"}
        return self._pids

    def _tops(self) -> list[Any]:
        auto = self._backend._auto
        pids = self._relay_pids()
        return [w for w in auto.GetRootControl().GetChildren() if w.ProcessId in pids]

    def _main(self) -> Any:
        from relay_screen.uia_windows import WinElement

        def work() -> Any:
            tops = [w for w in self._tops() if (w.Name or "") == "Relay"]
            if not tops:
                self._pids = set()
                tops = [w for w in self._tops() if (w.Name or "") == "Relay"]
            return WinElement(tops[0], self._backend._auto) if tops else None

        return self._backend.run(work)

    def ready(self) -> str | None:
        main = self._main()
        if main is None:
            return "Relay's window is not open"

        def work() -> None:
            control = main._c
            if control.IsMinimize() if hasattr(control, "IsMinimize") else False:
                control.Restore()
            control.SetActive()
            control.SetTopmost(False)

        with contextlib.suppress(Exception):
            self._backend.run(work)
        return None

    def _matches(self, name: str, role: str | None) -> list[Any]:
        main = self._main()
        if main is None:
            return []
        q = name.strip().casefold()

        def work() -> list[Any]:
            (root,) = self._backend.snapshot([main])
            exact: list[Any] = []
            prefix: list[Any] = []
            stack = [root]
            while stack:
                e = stack.pop()
                stack.extend(reversed(e.children()))
                if role and e.role.casefold() != role.casefold():
                    continue
                n = " ".join(e.name.split()).casefold()
                if n == q:
                    exact.append(e)
                elif n.startswith(q):
                    prefix.append(e)
            return exact + prefix

        return list(self._backend.run(work))  # type: ignore[arg-type]

    def find(self, name: str, role: str | None = None) -> list[str]:
        return [e.name for e in self._matches(name, role)]

    def press(self, name: str, role: str | None = None) -> str:
        found = self._matches(name, role)
        if not found:
            raise LookupError(name)
        element = found[0]

        def work() -> None:
            try:
                element.invoke()
            except Exception:
                element._live()._c.Click(simulateMove=False)

        self._backend.run(work)
        return element.name

    def fill(self, name: str, text: str) -> None:
        found = self._matches(name, "Edit")
        if not found:
            raise LookupError(name)
        element = found[0]
        auto = self._backend._auto

        def work() -> None:
            element._live()._c.SetFocus()
            auto.SendKeys("{Ctrl}a{Delete}", waitTime=0.05)
            auto.SendKeys(text.replace("{", "{{}").replace("}", "{}}"), interval=0.005)

        self._backend.run(work)

    def key(self, keys: str) -> None:
        self._backend.run(lambda: self._backend._auto.SendKeys(keys, waitTime=0.1))

    def dialog(self) -> str | None:
        main = self._main()

        def work() -> str | None:
            found = [w for w in self._tops() if w.ClassName == "#32770"]
            if main is not None:
                found += [c for c in main._c.GetChildren() if c.ClassName == "#32770"]
            return (found[0].Name or "file dialog") if found else None

        return self._backend.run(work)  # type: ignore[return-value]
