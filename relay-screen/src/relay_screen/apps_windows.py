"""Windows backend for apps.py: Start menu, App Paths, registered browsers, windows."""

from __future__ import annotations

import contextlib
import ctypes
import json
import os
import re
import subprocess
from collections.abc import Sequence
from ctypes import wintypes
from pathlib import Path
from typing import Any

from relay_screen import win32
from relay_screen.apps import App, Window
from relay_screen.scaling import Rect

START_APPS_TIMEOUT = 30  # s; Get-StartApps takes about a second
_NO_WINDOW = 0x08000000  # CREATE_NO_WINDOW
_DETACHED = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
_BREAKAWAY = 0x01000000  # CREATE_BREAKAWAY_FROM_JOB: outlive Claude Code's job object

_KNOWN_FOLDER = re.compile(r"^(\{[0-9A-Fa-f-]{36}\})\\(.*)$")
_START_APPS_SCRIPT = (
    "[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); "
    "@(Get-StartApps | Select-Object Name, AppID) | ConvertTo-Json -Compress"
)
_APP_PATHS = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"
_CLIENTS = r"SOFTWARE\Clients\StartMenuInternet"
_USER_CHOICE = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\{}\UserChoice"

GW_OWNER = 4
GWL_EXSTYLE = -20
WS_EX_TOOLWINDOW = 0x80
DWMWA_CLOAKED = 14
SW_RESTORE = 9
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000


class GUID(ctypes.Structure):
    _fields_ = (
        ("Data1", wintypes.DWORD),
        ("Data2", wintypes.WORD),
        ("Data3", wintypes.WORD),
        ("Data4", ctypes.c_ubyte * 8),
    )


def known_folder(guid: str) -> str | None:
    """SHGetKnownFolderPath for a "{GUID}" string, as Get-StartApps writes them."""
    ole32 = ctypes.windll.ole32  # pyright: ignore[reportAttributeAccessIssue]
    shell32 = ctypes.windll.shell32  # pyright: ignore[reportAttributeAccessIssue]
    folder = GUID()
    if ole32.CLSIDFromString(ctypes.c_wchar_p(guid), ctypes.byref(folder)) != 0:
        return None
    path = ctypes.c_void_p()
    try:
        if shell32.SHGetKnownFolderPath(ctypes.byref(folder), 0, None, ctypes.byref(path)) != 0:
            return None
        return ctypes.wstring_at(path.value) if path.value else None
    finally:
        ole32.CoTaskMemFree(path)


def exe_from_app_id(app_id: str) -> str | None:
    """The program path behind a desktop app's AppID; None for Store apps and links."""
    match = _KNOWN_FOLDER.match(app_id)
    if match:
        base = known_folder(match.group(1))
        if base is None:
            return None
        path = os.path.join(base, match.group(2))
    elif re.match(r"^[A-Za-z]:\\", app_id):
        path = app_id
    else:
        return None
    return path if path.lower().endswith(".exe") and os.path.isfile(path) else None


def exe_from_command(command: str) -> str | None:
    """The program in a shell\\open\\command line such as '"C:\\x\\b.exe" -- "%1"'."""
    command = os.path.expandvars(command.strip())
    if command.startswith('"'):
        path = command[1:].split('"', 1)[0]
    else:
        lower = command.lower()
        end = lower.find(".exe")
        path = command[: end + 4] if end >= 0 else command.split(" ", 1)[0]
    return path if os.path.isfile(path) else None


def indirect_string(text: str) -> str | None:
    """Resolve '@{Package?ms-resource://...}' or '@dll,-id' names (SHLoadIndirectString)."""
    if not text.startswith("@"):
        return text
    buffer = ctypes.create_unicode_buffer(512)
    shlwapi = ctypes.windll.shlwapi  # pyright: ignore[reportAttributeAccessIssue]
    if shlwapi.SHLoadIndirectString(ctypes.c_wchar_p(text), buffer, len(buffer), None) != 0:
        return None
    return buffer.value or None


def _reg_value(root: Any, path: str, name: str = "") -> str | None:
    import winreg

    try:
        with winreg.OpenKey(root, path) as key:
            value, _ = winreg.QueryValueEx(key, name)
    except OSError:
        return None
    return value if isinstance(value, str) and value else None


def _reg_subkeys(root: Any, path: str) -> list[str]:
    import winreg

    try:
        with winreg.OpenKey(root, path) as key:
            count = winreg.QueryInfoKey(key)[0]
            return [winreg.EnumKey(key, i) for i in range(count)]
    except OSError:
        return []


