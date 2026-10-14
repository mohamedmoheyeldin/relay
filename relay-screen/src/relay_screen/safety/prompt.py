"""Relay's approval prompt window (Windows; run as a child process by the gate).

Reads a JSON request on stdin, shows a small always-on-top window, and prints
``{"decision": "approve" | "approve_batch" | "deny" | "timeout"}``. While it is open,
synthetic mouse and keyboard input aimed at it is dropped by low-level hooks
(D-019), so only the owner's real mouse and keyboard can answer it. Afterwards the
previously active window gets the focus back, so Relay's next keystrokes land where
Claude expects.
"""

from __future__ import annotations

import base64
import ctypes
import json
import os
import sys
from ctypes import wintypes
from typing import Any

WH_KEYBOARD_LL = 13
WH_MOUSE_LL = 14
LLMHF_INJECTED = 0x01
LLKHF_INJECTED = 0x10
GA_ROOT = 2


class MSLLHOOKSTRUCT(ctypes.Structure):
    _fields_ = (
        ("pt", wintypes.POINT),
        ("mouseData", wintypes.DWORD),
        ("flags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ctypes.c_size_t),
    )


class KBDLLHOOKSTRUCT(ctypes.Structure):
    _fields_ = (
        ("vkCode", wintypes.DWORD),
        ("scanCode", wintypes.DWORD),
        ("flags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ctypes.c_size_t),
    )


LRESULT = ctypes.c_ssize_t


def _install_hooks(user32: Any, kernel32: Any) -> list[Any]:
    """Drop injected input that targets this process's windows. Returns keep-alives."""
    hook_proc = ctypes.WINFUNCTYPE(LRESULT, ctypes.c_int, wintypes.WPARAM, wintypes.LPARAM)
    user32.CallNextHookEx.argtypes = (
        wintypes.HHOOK,
        ctypes.c_int,
        wintypes.WPARAM,
        wintypes.LPARAM,
    )
    user32.CallNextHookEx.restype = LRESULT
    user32.SetWindowsHookExW.restype = wintypes.HHOOK
    user32.SetWindowsHookExW.argtypes = (
        ctypes.c_int,
        hook_proc,
        wintypes.HINSTANCE,
        wintypes.DWORD,
    )
    user32.WindowFromPoint.argtypes = (wintypes.POINT,)
    user32.WindowFromPoint.restype = wintypes.HWND
    user32.GetForegroundWindow.restype = wintypes.HWND
    user32.GetWindowThreadProcessId.argtypes = (wintypes.HWND, ctypes.POINTER(wintypes.DWORD))
    pid = os.getpid()

    def ours(hwnd: int | None) -> bool:
        if not hwnd:
            return False
        owner = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        return owner.value == pid

    @hook_proc
    def on_mouse(code: int, wparam: int, lparam: int) -> int:
        if code >= 0:
            info = ctypes.cast(lparam, ctypes.POINTER(MSLLHOOKSTRUCT)).contents
            if info.flags & LLMHF_INJECTED and ours(user32.WindowFromPoint(info.pt)):
                return 1
        return user32.CallNextHookEx(None, code, wparam, lparam)

    @hook_proc
    def on_key(code: int, wparam: int, lparam: int) -> int:
        if code >= 0:
            info = ctypes.cast(lparam, ctypes.POINTER(KBDLLHOOKSTRUCT)).contents
            if info.flags & LLKHF_INJECTED and ours(user32.GetForegroundWindow()):
                return 1
        return user32.CallNextHookEx(None, code, wparam, lparam)

    module = kernel32.GetModuleHandleW(None)
    hooks = [
        user32.SetWindowsHookExW(WH_MOUSE_LL, on_mouse, module, 0),
        user32.SetWindowsHookExW(WH_KEYBOARD_LL, on_key, module, 0),
    ]
    if not all(hooks):
        raise OSError("could not install input hooks")
    return [on_mouse, on_key, *hooks]


def show(request: dict[str, Any]) -> str:
    import tkinter as tk

    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.GetModuleHandleW.restype = wintypes.HMODULE
    keep = _install_hooks(user32, kernel32)

    result = {"decision": "timeout"}
    root = tk.Tk()
    root.title("Relay: approval needed")
    root.attributes("-topmost", True)
    root.resizable(False, False)
    root.configure(padx=16, pady=14)

    def decide(decision: str) -> None:
        result["decision"] = decision
        root.destroy()

    root.protocol("WM_DELETE_WINDOW", lambda: decide("deny"))
    font = ("Segoe UI", 11)
    tk.Label(root, text=request["title"], font=("Segoe UI", 13, "bold")).pack(anchor="w")
    if request.get("step") and request.get("total"):
        step = f"Step {request['step']} of {request['total']} in this batch"
        tk.Label(root, text=step, font=font, fg="#555").pack(anchor="w")
    tk.Label(root, text=request["detail"], font=font, wraplength=620, justify="left").pack(
        anchor="w", pady=(8, 8)
    )
    if request.get("image"):
        photo = tk.PhotoImage(data=request["image"])  # PNG, base64
        tk.Label(root, image=photo, borderwidth=1, relief="solid").pack(pady=(0, 10))
        keep.append(photo)

    buttons = tk.Frame(root)
    buttons.pack(fill="x")
    tk.Button(buttons, text="Deny", width=10, command=lambda: decide("deny")).pack(side="right")
    if request.get("allow_batch"):
        tk.Button(
            buttons, text="Approve rest of batch", command=lambda: decide("approve_batch")
        ).pack(side="right", padx=(0, 8))
    tk.Button(
        buttons,
        text=request.get("approve_label") or "Approve",
        width=12,
        command=lambda: decide("approve"),
    ).pack(side="right", padx=(0, 8))
    tk.Label(buttons, text="Only your own mouse or keyboard can answer.", fg="#777").pack(
        side="left"
    )

    root.update_idletasks()
    # Centre on the primary display (it starts at 0, 0).
    screen_w = user32.GetSystemMetrics(0)
    screen_h = user32.GetSystemMetrics(1)
    x = (screen_w - root.winfo_width()) // 2
    y = (screen_h - root.winfo_height()) // 3
    root.geometry(f"+{max(0, x)}+{max(0, y)}")
    root.after(int(float(request.get("timeout", 120)) * 1000), lambda: decide("timeout"))
    root.lift()
    root.focus_force()
    root.mainloop()

    restore = int(request.get("restore_hwnd") or 0)
    if restore:
        user32.SetForegroundWindow(wintypes.HWND(restore))
    del keep
    return result["decision"]


def main() -> None:
    request = json.loads(sys.stdin.read())
    if request.get("image"):
        base64.b64decode(request["image"], validate=True)  # reject garbage early
    try:
        decision = show(request)
    except Exception as exc:  # report, and count it as a denial
        print(json.dumps({"decision": "deny", "error": f"{type(exc).__name__}: {exc}"}))
        return
    print(json.dumps({"decision": decision}))


if __name__ == "__main__":
    main()
