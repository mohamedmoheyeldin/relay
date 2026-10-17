//! macOS implementation of [`Desktop`] (Core Graphics capture and `CGEvent` input).
//!
//! Coordinates follow the crate contract: pixels, laid out as described in `mac_geom`. Pure logic
//! (key codes, geometry, pixel formats) lives in `mac_keys` and `mac_geom` so it is tested on
//! every OS; this file is only the system calls.
//!
//! Permissions: capture needs Screen Recording and input needs Accessibility. macOS grants both
//! per app in System Settings; without them capture fails and posted events are silently dropped,
//! so both are checked up front and reported with instructions.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::data::{CFData, CFDataRef};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::access::ScreenCaptureAccess;
use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
    ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_graphics::image::CGImage;
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly,
};
use foreign_types::ForeignType;

use crate::mac_geom::{to_rgba, Layout, PtRect, Screen};
use crate::mac_keys::{
    layout_table, mac_keycode, modifier_flag, wheel_lines, ClickCounter, SurrogateJoiner,
    FLAG_COMMAND, FLAG_CONTROL, RAW_MAC_KEY,
};
use crate::types::{
    CharKey, Desktop, DisplayInfo, Frame, InputEvent, MouseButton, OsError, Rect, Result,
    WindowInfo,
};

const SCREEN_RECORDING_MISSING: &str = "macOS has not given Relay the Screen Recording permission. \
Open System Settings > Privacy & Security > Screen Recording (\"Screen & System Audio Recording\" on newer \
macOS), turn Relay on, then quit and reopen Relay.";

const ACCESSIBILITY_MISSING: &str = "macOS has not given Relay the Accessibility permission, so it \
cannot move the mouse or type. Open System Settings > Privacy & Security > Accessibility, turn Relay on, \
then try again.";

/// How long `focus_window` waits for the activation script (it can wait on a consent prompt).
const FOCUS_TIMEOUT: Duration = Duration::from_secs(15);

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGImageGetBitmapInfo(image: *mut core_graphics::sys::CGImage) -> u32;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, key: CFStringRef) -> *mut c_void;
    fn LMGetKbdType() -> u8;
    #[allow(clippy::too_many_arguments)]
    fn UCKeyTranslate(
        layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
}

extern "C" {
    fn pthread_main_np() -> i32;
}

fn platform<E: std::fmt::Display>(what: &str, e: E) -> OsError {
    OsError::Platform(format!("{what}: {e}"))
}

fn on_main_thread() -> bool {
    // SAFETY: no preconditions.
    unsafe { pthread_main_np() != 0 }
}

/// Character -> (key code, shift, option) for the current keyboard layout.
type KeyTable = HashMap<char, (u16, bool, bool)>;

/// Read the current keyboard layout. The Text Input Sources API must run on the main thread
/// (recent macOS asserts this), so callers check [`on_main_thread`] first.
fn read_key_table() -> Option<KeyTable> {
    // SAFETY: TIS returns a +1 reference we wrap (and so release); the layout data is borrowed
    // from it and only used while `source` is alive; UCKeyTranslate gets valid out pointers.
    unsafe {
        let raw = TISCopyCurrentKeyboardLayoutInputSource();
        if raw.is_null() {
            return None;
        }
        let source = CFType::wrap_under_create_rule(raw as CFTypeRef);
        let data = TISGetInputSourceProperty(
            source.as_CFTypeRef() as *mut c_void,
            kTISPropertyUnicodeKeyLayoutData,
        );
        if data.is_null() {
            return None;
        }
        let data = CFData::wrap_under_get_rule(data as CFDataRef);
        let layout = data.bytes().as_ptr() as *const c_void;
        let keyboard = u32::from(LMGetKbdType());
        Some(layout_table(|code, shift, option| {
            // Carbon modifier state is (EventModifiers >> 8): shift 0x02, option 0x08.
            let mods = if shift { 0x02 } else { 0 } | if option { 0x08 } else { 0 };
            let mut dead = 0u32;
            let mut buf = [0u16; 4];
            let mut len = 0usize;
            let status = UCKeyTranslate(
                layout,
                code,
                0, // kUCKeyActionDown
                mods,
                keyboard,
                0, // dead keys stay dead, so they are skipped below
                &mut dead,
                buf.len(),
                &mut len,
                buf.as_mut_ptr(),
            );
            if status != 0 || dead != 0 || len == 0 {
                return None;
            }
            let s = String::from_utf16(&buf[..len.min(buf.len())]).ok()?;
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        }))
    }
}

