"""Relay's approval prompt window (Windows; run as a child process by the gate).

Reads a JSON request on stdin, shows a small always-on-top window, and prints
``{"decision": "approve" | "approve_batch" | "deny" | "timeout"}``. While it is open,
synthetic mouse and keyboard input aimed at it is dropped by low-level hooks
(D-019), so only the owner's real mouse and keyboard can answer it. Afterwards the
previously active window gets the focus back, so Relay's next keystrokes land where
Cloud AI expects.

The window is plain Win32 through ctypes: the Python bundled with Relay has no
tkinter. ``--check`` builds the window without showing it and prints ``{"ok": true}``,
which the self-test uses to prove the prompt works without opening anything.
"""

from __future__ import annotations

import base64
import ctypes
import io
import json
import os
import sys
from ctypes import wintypes
from typing import Any

WH_KEYBOARD_LL = 13
WH_MOUSE_LL = 14
LLMHF_INJECTED = 0x01
LLKHF_INJECTED = 0x10

WS_POPUP = 0x80000000
WS_CAPTION = 0x00C00000
WS_SYSMENU = 0x00080000
WS_CHILD = 0x40000000
WS_VISIBLE = 0x10000000
WS_TABSTOP = 0x00010000
WS_EX_TOPMOST = 0x00000008
WS_EX_DLGMODALFRAME = 0x00000001
SS_LEFT = 0x0
SS_BITMAP = 0x0E
SS_NOPREFIX = 0x80
BS_PUSHBUTTON = 0x0
WM_DESTROY = 0x0002
WM_CLOSE = 0x0010
WM_SETFONT = 0x0030
WM_COMMAND = 0x0111
WM_TIMER = 0x0113
WM_CTLCOLORSTATIC = 0x0138
STM_SETIMAGE = 0x0172
IMAGE_BITMAP = 0
IDCANCEL = 2
SW_SHOWNORMAL = 1
SW_HIDE = 0
COLOR_WINDOW = 5
DT_WORDBREAK = 0x10
DT_CALCRECT = 0x400
DT_NOPREFIX = 0x800
FW_NORMAL = 400
FW_SEMIBOLD = 600
DEFAULT_CHARSET = 1
CLEARTYPE_QUALITY = 5
TRANSPARENT = 1
SM_CXSCREEN = 0
SM_CYSCREEN = 1
DIB_RGB_COLORS = 0
TIMER_ID = 1

ID_APPROVE = 10
ID_BATCH = 11
ID_DENY = IDCANCEL  # Esc and the close box both deny
ID_SECONDARY = 20  # grey text: the step line and the hint

CLASS_NAME = "RelayApprovalPrompt"
CONTENT_WIDTH = 560  # px at 96 DPI
MAX_IMAGE_HEIGHT = 300

LRESULT = ctypes.c_ssize_t
WNDPROC = ctypes.WINFUNCTYPE(
    LRESULT, wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM
)


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


class WNDCLASSEXW(ctypes.Structure):
    _fields_ = (
        ("cbSize", wintypes.UINT),
        ("style", wintypes.UINT),
        ("lpfnWndProc", WNDPROC),
        ("cbClsExtra", ctypes.c_int),
        ("cbWndExtra", ctypes.c_int),
        ("hInstance", wintypes.HINSTANCE),
        ("hIcon", wintypes.HICON),
        ("hCursor", wintypes.HANDLE),
        ("hbrBackground", wintypes.HBRUSH),
        ("lpszMenuName", wintypes.LPCWSTR),
        ("lpszClassName", wintypes.LPCWSTR),
        ("hIconSm", wintypes.HICON),
    )


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = (
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    )


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
    user32.UnhookWindowsHookEx.argtypes = (wintypes.HHOOK,)
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
        for hook in hooks:
            if hook:
                user32.UnhookWindowsHookEx(hook)
        raise OSError("could not install input hooks")
    return [on_mouse, on_key, *hooks]


