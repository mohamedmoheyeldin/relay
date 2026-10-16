"""Windows-only helpers for the harder scenarios (D-096): a window put on another
monitor before a task, and the owner using the PC in the middle of one."""

from __future__ import annotations

import ctypes
import subprocess
import time
from ctypes import wintypes
from pathlib import Path

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
