//! The floating Relay popup: a small pill at the bottom of the screen that shows what
//! Relay is doing while its main window is out of the way, with Stop and a box to
//! tell it something new. Ctrl+Shift+Space opens it at any time.
//!
//! It never takes focus from the program Relay is working in until the owner clicks
//! its text box, and it is left out of screen capture, so the local operator neither
//! sees nor clicks it. Injected input aimed at it is dropped by the input guard in
//! `approvals.rs` like for every Relay window.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "popup";
const WIDTH: f64 = 460.0;
const HEIGHT: f64 = 132.0;
/// Gap above the taskbar, in logical pixels.
const MARGIN: f64 = 20.0;

pub const SHORTCUT: &str = "Ctrl+Shift+Space";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html#popup".into()))
        .title("Relay")
        .inner_size(WIDTH, HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        win::prepare(hwnd.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

/// Bottom centre of the work area of the monitor the main window is on.
fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(m) = monitor else {
        return;
    };
    let scale = m.scale_factor();
    let area = m.work_area();
    let w = (WIDTH * scale) as i32;
    let h = (HEIGHT * scale) as i32;
    let x = area.position.x + (area.size.width as i32 - w) / 2;
    let y = area.position.y + area.size.height as i32 - h - (MARGIN * scale) as i32;
    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Show the popup without taking focus from whatever the owner or Relay is using.
pub fn show(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        place(app, &window);
    }
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        win::show_inactive(hwnd.0 as isize);
        return;
    }
    let _ = window.show();
}

pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

/// The shortcut: open the popup and put the cursor in its box, or close it again.
pub fn toggle(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        release(app);
        hide(app);
    } else {
        show(app);
        grab(app);
    }
}

/// The owner clicked the text box: let the popup take the keyboard. The window that
/// had focus is remembered and gets it back in [`release`].
pub fn grab(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        win::grab(hwnd.0 as isize);
    }
    let _ = window.set_focus();
}

/// Typing is done: hand the keyboard back to the program Relay works in.
pub fn release(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else {
        return;
    };
    #[cfg(windows)]
    if let Ok(hwnd) = window.hwnd() {
        win::release(hwnd.0 as isize);
    }
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowLongPtrW, IsWindow, SetForegroundWindow,
        SetWindowDisplayAffinity, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_SHOWNOACTIVATE,
        WDA_EXCLUDEFROMCAPTURE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };

    /// The window that had focus before the owner clicked into the popup.
    static BEFORE: AtomicIsize = AtomicIsize::new(0);

    fn set_noactivate(hwnd: HWND, on: bool) {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let style = if on {
                style | WS_EX_NOACTIVATE as isize
            } else {
                style & !(WS_EX_NOACTIVATE as isize)
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
        }
    }

    pub fn prepare(hwnd: isize) {
        let hwnd = hwnd as HWND;
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
            );
            // Screenshots (and so the local operator) see what is behind the popup.
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
    }

    pub fn show_inactive(hwnd: isize) {
        unsafe { ShowWindow(hwnd as HWND, SW_SHOWNOACTIVATE) };
    }

    pub fn grab(hwnd: isize) {
        let hwnd = hwnd as HWND;
        let before = unsafe { GetForegroundWindow() };
        if before != hwnd {
            BEFORE.store(before as isize, Ordering::SeqCst);
        }
        set_noactivate(hwnd, false);
        unsafe { SetForegroundWindow(hwnd) };
    }

    pub fn release(hwnd: isize) {
        set_noactivate(hwnd as HWND, true);
        let before = BEFORE.swap(0, Ordering::SeqCst) as HWND;
        if !before.is_null() && unsafe { IsWindow(before) } != 0 {
            unsafe { SetForegroundWindow(before) };
        }
    }
}