def _bind(user32: Any, gdi32: Any, kernel32: Any) -> None:
    """Argument and return types, so handles survive on 64-bit Windows."""
    H, U, I = wintypes.HWND, wintypes.UINT, ctypes.c_int  # noqa: E741
    kernel32.GetModuleHandleW.restype = wintypes.HMODULE
    kernel32.GetModuleHandleW.argtypes = (wintypes.LPCWSTR,)
    user32.RegisterClassExW.argtypes = (ctypes.POINTER(WNDCLASSEXW),)
    user32.RegisterClassExW.restype = wintypes.ATOM
    user32.CreateWindowExW.argtypes = (
        wintypes.DWORD,
        wintypes.LPCWSTR,
        wintypes.LPCWSTR,
        wintypes.DWORD,
        I,
        I,
        I,
        I,
        H,
        wintypes.HMENU,
        wintypes.HINSTANCE,
        wintypes.LPVOID,
    )
    user32.CreateWindowExW.restype = H
    user32.DefWindowProcW.argtypes = (H, U, wintypes.WPARAM, wintypes.LPARAM)
    user32.DefWindowProcW.restype = LRESULT
    user32.SendMessageW.argtypes = (H, U, wintypes.WPARAM, wintypes.LPARAM)
    user32.SendMessageW.restype = LRESULT
    user32.GetDlgCtrlID.argtypes = (H,)
    user32.DestroyWindow.argtypes = (H,)
    user32.ShowWindow.argtypes = (H, I)
    user32.SetForegroundWindow.argtypes = (H,)
    user32.SetFocus.argtypes = (H,)
    user32.SetFocus.restype = H
    user32.SetTimer.argtypes = (H, ctypes.c_size_t, U, wintypes.LPVOID)
    user32.SetTimer.restype = ctypes.c_size_t
    user32.KillTimer.argtypes = (H, ctypes.c_size_t)
    user32.GetMessageW.argtypes = (ctypes.POINTER(wintypes.MSG), H, U, U)
    user32.IsDialogMessageW.argtypes = (H, ctypes.POINTER(wintypes.MSG))
    user32.TranslateMessage.argtypes = (ctypes.POINTER(wintypes.MSG),)
    user32.DispatchMessageW.argtypes = (ctypes.POINTER(wintypes.MSG),)
    user32.DispatchMessageW.restype = LRESULT
    user32.AdjustWindowRectEx.argtypes = (
        ctypes.POINTER(wintypes.RECT),
        wintypes.DWORD,
        wintypes.BOOL,
        wintypes.DWORD,
    )
    user32.GetDC.argtypes = (H,)
    user32.GetDC.restype = wintypes.HDC
    user32.ReleaseDC.argtypes = (H, wintypes.HDC)
    user32.DrawTextW.argtypes = (
        wintypes.HDC,
        wintypes.LPCWSTR,
        I,
        ctypes.POINTER(wintypes.RECT),
        U,
    )
    user32.GetSysColorBrush.argtypes = (I,)
    user32.GetSysColorBrush.restype = wintypes.HBRUSH
    user32.LoadCursorW.argtypes = (wintypes.HINSTANCE, wintypes.LPVOID)
    user32.LoadCursorW.restype = wintypes.HANDLE
    user32.UnregisterClassW.argtypes = (wintypes.LPCWSTR, wintypes.HINSTANCE)
    gdi32.CreateFontW.argtypes = (I,) * 5 + (wintypes.DWORD,) * 8 + (wintypes.LPCWSTR,)
    gdi32.CreateFontW.restype = wintypes.HFONT
    gdi32.SelectObject.argtypes = (wintypes.HDC, wintypes.HGDIOBJ)
    gdi32.SelectObject.restype = wintypes.HGDIOBJ
    gdi32.DeleteObject.argtypes = (wintypes.HGDIOBJ,)
    gdi32.SetTextColor.argtypes = (wintypes.HDC, wintypes.COLORREF)
    gdi32.SetBkMode.argtypes = (wintypes.HDC, I)
    gdi32.CreateDIBSection.argtypes = (
        wintypes.HDC,
        ctypes.POINTER(BITMAPINFOHEADER),
        U,
        ctypes.POINTER(ctypes.c_void_p),
        wintypes.HANDLE,
        wintypes.DWORD,
    )
    gdi32.CreateDIBSection.restype = wintypes.HBITMAP


