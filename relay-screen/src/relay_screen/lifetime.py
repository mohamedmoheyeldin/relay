"""Exit when the process that started us is gone (D-105).

Cloud AI starts relay-screen as an MCP server for each `cloud-cli -p` run. When that
run is stopped or times out, only cloud-cli.exe is killed, and on Windows its children
live on: the round-5 run found 11 of them holding 2.2 GB. So relay-screen watches its
parent and leaves with it, and leaves as soon as its stdin closes.
"""

from __future__ import annotations

import os
import sys
import threading
import time
from collections.abc import Callable

SYNCHRONIZE = 0x00100000
INFINITE = 0xFFFFFFFF
POLL = 2.0


def _windows_waiter(pid: int) -> Callable[[], None] | None:
    """A call that blocks until `pid` exits, or None if it can't be opened."""
    import ctypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)  # type: ignore[attr-defined]
    kernel32.OpenProcess.restype = ctypes.c_void_p
    handle = kernel32.OpenProcess(SYNCHRONIZE, False, pid)
    if not handle:
        return None
    kernel32.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    return lambda: kernel32.WaitForSingleObject(handle, INFINITE)


def _poll_waiter(pid: int, getppid: Callable[[], int], sleep: Callable[[float], None]):
    def wait() -> None:
        while getppid() == pid:
            sleep(POLL)

    return wait


def exit_with_parent(
    exit: Callable[[int], object] = os._exit,
    getppid: Callable[[], int] = os.getppid,
    sleep: Callable[[float], None] = time.sleep,
    windows: bool = sys.platform == "win32",
) -> threading.Thread | None:
    """Start a daemon thread that ends this process once its parent has exited."""
    parent = getppid()
    if parent <= 1:  # already orphaned, or started by init: nothing to follow
        return None
    wait = _windows_waiter(parent) if windows else None
    if wait is None:
        wait = _poll_waiter(parent, getppid, sleep)

    def watch() -> None:
        wait()
        exit(0)

    thread = threading.Thread(target=watch, name="relay-exit-with-parent", daemon=True)
    thread.start()
    return thread