#[derive(Default)]
struct InputState {
    /// Modifier flags currently held by Relay's own key presses.
    flags: u64,
    /// Mouse button currently held (moves become drags).
    held: Option<MouseButton>,
    clicks: ClickCounter,
    joiner: SurrogateJoiner,
}

/// The real macOS desktop.
pub struct MacDesktop {
    state: Mutex<InputState>,
    keys: Mutex<Option<KeyTable>>,
    asked_for_capture: AtomicBool,
}

impl MacDesktop {
    pub fn new() -> Self {
        // Relay creates the desktop during app setup on the main thread, the only place the
        // keyboard layout can be read.
        let keys = if on_main_thread() {
            read_key_table()
        } else {
            None
        };
        Self {
            state: Mutex::new(InputState::default()),
            keys: Mutex::new(keys),
            asked_for_capture: AtomicBool::new(false),
        }
    }

    fn layout(&self) -> Result<Layout> {
        let ids = CGDisplay::active_displays().map_err(|e| platform("listing displays", e))?;
        let mut screens = Vec::new();
        for id in ids {
            let d = CGDisplay::new(id);
            // A display that mirrors another shows the same pixels; list only the source.
            if d.mirrors_display() != 0 {
                continue;
            }
            let b = d.bounds();
            let (pixel_w, pixel_h) = match d.display_mode() {
                Some(mode) => (mode.pixel_width(), mode.pixel_height()),
                None => (d.pixels_wide(), d.pixels_high()),
            };
            screens.push(Screen {
                cg_id: id,
                points: PtRect {
                    x: b.origin.x,
                    y: b.origin.y,
                    w: b.size.width,
                    h: b.size.height,
                },
                pixel_w: u32::try_from(pixel_w).unwrap_or(u32::MAX),
                pixel_h: u32::try_from(pixel_h).unwrap_or(u32::MAX),
                primary: d.is_main(),
            });
        }
        Layout::new(screens)
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, InputState>> {
        self.state
            .lock()
            .map_err(|_| OsError::Platform("input state lock poisoned".into()))
    }

    fn scan_windows(&self) -> Result<Vec<(i64, WindowInfo)>> {
        let layout = self.layout()?;
        let list = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        )
        .ok_or_else(|| OsError::Platform("macOS did not return the window list".into()))?;
        let mut found = Vec::new();
        // The list runs front to back, so the first normal-layer window belongs to the active app.
        let mut first = true;
        for item in list.iter() {
            // SAFETY: every element of the window list is a CFDictionary owned by `list`.
            let d: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
            // Layer 0 holds ordinary app windows (menus, the Dock and overlays sit above).
            if dict_number(&d, "kCGWindowLayer") != Some(0.0) {
                continue;
            }
            let owner = dict_string(&d, "kCGWindowOwnerName").unwrap_or_default();
            // Window names are only visible with the Screen Recording permission.
            let name = dict_string(&d, "kCGWindowName").unwrap_or_default();
            let title = match (name.trim().is_empty(), owner.trim().is_empty()) {
                (true, true) => continue,
                (true, false) => owner.clone(),
                (false, true) => name,
                (false, false) => format!("{name} - {owner}"),
            };
            let Some(bounds) = dict_bounds(&d) else {
                continue;
            };
            let pid = dict_number(&d, "kCGWindowOwnerPID").unwrap_or(0.0) as i64;
            found.push((
                pid,
                WindowInfo {
                    title,
                    rect: layout.rect_to_pixels(bounds),
                    foreground: first,
                },
            ));
            first = false;
        }
        Ok(found)
    }
}