class WinAppBackend:
    def __init__(self) -> None:
        win32.require_windows()
        user32 = ctypes.WinDLL("user32", use_last_error=True)
        user32.GetForegroundWindow.restype = wintypes.HWND
        user32.GetWindow.argtypes = (wintypes.HWND, wintypes.UINT)
        user32.GetWindow.restype = wintypes.HWND
        user32.GetWindowLongW.argtypes = (wintypes.HWND, ctypes.c_int)
        user32.GetWindowLongW.restype = ctypes.c_long
        user32.GetWindowTextLengthW.argtypes = (wintypes.HWND,)
        user32.GetWindowTextW.argtypes = (wintypes.HWND, wintypes.LPWSTR, ctypes.c_int)
        user32.GetWindowRect.argtypes = (wintypes.HWND, ctypes.POINTER(wintypes.RECT))
        user32.GetWindowThreadProcessId.argtypes = (wintypes.HWND, ctypes.POINTER(wintypes.DWORD))
        user32.GetWindowThreadProcessId.restype = wintypes.DWORD
        user32.IsWindowVisible.argtypes = (wintypes.HWND,)
        user32.IsIconic.argtypes = (wintypes.HWND,)
        user32.ShowWindow.argtypes = (wintypes.HWND, ctypes.c_int)
        user32.SetForegroundWindow.argtypes = (wintypes.HWND,)
        user32.BringWindowToTop.argtypes = (wintypes.HWND,)
        user32.AttachThreadInput.argtypes = (wintypes.DWORD, wintypes.DWORD, wintypes.BOOL)
        self._user32 = user32
        self._kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        self._kernel32.OpenProcess.restype = wintypes.HANDLE
        self._kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
        self._kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
        self._kernel32.QueryFullProcessImageNameW.argtypes = (
            wintypes.HANDLE,
            wintypes.DWORD,
            wintypes.LPWSTR,
            ctypes.POINTER(wintypes.DWORD),
        )
        self._dwm = ctypes.WinDLL("dwmapi")
        self._exe_cache: dict[int, str | None] = {}

    # -- discovery ------------------------------------------------------------

    def start_apps(self) -> list[App]:
        """Everything in the Start menu's All apps list, Store apps included.

        Read from the Shell's AppsFolder over COM (D-090); PowerShell's Get-StartApps,
        which reads the same folder, is only the fallback.
        """
        try:
            apps = self._apps_folder()
        except Exception:
            apps = []
        return apps or self._start_apps_powershell()

    @staticmethod
    def _apps_folder() -> list[App]:
        import comtypes  # pyright: ignore[reportMissingImports]
        import comtypes.client  # pyright: ignore[reportMissingImports]

        with contextlib.suppress(OSError):
            comtypes.CoInitialize()  # this thread may not have COM yet
        shell = comtypes.client.CreateObject("Shell.Application", dynamic=True)
        folder = shell.NameSpace("shell:AppsFolder")
        if folder is None:
            return []
        apps = []
        items = folder.Items()
        for i in range(items.Count):
            item = items.Item(i)
            name, app_id = str(item.Name or ""), str(item.Path or "")
            if not name or not app_id or app_id.lower().startswith(("http:", "https:")):
                continue
            apps.append(App(name, app_id=app_id, exe=exe_from_app_id(app_id)))
        return apps

    def _start_apps_powershell(self) -> list[App]:
        out = subprocess.run(
            ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", _START_APPS_SCRIPT],
            capture_output=True,
            timeout=START_APPS_TIMEOUT,
            creationflags=_NO_WINDOW,
            check=False,
        )
        text = out.stdout.decode("utf-8", errors="replace").strip()
        if out.returncode != 0 or not text:
            return []
        rows = json.loads(text)
        rows = rows if isinstance(rows, list) else [rows]
        apps = []
        for row in rows:
            name, app_id = row.get("Name"), row.get("AppID")
            if not name or not app_id or app_id.lower().startswith(("http:", "https:")):
                continue
            apps.append(App(str(name), app_id=str(app_id), exe=exe_from_app_id(str(app_id))))
        return apps

    def app_paths(self) -> dict[str, str]:
        """Programs registered under App Paths (what Win+R finds by name)."""
        import winreg

        found: dict[str, str] = {}
        roots = (
            (winreg.HKEY_CURRENT_USER, _APP_PATHS),
            (winreg.HKEY_LOCAL_MACHINE, _APP_PATHS),
            (winreg.HKEY_LOCAL_MACHINE, _APP_PATHS.replace("SOFTWARE", r"SOFTWARE\WOW6432Node")),
        )
        for root, path in roots:
            for key in _reg_subkeys(root, path):
                if key.casefold() in found or not key.lower().endswith(".exe"):
                    continue
                value = _reg_value(root, f"{path}\\{key}")
                exe = os.path.expandvars(value.strip().strip('"')) if value else None
                if exe and os.path.isfile(exe):
                    found[key.casefold()] = exe
        return found

    def browsers(self) -> list[App]:
        """Registered web browsers (StartMenuInternet), the https handler marked default."""
        import winreg

        default_exe = None
        for scheme in ("https", "http"):
            prog_id = _reg_value(winreg.HKEY_CURRENT_USER, _USER_CHOICE.format(scheme), "ProgId")
            if prog_id:
                command = _reg_value(winreg.HKEY_CLASSES_ROOT, rf"{prog_id}\shell\open\command")
                default_exe = exe_from_command(command) if command else None
                if default_exe:
                    break
        found: list[App] = []
        for root in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
            for key in _reg_subkeys(root, _CLIENTS):
                command = _reg_value(root, rf"{_CLIENTS}\{key}\shell\open\command")
                exe = exe_from_command(command) if command else None
                if exe is None or any(a.exe and a.exe.casefold() == exe.casefold() for a in found):
                    continue
                label = _reg_value(root, rf"{_CLIENTS}\{key}")
                name = (indirect_string(label) if label else None) or Path(exe).stem
                is_default = default_exe is not None and default_exe.casefold() == exe.casefold()
                found.append(App(name, exe=exe, browser=True, default_browser=is_default))
        if default_exe and not any(a.default_browser for a in found):
            found.append(
                App(Path(default_exe).stem, exe=default_exe, browser=True, default_browser=True)
            )
        return found

    # -- launching ------------------------------------------------------------

    def launch(self, app: App, args: Sequence[str]) -> None:
        if app.exe and (args or app.app_id is None):
            cwd = str(Path(app.exe).parent)
            command = [app.exe, *args]
            kwargs: dict[str, Any] = {
                "cwd": cwd,
                "stdin": subprocess.DEVNULL,
                "stdout": subprocess.DEVNULL,
                "stderr": subprocess.DEVNULL,
                "close_fds": True,
            }
            try:
                subprocess.Popen(command, creationflags=_DETACHED | _BREAKAWAY, **kwargs)
            except OSError:  # the job forbids breakaway
                subprocess.Popen(command, creationflags=_DETACHED, **kwargs)
            return
        if app.app_id is None:
            raise OSError(f"{app.name} has neither a program path nor a Start menu entry")
        # Through the shell, as a Start menu click would: works for Store apps too.
        target = f"shell:AppsFolder\\{app.app_id}"
        if args:
            os.startfile(target, arguments=subprocess.list2cmdline(list(args)))  # type: ignore[attr-defined]
        else:
            os.startfile(target)  # type: ignore[attr-defined]

    # -- windows --------------------------------------------------------------

    def _exe_of(self, pid: int) -> str | None:
        if pid in self._exe_cache:
            return self._exe_cache[pid]
        exe = None
        handle = self._kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if handle:
            try:
                size = wintypes.DWORD(1024)
                buffer = ctypes.create_unicode_buffer(size.value)
                if self._kernel32.QueryFullProcessImageNameW(handle, 0, buffer, ctypes.byref(size)):
                    exe = buffer.value
            finally:
                self._kernel32.CloseHandle(handle)
        if len(self._exe_cache) > 2000:
            self._exe_cache.clear()
        self._exe_cache[pid] = exe
        return exe

    def _cloaked(self, hwnd: int) -> bool:
        value = wintypes.DWORD()
        result = self._dwm.DwmGetWindowAttribute(
            wintypes.HWND(hwnd), DWMWA_CLOAKED, ctypes.byref(value), ctypes.sizeof(value)
        )
        return result == 0 and value.value != 0

    def windows(self) -> list[Window]:
        user32 = self._user32
        found: list[Window] = []

        def callback(hwnd: int, _lparam: int) -> bool:
            if not user32.IsWindowVisible(hwnd) or user32.GetWindow(hwnd, GW_OWNER):
                return True
            if user32.GetWindowLongW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW:
                return True
            length = user32.GetWindowTextLengthW(hwnd)
            if length <= 0 or self._cloaked(hwnd):
                return True
            buffer = ctypes.create_unicode_buffer(length + 1)
            user32.GetWindowTextW(hwnd, buffer, length + 1)
            rect = wintypes.RECT()
            user32.GetWindowRect(hwnd, ctypes.byref(rect))
            if rect.right <= rect.left or rect.bottom <= rect.top:
                return True
            pid = wintypes.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            found.append(
                Window(
                    int(hwnd),
                    buffer.value,
                    Rect(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top),
                    self._exe_of(pid.value),
                )
            )
            return True

        proc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)(callback)  # pyright: ignore[reportAttributeAccessIssue]
        user32.EnumWindows(proc, 0)
        return found

    def foreground(self) -> int:
        return int(self._user32.GetForegroundWindow() or 0)

    def bring_to_front(self, handle: int) -> bool:
        """Activate a window the server's background process just started.

        Windows only lets the foreground process hand focus on, so borrow the
        foreground thread's input state for the call (AttachThreadInput).
        """
        user32 = self._user32
        if user32.IsIconic(handle):
            user32.ShowWindow(handle, SW_RESTORE)
        if user32.SetForegroundWindow(handle) and self.foreground() == handle:
            return True
        current = self.foreground()
        theirs = user32.GetWindowThreadProcessId(current, None) if current else 0
        mine = self._kernel32.GetCurrentThreadId()
        attached = bool(theirs and theirs != mine and user32.AttachThreadInput(mine, theirs, True))
        try:
            user32.BringWindowToTop(handle)
            user32.SetForegroundWindow(handle)
        finally:
            if attached:
                user32.AttachThreadInput(mine, theirs, False)
        return self.foreground() == handle
