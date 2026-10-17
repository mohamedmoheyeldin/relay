//! Operating-system layer for Relay: screen capture and input behind the [`Desktop`] trait.
//!
//! Platform-neutral logic (coordinate scaling, key parsing, the input [`driver`]) is pure and
//! tested everywhere. The Windows implementation lives in `win`, the macOS one in `mac`; the
//! right one is selected by [`system_desktop`].

pub mod capture;
pub mod driver;
pub mod keys;
#[cfg(target_os = "macos")]
mod mac;
pub mod scaling;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod types;
pub mod ui;
// Pure macOS helpers: compiled and tested on every OS, used only by `mac` on macOS.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod mac_geom;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod mac_keys;
#[cfg(windows)]
mod win;
#[cfg(windows)]
mod win_uia;

use std::sync::Arc;

pub use types::{
    CharKey, Desktop, DisplayInfo, Frame, InputEvent, MouseButton, OsError, Rect, Result,
    UiElement, UiSelector, WindowInfo,
};

/// The desktop implementation for the current platform.
#[cfg(windows)]
pub fn system_desktop() -> Result<Arc<dyn Desktop>> {
    Ok(Arc::new(win::WindowsDesktop::new()))
}

/// The desktop implementation for the current platform.
#[cfg(target_os = "macos")]
pub fn system_desktop() -> Result<Arc<dyn Desktop>> {
    Ok(Arc::new(mac::MacDesktop::new()))
}

/// The desktop implementation for the current platform.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn system_desktop() -> Result<Arc<dyn Desktop>> {
    Err(OsError::Unsupported(
        "only Windows and macOS are implemented; Linux can be added behind the Desktop trait",
    ))
}

/// Total and free bytes on the drive that holds `path`, if the OS can tell.
#[cfg(windows)]
pub fn disk_space(path: &std::path::Path) -> Option<(u64, u64)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` is NUL-terminated and outlives the call; the out pointers are valid u64s.
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut avail),
            Some(&mut total),
            Some(&mut free),
        )
    }
    .ok()?;
    Some((total, avail))
}

/// Total and free bytes on the drive that holds `path`, if the OS can tell.
#[cfg(not(windows))]
pub fn disk_space(_path: &std::path::Path) -> Option<(u64, u64)> {
    None
}