impl Default for MacDesktop {
    fn default() -> Self {
        Self::new()
    }
}

fn dict_value(d: &CFDictionary<CFString, CFType>, key: &'static str) -> Option<CFType> {
    d.find(CFString::from_static_string(key))
        .map(|v| (*v).clone())
}

fn dict_number(d: &CFDictionary<CFString, CFType>, key: &'static str) -> Option<f64> {
    dict_value(d, key)?.downcast::<CFNumber>()?.to_f64()
}

fn dict_string(d: &CFDictionary<CFString, CFType>, key: &'static str) -> Option<String> {
    Some(dict_value(d, key)?.downcast::<CFString>()?.to_string())
}

fn dict_bounds(d: &CFDictionary<CFString, CFType>) -> Option<PtRect> {
    let raw: CFDictionary = dict_value(d, "kCGWindowBounds")?.downcast::<CFDictionary>()?;
    // SAFETY: re-typing the same dictionary; it is retained by the get rule.
    let b: CFDictionary<CFString, CFType> =
        unsafe { CFDictionary::wrap_under_get_rule(raw.as_concrete_TypeRef()) };
    Some(PtRect {
        x: dict_number(&b, "X")?,
        y: dict_number(&b, "Y")?,
        w: dict_number(&b, "Width")?,
        h: dict_number(&b, "Height")?,
    })
}

fn ensure_accessibility() -> Result<()> {
    // SAFETY: no preconditions.
    if unsafe { AXIsProcessTrusted() } != 0 {
        Ok(())
    } else {
        Err(OsError::Platform(ACCESSIBILITY_MISSING.into()))
    }
}

fn event_source() -> Result<CGEventSource> {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| OsError::Platform("could not create an input event source".into()))
}

fn image_to_frame(image: &CGImage, want_w: u32, want_h: u32) -> Result<Frame> {
    let (w, h) = (image.width(), image.height());
    // SAFETY: `image` is a live CGImage.
    let info = unsafe { CGImageGetBitmapInfo(image.as_ptr()) };
    let data = image.data();
    let rgba = to_rgba(
        data.bytes(),
        w,
        h,
        image.bytes_per_row(),
        image.bits_per_pixel(),
        info,
    )?;
    let (w, h) = (
        u32::try_from(w).map_err(|e| platform("image width", e))?,
        u32::try_from(h).map_err(|e| platform("image height", e))?,
    );
    if (w, h) == (want_w, want_h) {
        return Ok(Frame {
            width: w,
            height: h,
            rgba,
        });
    }
    // Rounding at the point/pixel boundary can be off by a pixel; honour the requested size.
    let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(w, h, rgba)
        .ok_or_else(|| OsError::Platform("frame buffer size mismatch".into()))?;
    let img = image::imageops::resize(&img, want_w, want_h, image::imageops::FilterType::Triangle);
    Ok(Frame {
        width: want_w,
        height: want_h,
        rgba: img.into_raw(),
    })
}

/// Bring the app with process id `pid` to the front. The pid travels as a script argument
/// (`on run argv`), never as script text.
fn activate_pid(pid: i64) -> Result<()> {
    let mut child = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "tell application \"System Events\" to set frontmost of (first process whose unix id is ((item 1 of argv) as integer)) to true",
            "-e",
            "end run",
            &pid.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| platform("starting osascript", e))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|e| platform("osascript", e))? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                return Err(OsError::Platform(format!(
                    "macOS refused to switch apps ({}). If it mentions permission, allow Relay to \
                     control \"System Events\" in System Settings > Privacy & Security > Automation.",
                    err.trim()
                )));
            }
            None if started.elapsed() > FOCUS_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(OsError::Platform(
                    "switching apps timed out (a macOS permission prompt may be waiting)".into(),
                ));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

