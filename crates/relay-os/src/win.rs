//! Windows implementation of [`Desktop`] (GDI capture, `SendInput`).
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC, HGDIOBJ, HMONITOR, MONITORINFO,
    SRCCOPY,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, VkKeyScanW, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, MOUSE_EVENT_FLAGS,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
    GetWindowTextW, IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow,
    MONITORINFOF_PRIMARY, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SW_RESTORE,
};

use crate::scaling::normalize_absolute;
use crate::types::{
    CharKey, Desktop, DisplayInfo, Frame, InputEvent, MouseButton, OsError, Rect, Result,
    WindowInfo,
};

/// The real Windows desktop.
pub struct WindowsDesktop;

impl WindowsDesktop {
    /// Make the process per-monitor DPI aware (so every rectangle is in physical pixels).
    pub fn new() -> Self {
        // SAFETY: plain Win32 call with a valid constant; failure only means already set.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        Self
    }
}

impl Default for WindowsDesktop {
    fn default() -> Self {
        Self::new()
    }
}

fn platform<E: std::fmt::Display>(what: &str, e: E) -> OsError {
    OsError::Platform(format!("{what}: {e}"))
}

unsafe extern "system" fn monitor_cb(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the address of the Vec passed by `displays` for the duration of the call.
    let out = unsafe { &mut *(data.0 as *mut Vec<DisplayInfo>) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is correctly sized and initialised.
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        let r = info.rcMonitor;
        out.push(DisplayInfo {
            id: 0,
            rect: Rect {
                x: r.left,
                y: r.top,
                w: (r.right - r.left).max(0) as u32,
                h: (r.bottom - r.top).max(0) as u32,
            },
            primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL::from(true)
}

struct WindowScan {
    foreground: HWND,
    found: Vec<(HWND, WindowInfo)>,
}

unsafe extern "system" fn window_cb(hwnd: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` points at the WindowScan owned by `scan_windows`.
    let scan = unsafe { &mut *(data.0 as *mut WindowScan) };
    // SAFETY: plain Win32 queries on a handle supplied by EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL::from(true);
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return BOOL::from(true);
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return BOOL::from(true);
        }
        scan.found.push((
            hwnd,
            WindowInfo {
                title,
                rect: Rect {
                    x: r.left,
                    y: r.top,
                    w: (r.right - r.left).max(0) as u32,
                    h: (r.bottom - r.top).max(0) as u32,
                },
                foreground: hwnd == scan.foreground,
            },
        ));
    }
    BOOL::from(true)
}

pub(crate) fn scan_windows() -> Result<Vec<(HWND, WindowInfo)>> {
    // SAFETY: GetForegroundWindow has no preconditions.
    let mut scan = WindowScan {
        foreground: unsafe { GetForegroundWindow() },
        found: Vec::new(),
    };
    // SAFETY: the callback only dereferences the pointer during this call.
    unsafe {
        EnumWindows(
            Some(window_cb),
            LPARAM(&mut scan as *mut WindowScan as isize),
        )
        .map_err(|e| platform("EnumWindows", e))?;
    }
    Ok(scan.found)
}

fn key_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn mouse_input(dx: i32, dy: i32, data: u32, flags: MOUSE_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn dispatch(input: INPUT) -> Result<()> {
    // SAFETY: a single, fully initialised INPUT with its true size.
    let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err(OsError::Platform(
            "SendInput was blocked (the target window may be elevated)".into(),
        ))
    }
}

/// Accessible name of the UI Automation element at a physical point (its parent's if it has none).
fn control_name(x: i32, y: i32) -> Result<Option<String>> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};

    // SAFETY: COM initialisation per thread is idempotent; S_FALSE just means already initialised.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| platform("UI Automation", e))?;
        let element = automation
            .ElementFromPoint(POINT { x, y })
            .map_err(|e| platform("ElementFromPoint", e))?;
        let name = element
            .CurrentName()
            .map(|b| b.to_string())
            .unwrap_or_default();
        if !name.trim().is_empty() {
            return Ok(Some(name));
        }
        let parent_name = automation
            .ControlViewWalker()
            .and_then(|walker| walker.GetParentElement(&element))
            .and_then(|parent| parent.CurrentName())
            .map(|b| b.to_string())
            .unwrap_or_default();
        Ok(if parent_name.trim().is_empty() {
            None
        } else {
            Some(parent_name)
        })
    }
}

impl Desktop for WindowsDesktop {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        let mut found: Vec<DisplayInfo> = Vec::new();
        // SAFETY: callback only touches `found` during the call.
        let ok = unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(monitor_cb),
                LPARAM(&mut found as *mut Vec<DisplayInfo> as isize),
            )
        };
        if !ok.as_bool() || found.is_empty() {
            return Err(OsError::Platform("no displays found".into()));
        }
        found.sort_by_key(|d| (!d.primary, d.rect.x, d.rect.y));
        for (i, d) in found.iter_mut().enumerate() {
            d.id = i as u32 + 1;
        }
        Ok(found)
    }

    fn virtual_desktop(&self) -> Result<Rect> {
        // SAFETY: GetSystemMetrics has no preconditions.
        let (x, y, w, h) = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        if w <= 0 || h <= 0 {
            return Err(OsError::Platform("virtual desktop has no size".into()));
        }
        Ok(Rect {
            x,
            y,
            w: w as u32,
            h: h as u32,
        })
    }

    fn capture(&self, rect: Rect) -> Result<Frame> {
        let (w, h) = (rect.w as i32, rect.h as i32);
        // SAFETY: standard GDI capture; every handle created here is released before returning.
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err(OsError::Platform("GetDC failed".into()));
            }
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(mem, HGDIOBJ(bmp.0));
            let blt = BitBlt(
                mem,
                0,
                0,
                w,
                h,
                Some(screen),
                rect.x,
                rect.y,
                SRCCOPY | CAPTUREBLT,
            );
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgra = vec![0u8; rect.w as usize * rect.h as usize * 4];
            let lines = GetDIBits(
                mem,
                bmp,
                0,
                rect.h,
                Some(bgra.as_mut_ptr() as *mut c_void),
                &mut info,
                DIB_RGB_COLORS,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
            blt.map_err(|e| platform("BitBlt", e))?;
            if lines == 0 {
                return Err(OsError::Platform("GetDIBits returned no lines".into()));
            }
            for px in bgra.chunks_exact_mut(4) {
                px.swap(0, 2);
                px[3] = 255;
            }
            Ok(Frame {
                width: rect.w,
                height: rect.h,
                rgba: bgra,
            })
        }
    }

    fn cursor_position(&self) -> Result<(i32, i32)> {
        let mut p = windows::Win32::Foundation::POINT::default();
        // SAFETY: `p` is a valid out pointer.
        unsafe { GetCursorPos(&mut p) }.map_err(|e| platform("GetCursorPos", e))?;
        Ok((p.x, p.y))
    }

    fn send(&self, event: InputEvent) -> Result<()> {
        let input = match event {
            InputEvent::MoveTo { x, y } => {
                let (nx, ny) = normalize_absolute(x, y, &self.virtual_desktop()?)?;
                mouse_input(
                    nx,
                    ny,
                    0,
                    MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                )
            }
            InputEvent::Button { button, down } => {
                let flag = match (button, down) {
                    (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                    (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                    (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                    (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                    (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                    (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                };
                mouse_input(0, 0, 0, flag)
            }
            InputEvent::Wheel { horizontal, delta } => mouse_input(
                0,
                0,
                delta as u32,
                if horizontal {
                    MOUSEEVENTF_HWHEEL
                } else {
                    MOUSEEVENTF_WHEEL
                },
            ),
            InputEvent::Vk { vk, extended, down } => {
                let mut flags = KEYBD_EVENT_FLAGS(0);
                if extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                // SAFETY: MapVirtualKeyW has no preconditions.
                let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
                key_input(vk, scan, flags)
            }
            InputEvent::Unicode { unit, down } => {
                let mut flags = KEYEVENTF_UNICODE;
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                key_input(0, unit, flags)
            }
        };
        dispatch(input)
    }

    fn resolve_char(&self, ch: char) -> Option<CharKey> {
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        if units.len() != 1 {
            return None;
        }
        // SAFETY: VkKeyScanW has no preconditions.
        let scan = unsafe { VkKeyScanW(units[0]) };
        let vk = (scan & 0xFF) as u16;
        let state = ((scan >> 8) & 0xFF) as u8;
        if scan == -1 || vk == 0xFF || state & 0b1000 != 0 {
            return None;
        }
        Some(CharKey {
            vk,
            shift: state & 1 != 0,
            ctrl: state & 2 != 0,
            alt: state & 4 != 0,
        })
    }

    fn control_name_at(&self, x: i32, y: i32) -> Result<Option<String>> {
        control_name(x, y)
    }

    fn ui_find(&self, selector: &crate::UiSelector, max: usize) -> Result<Vec<crate::UiElement>> {
        crate::win_uia::find(selector, max)
    }

    fn ui_invoke(&self, selector: &crate::UiSelector) -> Result<String> {
        crate::win_uia::invoke(self, selector)
    }

    fn ui_set_value(&self, selector: &crate::UiSelector, text: &str) -> Result<String> {
        crate::win_uia::set_value(self, selector, text)
    }

    fn ui_outline(&self, window: Option<&str>, max_nodes: usize) -> Result<String> {
        crate::win_uia::outline(window, max_nodes)
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(scan_windows()?.into_iter().map(|(_, w)| w).collect())
    }

    fn focus_window(&self, needle: &str) -> Result<String> {
        let needle_lc = needle.to_lowercase();
        let (hwnd, info) = scan_windows()?
            .into_iter()
            .find(|(_, w)| w.title.to_lowercase().contains(&needle_lc))
            .ok_or_else(|| {
                OsError::InvalidArgument(format!("no window title contains {needle:?}"))
            })?;
        // A synthetic Alt tap lets SetForegroundWindow succeed from a background process.
        let _ = dispatch(key_input(0x12, 0, KEYBD_EVENT_FLAGS(0)));
        let _ = dispatch(key_input(0x12, 0, KEYEVENTF_KEYUP));
        // SAFETY: `hwnd` came from EnumWindows moments ago.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            if !SetForegroundWindow(hwnd).as_bool() {
                return Err(OsError::Platform(format!(
                    "Windows refused to focus {:?}",
                    info.title
                )));
            }
        }
        Ok(info.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_at_least_one_display_and_resolves_letters() {
        let d = WindowsDesktop::new();
        let displays = d.displays().unwrap();
        assert!(displays[0].primary);
        assert!(displays[0].rect.w > 0);
        let vd = d.virtual_desktop().unwrap();
        assert!(vd.w >= displays[0].rect.w);
        assert!(d.resolve_char('a').is_some() || d.resolve_char('A').is_some());
        assert!(d.resolve_char('😀').is_none());
    }

    #[test]
    fn captures_a_small_region_with_opaque_pixels() {
        let d = WindowsDesktop::new();
        let primary = d.displays().unwrap().remove(0);
        let frame = d
            .capture(Rect {
                x: primary.rect.x,
                y: primary.rect.y,
                w: 64,
                h: 48,
            })
            .unwrap();
        assert_eq!(frame.rgba.len(), 64 * 48 * 4);
        assert!(frame.rgba.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn reads_a_control_name_without_error() {
        let d = WindowsDesktop::new();
        let primary = d.displays().unwrap().remove(0);
        // Some screen spots (for example a locked desktop or a transient overlay) cannot be queried;
        // the call must report that as an error, never crash.
        let result = d.control_name_at(primary.rect.x + 5, primary.rect.y + 5);
        assert!(
            result.is_ok() || matches!(result, Err(OsError::Platform(_))),
            "{result:?}"
        );
    }

    #[test]
    fn reads_the_ui_tree_of_a_window_without_changing_anything() {
        let d = WindowsDesktop::new();
        // Some windows (elevated, protected or mid-redraw) refuse UI Automation, so try a few;
        // at least one ordinary window must work. This only reads.
        let windows: Vec<_> = d
            .windows()
            .unwrap()
            .into_iter()
            .filter(|w| !w.title.is_empty())
            .take(8)
            .collect();
        if windows.is_empty() {
            return;
        }
        let mut worked = 0;
        for w in &windows {
            let sel = crate::UiSelector {
                window: Some(w.title.clone()),
                role: Some("button".into()),
                ..Default::default()
            };
            if let (Ok(_), Ok(found)) = (d.ui_outline(Some(&w.title), 40), d.ui_find(&sel, 5)) {
                assert!(found.iter().all(|e| e.role == "button"));
                worked += 1;
            }
        }
        assert!(worked > 0, "no window could be read through UI Automation");
    }

    #[test]
    fn enumerates_windows_without_error() {
        assert!(WindowsDesktop::new().windows().is_ok());
    }

    #[test]
    #[ignore = "moves the real mouse"]
    fn moves_the_real_cursor() {
        let d = WindowsDesktop::new();
        let primary = d.displays().unwrap().remove(0);
        let target = (primary.rect.x + 200, primary.rect.y + 200);
        d.send(InputEvent::MoveTo {
            x: target.0,
            y: target.1,
        })
        .unwrap();
        let (x, y) = d.cursor_position().unwrap();
        assert!((x - target.0).abs() <= 2 && (y - target.1).abs() <= 2);
    }
}