def _rgb(r: int, g: int, b: int) -> int:
    return r | (g << 8) | (b << 16)


def _bitmap(gdi32: Any, png: bytes, max_w: int, max_h: int) -> tuple[Any, int, int]:
    """A DIB section holding the preview, shrunk to fit. Returns (handle, w, h)."""
    from PIL import Image

    img = Image.open(io.BytesIO(png)).convert("RGB")
    img.thumbnail((max_w, max_h))
    w, h = img.size
    header = BITMAPINFOHEADER(
        biSize=ctypes.sizeof(BITMAPINFOHEADER), biWidth=w, biHeight=-h, biPlanes=1, biBitCount=32
    )
    bits = ctypes.c_void_p()
    handle = gdi32.CreateDIBSection(
        None, ctypes.byref(header), DIB_RGB_COLORS, ctypes.byref(bits), None, 0
    )
    if not handle or not bits.value:
        raise OSError("could not create the preview bitmap")
    data = img.convert("RGBA").tobytes("raw", "BGRA")
    ctypes.memmove(bits.value, data, len(data))
    return handle, w, h


def show(request: dict[str, Any], visible: bool = True) -> str:
    from relay_screen.dpi import set_dpi_awareness

    set_dpi_awareness()
    user32 = ctypes.WinDLL("user32", use_last_error=True)
    gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _bind(user32, gdi32, kernel32)
    instance = kernel32.GetModuleHandleW(None)

    dpi = user32.GetDpiForSystem() or 96

    def px(value: float) -> int:
        return round(value * dpi / 96)

    def font(points: int, weight: int) -> Any:
        height = -round(points * dpi / 72)
        return gdi32.CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            0,
            0,
            CLEARTYPE_QUALITY,
            0,
            "Segoe UI",
        )

    fonts = {"title": font(13, FW_SEMIBOLD), "body": font(10, FW_NORMAL)}
    white = user32.GetSysColorBrush(COLOR_WINDOW)
    result = {"decision": "timeout"}
    handles: dict[str, Any] = {}

    @WNDPROC
    def wndproc(hwnd: Any, msg: int, wparam: int, lparam: int) -> int:
        if msg == WM_COMMAND:
            control = wparam & 0xFFFF
            decision = {ID_APPROVE: "approve", ID_BATCH: "approve_batch", ID_DENY: "deny"}.get(
                control
            )
            if decision:
                result["decision"] = decision
                user32.DestroyWindow(hwnd)
                return 0
        elif msg == WM_CLOSE:
            result["decision"] = "deny"
            user32.DestroyWindow(hwnd)
            return 0
        elif msg == WM_TIMER and wparam == TIMER_ID:
            result["decision"] = "timeout"
            user32.DestroyWindow(hwnd)
            return 0
        elif msg == WM_CTLCOLORSTATIC:
            grey = user32.GetDlgCtrlID(lparam) == ID_SECONDARY
            gdi32.SetTextColor(wparam, _rgb(102, 112, 133) if grey else _rgb(16, 24, 40))
            gdi32.SetBkMode(wparam, TRANSPARENT)
            return white or 0
        elif msg == WM_DESTROY:
            user32.KillTimer(hwnd, TIMER_ID)
            user32.PostQuitMessage(0)
            return 0
        return user32.DefWindowProcW(hwnd, msg, wparam, lparam)

    cls = WNDCLASSEXW(
        cbSize=ctypes.sizeof(WNDCLASSEXW),
        lpfnWndProc=wndproc,
        hInstance=instance,
        hCursor=user32.LoadCursorW(None, ctypes.c_void_p(32512)),  # IDC_ARROW
        hbrBackground=white,
        lpszClassName=CLASS_NAME,
    )
    if not user32.RegisterClassExW(ctypes.byref(cls)):
        raise ctypes.WinError(ctypes.get_last_error())

    # Measure the text so the window fits it at this DPI.
    dc = user32.GetDC(None)

    def measure(text: str, which: str, width: int) -> tuple[int, int]:
        old = gdi32.SelectObject(dc, fonts[which])
        rect = wintypes.RECT(0, 0, width, 0)
        user32.DrawTextW(dc, text, -1, ctypes.byref(rect), DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX)
        gdi32.SelectObject(dc, old)
        return rect.right - rect.left, rect.bottom - rect.top

    margin, gap, width = px(20), px(8), px(CONTENT_WIDTH)
    title = str(request.get("title") or "Relay: approval needed")
    detail = str(request.get("detail") or "")
    step = (
        f"Step {request['step']} of {request['total']} in this batch"
        if request.get("step") and request.get("total")
        else ""
    )
    hint = "Only your own mouse or keyboard can answer."
    labels = [(ID_APPROVE, str(request.get("approve_label") or "Approve"))]
    if request.get("allow_batch"):
        labels.append((ID_BATCH, str(request.get("batch_label") or "Approve rest of batch")))
    labels.append((ID_DENY, "Deny"))
    button_h = px(32)
    button_w = [max(px(96), measure(text, "body", width)[0] + px(28)) for _, text in labels]
    hint_w, hint_h = measure(hint, "body", width - sum(button_w) - gap * len(labels))
    title_h = measure(title, "title", width)[1]
    step_h = measure(step, "body", width)[1] if step else 0
    detail_h = measure(detail, "body", width)[1] if detail else 0
    user32.ReleaseDC(None, dc)

    bitmap = None
    image_w = image_h = 0
    if request.get("image"):
        bitmap, image_w, image_h = _bitmap(
            gdi32, base64.b64decode(request["image"]), width - 2, px(MAX_IMAGE_HEIGHT)
        )
        handles["bitmap"] = bitmap

    footer_h = max(button_h, hint_h)
    height = margin + title_h
    height += (gap // 2 + step_h) if step_h else 0
    height += (gap + detail_h) if detail_h else 0
    height += (gap + image_h) if bitmap else 0
    height += margin + footer_h + margin

    style = WS_POPUP | WS_CAPTION | WS_SYSMENU
    ex_style = WS_EX_TOPMOST | WS_EX_DLGMODALFRAME
    frame = wintypes.RECT(0, 0, width + 2 * margin, height)
    user32.AdjustWindowRectEx(ctypes.byref(frame), style, False, ex_style)
    outer_w, outer_h = frame.right - frame.left, frame.bottom - frame.top
    # Centre on the primary display (it starts at 0, 0).
    x = max(0, (user32.GetSystemMetrics(SM_CXSCREEN) - outer_w) // 2)
    y = max(0, (user32.GetSystemMetrics(SM_CYSCREEN) - outer_h) // 3)
    hwnd = user32.CreateWindowExW(
        ex_style,
        CLASS_NAME,
        "Relay: approval needed",
        style,
        x,
        y,
        outer_w,
        outer_h,
        None,
        None,
        instance,
        None,
    )
    if not hwnd:
        raise ctypes.WinError(ctypes.get_last_error())

    def child(
        cls_name: str, text: str, style: int, box: tuple[int, int, int, int], ident: int, which: str
    ) -> Any:
        handle = user32.CreateWindowExW(
            0,
            cls_name,
            text,
            WS_CHILD | WS_VISIBLE | style,
            *box,
            hwnd,
            wintypes.HMENU(ident),
            instance,
            None,
        )
        if not handle:
            raise ctypes.WinError(ctypes.get_last_error())
        user32.SendMessageW(handle, WM_SETFONT, fonts[which], 1)
        return handle

    top = margin
    child("STATIC", title, SS_LEFT | SS_NOPREFIX, (margin, top, width, title_h), 0, "title")
    top += title_h
    if step_h:
        top += gap // 2
        child(
            "STATIC",
            step,
            SS_LEFT | SS_NOPREFIX,
            (margin, top, width, step_h),
            ID_SECONDARY,
            "body",
        )
        top += step_h
    if detail_h:
        top += gap
        child("STATIC", detail, SS_LEFT | SS_NOPREFIX, (margin, top, width, detail_h), 0, "body")
        top += detail_h
    if bitmap:
        top += gap
        image = child("STATIC", "", SS_BITMAP, (margin, top, image_w, image_h), 0, "body")
        user32.SendMessageW(image, STM_SETIMAGE, IMAGE_BITMAP, bitmap)
        top += image_h
    top += margin
    child(
        "STATIC",
        hint,
        SS_LEFT | SS_NOPREFIX,
        (margin, top + (footer_h - hint_h) // 2, hint_w, hint_h),
        ID_SECONDARY,
        "body",
    )
    right = margin + width
    first = None
    for (ident, text), w in reversed(list(zip(labels, button_w, strict=True))):
        right -= w
        button = child(
            "BUTTON", text, BS_PUSHBUTTON | WS_TABSTOP, (right, top, w, button_h), ident, "body"
        )
        right -= gap
        if ident == ID_APPROVE:
            first = button

    keep = _install_hooks(user32, kernel32) if visible else []
    try:
        if visible:
            user32.SetTimer(hwnd, TIMER_ID, int(float(request.get("timeout", 120)) * 1000), None)
            user32.ShowWindow(hwnd, SW_SHOWNORMAL)
            user32.SetForegroundWindow(hwnd)
            if first:
                user32.SetFocus(first)
        else:
            result["decision"] = "ok"
            user32.DestroyWindow(hwnd)
        msg = wintypes.MSG()
        while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            if not user32.IsDialogMessageW(hwnd, ctypes.byref(msg)):
                user32.TranslateMessage(ctypes.byref(msg))
                user32.DispatchMessageW(ctypes.byref(msg))
    finally:
        for hook in keep[2:]:
            user32.UnhookWindowsHookEx(hook)
        for handle in [*fonts.values(), *handles.values()]:
            gdi32.DeleteObject(handle)
        user32.UnregisterClassW(CLASS_NAME, instance)

    restore = int(request.get("restore_hwnd") or 0)
    if visible and restore:
        user32.SetForegroundWindow(wintypes.HWND(restore))
    return result["decision"]


def check() -> None:
    """Build the prompt off screen and tear it down: proves it can be shown."""
    tiny = base64.b64encode(_sample_png()).decode()
    request = {
        "title": "Self-test",
        "detail": "Not shown.",
        "step": 1,
        "total": 2,
        "allow_batch": True,
        "image": tiny,
    }
    print(json.dumps({"ok": show(request, visible=False) == "ok"}))


def _sample_png() -> bytes:
    from PIL import Image

    out = io.BytesIO()
    Image.new("RGB", (8, 8), (255, 255, 255)).save(out, "PNG")
    return out.getvalue()


def main() -> None:
    if "--check" in sys.argv[1:]:
        try:
            check()
        except Exception as exc:
            print(json.dumps({"ok": False, "error": f"{type(exc).__name__}: {exc}"}))
            sys.exit(1)
        return
    request = json.loads(sys.stdin.read())
    if request.get("image"):
        base64.b64decode(request["image"], validate=True)  # reject garbage early
    try:
        decision = show(request)
    except Exception as exc:  # report it; the gate tells Cloud AI the prompt failed
        print(json.dumps({"decision": "error", "error": f"{type(exc).__name__}: {exc}"}))
        return
    print(json.dumps({"decision": decision}))


if __name__ == "__main__":
    main()
