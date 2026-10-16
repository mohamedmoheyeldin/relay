//! Platform-neutral types and the [`Desktop`] trait.

use std::fmt;

/// Errors from the OS layer.
#[derive(Debug, thiserror::Error)]
pub enum OsError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("out of bounds: {0}")]
    OutOfBounds(String),
    #[error("not supported on this platform: {0}")]
    Unsupported(&'static str),
    #[error("stopped by the owner")]
    Stopped,
    #[error("platform error: {0}")]
    Platform(String),
}

pub type Result<T> = std::result::Result<T, OsError>;

/// Axis-aligned rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    /// Whether the point lies inside the rectangle.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let (px, py) = (i64::from(px), i64::from(py));
        px >= i64::from(self.x)
            && py >= i64::from(self.y)
            && px < i64::from(self.x) + i64::from(self.w)
            && py < i64::from(self.y) + i64::from(self.h)
    }
}

/// A physical display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    pub id: u32,
    pub rect: Rect,
    pub primary: bool,
}

/// A captured frame in RGBA, row-major, physical resolution.
#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frame({}x{})", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// A top-level window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub title: String,
    pub rect: Rect,
    pub foreground: bool,
}

/// A character resolved through the active keyboard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CharKey {
    pub vk: u16,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

/// One low-level input event. Coordinates are physical virtual-desktop pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    MoveTo {
        x: i32,
        y: i32,
    },
    Button {
        button: MouseButton,
        down: bool,
    },
    /// Wheel notches in 1/120 units; positive is up/right.
    Wheel {
        horizontal: bool,
        delta: i32,
    },
    Vk {
        vk: u16,
        extended: bool,
        down: bool,
    },
    Unicode {
        unit: u16,
        down: bool,
    },
}

impl InputEvent {
    /// Releases must always go through, even after a stop, so nothing stays held.
    pub fn is_release(&self) -> bool {
        matches!(
            self,
            InputEvent::Button { down: false, .. }
                | InputEvent::Vk { down: false, .. }
                | InputEvent::Unicode { down: false, .. }
        )
    }
}

/// The operating-system surface Relay needs. Implementations must be cheap to share.
pub trait Desktop: Send + Sync {
    fn displays(&self) -> Result<Vec<DisplayInfo>>;
    /// Bounding rectangle of all displays.
    fn virtual_desktop(&self) -> Result<Rect>;
    /// Capture `rect` (physical virtual-desktop pixels) at full resolution.
    fn capture(&self, rect: Rect) -> Result<Frame>;
    fn cursor_position(&self) -> Result<(i32, i32)>;
    fn send(&self, event: InputEvent) -> Result<()>;
    /// Resolve a character to a layout key, or `None` to type it as Unicode.
    fn resolve_char(&self, _ch: char) -> Option<CharKey> {
        None
    }
    fn windows(&self) -> Result<Vec<WindowInfo>>;
    /// Bring the first window whose title contains `needle` (case-insensitive) to the front.
    fn focus_window(&self, needle: &str) -> Result<String>;
}
