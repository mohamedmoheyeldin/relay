//! Operating-system layer for Relay: screen capture and input behind the [`Desktop`] trait.
//!
//! Platform-neutral logic (coordinate scaling, key parsing, the input [`driver`]) is pure and
//! tested everywhere. The Windows implementation lives in `win` and is selected by
//! [`system_desktop`].

pub mod capture;
pub mod driver;
pub mod keys;
pub mod scaling;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod types;
pub mod ui;
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
#[cfg(not(windows))]
pub fn system_desktop() -> Result<Arc<dyn Desktop>> {
    Err(OsError::Unsupported(
        "only Windows is implemented; macOS and Linux can be added behind the Desktop trait",
    ))
}