impl Desktop for MacDesktop {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(self.layout()?.displays())
    }

    fn virtual_desktop(&self) -> Result<Rect> {
        Ok(self.layout()?.virtual_rect())
    }

    fn capture(&self, rect: Rect) -> Result<Frame> {
        if !ScreenCaptureAccess.preflight() {
            // Ask once per launch so macOS lists Relay in the settings pane.
            if !self.asked_for_capture.swap(true, Ordering::SeqCst) {
                ScreenCaptureAccess.request();
            }
            return Err(OsError::Platform(SCREEN_RECORDING_MISSING.into()));
        }
        let (screen, local) = self.layout()?.capture_region(rect)?;
        let image = CGDisplay::new(screen.cg_id)
            .image_for_rect(CGRect::new(
                &CGPoint::new(local.x, local.y),
                &CGSize::new(local.w, local.h),
            ))
            .ok_or_else(|| {
                OsError::Platform(format!(
                    "macOS returned no screen image for display {}; {SCREEN_RECORDING_MISSING}",
                    screen.cg_id
                ))
            })?;
        image_to_frame(&image, rect.w, rect.h)
    }

    fn cursor_position(&self) -> Result<(i32, i32)> {
        let event = CGEvent::new(event_source()?)
            .map_err(|()| OsError::Platform("could not read the cursor position".into()))?;
        let p = event.location();
        Ok(self.layout()?.to_pixel(p.x, p.y))
    }

    fn send(&self, event: InputEvent) -> Result<()> {
        ensure_accessibility()?;
        let source = event_source()?;
        let mut st = self.lock_state()?;
        let posted = match event {
            InputEvent::MoveTo { x, y } => {
                let (px, py) = self.layout()?.to_point(x, y);
                let (kind, button) = match st.held {
                    Some(MouseButton::Left) => (CGEventType::LeftMouseDragged, CGMouseButton::Left),
                    Some(MouseButton::Right) => {
                        (CGEventType::RightMouseDragged, CGMouseButton::Right)
                    }
                    Some(MouseButton::Middle) => {
                        (CGEventType::OtherMouseDragged, CGMouseButton::Center)
                    }
                    None => (CGEventType::MouseMoved, CGMouseButton::Left),
                };
                CGEvent::new_mouse_event(source, kind, CGPoint::new(px, py), button)
            }
            InputEvent::Button { button, down } => {
                let at = CGEvent::new(source.clone())
                    .map_err(|()| OsError::Platform("could not read the cursor position".into()))?
                    .location();
                let (kind, cg_button, code) = match (button, down) {
                    (MouseButton::Left, true) => {
                        (CGEventType::LeftMouseDown, CGMouseButton::Left, 0)
                    }
                    (MouseButton::Left, false) => {
                        (CGEventType::LeftMouseUp, CGMouseButton::Left, 0)
                    }
                    (MouseButton::Right, true) => {
                        (CGEventType::RightMouseDown, CGMouseButton::Right, 1)
                    }
                    (MouseButton::Right, false) => {
                        (CGEventType::RightMouseUp, CGMouseButton::Right, 1)
                    }
                    (MouseButton::Middle, true) => {
                        (CGEventType::OtherMouseDown, CGMouseButton::Center, 2)
                    }
                    (MouseButton::Middle, false) => {
                        (CGEventType::OtherMouseUp, CGMouseButton::Center, 2)
                    }
                };
                let count = if down {
                    st.clicks.press(code, (at.x, at.y), Instant::now())
                } else {
                    st.clicks.current()
                };
                st.held = if down { Some(button) } else { None };
                CGEvent::new_mouse_event(source, kind, at, cg_button).inspect(|e| {
                    e.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, count);
                })
            }
            InputEvent::Wheel { horizontal, delta } => {
                let lines = wheel_lines(delta);
                // Windows: positive horizontal delta scrolls right; macOS wheel 2 is the
                // opposite sign.
                let (vertical, sideways) = if horizontal { (0, -lines) } else { (lines, 0) };
                CGEvent::new_scroll_event(source, ScrollEventUnit::LINE, 2, vertical, sideways, 0)
            }
            InputEvent::Vk { vk, extended, down } => {
                let code = mac_keycode(vk, extended).ok_or_else(|| {
                    OsError::InvalidArgument(format!(
                        "key code {vk:#x} has no equivalent on a Mac keyboard"
                    ))
                })?;
                let flag = modifier_flag(code);
                if down {
                    st.flags |= flag;
                } else {
                    st.flags &= !flag;
                }
                CGEvent::new_keyboard_event(source, code, down)
            }
            InputEvent::Unicode { unit, down } => {
                if st.flags & (FLAG_COMMAND | FLAG_CONTROL) != 0 {
                    // A shortcut needs the physical key, and the layout could not be read
                    // (resolve_char returned nothing). Guessing a key could fire a different
                    // shortcut, so refuse the press; the matching release has nothing to undo.
                    if !down {
                        return Ok(());
                    }
                    return Err(OsError::InvalidArgument(format!(
                        "cannot press {:?} with Command or Control: the keyboard layout could not be read",
                        char::from_u32(u32::from(unit)).unwrap_or('?')
                    )));
                }
                let Some(units) = st.joiner.push(unit, down) else {
                    return Ok(());
                };
                CGEvent::new_keyboard_event(source, 0, down).inspect(|e| {
                    e.set_string_from_utf16_unchecked(&units);
                })
            }
        };
        let event =
            posted.map_err(|()| OsError::Platform("could not create the input event".into()))?;
        event.set_flags(CGEventFlags::from_bits_truncate(st.flags));
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    /// Characters type as Unicode (layout independent) unless Command or Control is held: then
    /// the shortcut needs the physical key, looked up in the current keyboard layout.
    fn resolve_char(&self, ch: char) -> Option<CharKey> {
        let flags = self.state.lock().ok()?.flags;
        if flags & (FLAG_COMMAND | FLAG_CONTROL) == 0 {
            return None;
        }
        let mut keys = self.keys.lock().ok()?;
        if on_main_thread() {
            if let Some(fresh) = read_key_table() {
                *keys = Some(fresh);
            }
        }
        let (code, shift, option) = *keys.as_ref()?.get(&ch)?;
        Some(CharKey {
            vk: RAW_MAC_KEY | code,
            shift,
            ctrl: false,
            alt: option,
        })
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(self.scan_windows()?.into_iter().map(|(_, w)| w).collect())
    }

    fn focus_window(&self, needle: &str) -> Result<String> {
        let needle_lc = needle.to_lowercase();
        let (pid, info) = self
            .scan_windows()?
            .into_iter()
            .find(|(_, w)| w.title.to_lowercase().contains(&needle_lc))
            .ok_or_else(|| {
                OsError::InvalidArgument(format!("no window title contains {needle:?}"))
            })?;
        if pid <= 0 {
            return Err(OsError::Platform(format!(
                "macOS did not report which app owns {:?}",
                info.title
            )));
        }
        // This raises the owning app; with several windows of one app, macOS decides which is
        // in front.
        activate_pid(pid)?;
        Ok(info.title)
    }

    // UI automation (ui_find, ui_invoke, ui_set_value, ui_outline) and control_name_at keep the
    // trait defaults: the macOS Accessibility (AXUIElement) version is not written yet.
}

#[cfg(test)]
mod tests {
    use super::*;

    // These read the real desktop and need a Mac (Screen Recording for the capture test).

    #[test]
    fn lists_displays_with_pixel_sizes() {
        let d = MacDesktop::new();
        let displays = d.displays().unwrap();
        assert!(displays[0].primary);
        assert!(displays[0].rect.w > 0);
        let vd = d.virtual_desktop().unwrap();
        assert!(vd.w >= displays[0].rect.w);
    }

    #[test]
    fn enumerates_windows_without_error() {
        assert!(MacDesktop::new().windows().is_ok());
    }

    #[test]
    #[ignore = "needs the Screen Recording permission"]
    fn captures_a_small_region_with_opaque_pixels() {
        let d = MacDesktop::new();
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
    #[ignore = "moves the real mouse"]
    fn moves_the_real_cursor() {
        let d = MacDesktop::new();
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
