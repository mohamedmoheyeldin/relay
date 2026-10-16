//! In-memory [`Desktop`] used by tests in this and other crates.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::driver::StopFlag;
use crate::types::{
    CharKey, Desktop, DisplayInfo, Frame, InputEvent, OsError, Rect, Result, WindowInfo,
};

struct Inner {
    events: Vec<InputEvent>,
    chars: HashMap<char, CharKey>,
    trip: Option<(usize, StopFlag)>,
    cursor: (i32, i32),
    shade: u8,
    windows: Vec<WindowInfo>,
}

/// A fake desktop that records every input event and serves synthetic frames.
pub struct FakeDesktop {
    displays: Vec<DisplayInfo>,
    inner: Mutex<Inner>,
}

impl Default for FakeDesktop {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDesktop {
    /// One 1920x1080 primary display at the origin.
    pub fn new() -> Self {
        Self::with_displays(vec![DisplayInfo {
            id: 1,
            rect: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            primary: true,
        }])
    }

    pub fn with_displays(displays: Vec<DisplayInfo>) -> Self {
        Self {
            displays,
            inner: Mutex::new(Inner {
                events: Vec::new(),
                chars: HashMap::new(),
                trip: None,
                cursor: (0, 0),
                shade: 0,
                windows: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn events(&self) -> Vec<InputEvent> {
        self.lock().events.clone()
    }

    pub fn clear_events(&self) {
        self.lock().events.clear();
    }

    pub fn map_char(&self, ch: char, key: CharKey) {
        self.lock().chars.insert(ch, key);
    }

    /// Trip `stop` once `n` events have been recorded.
    pub fn trip_after(&self, n: usize, stop: StopFlag) {
        self.lock().trip = Some((n, stop));
    }

    /// Change the colour of subsequent frames (simulates the screen changing).
    pub fn set_shade(&self, shade: u8) {
        self.lock().shade = shade;
    }

    pub fn set_windows(&self, windows: Vec<WindowInfo>) {
        self.lock().windows = windows;
    }
}

impl Desktop for FakeDesktop {
    fn displays(&self) -> Result<Vec<DisplayInfo>> {
        Ok(self.displays.clone())
    }

    fn virtual_desktop(&self) -> Result<Rect> {
        let (mut l, mut t, mut r, mut b) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for d in &self.displays {
            l = l.min(i64::from(d.rect.x));
            t = t.min(i64::from(d.rect.y));
            r = r.max(i64::from(d.rect.x) + i64::from(d.rect.w));
            b = b.max(i64::from(d.rect.y) + i64::from(d.rect.h));
        }
        Ok(Rect {
            x: l as i32,
            y: t as i32,
            w: (r - l) as u32,
            h: (b - t) as u32,
        })
    }

    fn capture(&self, rect: Rect) -> Result<Frame> {
        let shade = self.lock().shade;
        let mut rgba = Vec::with_capacity(rect.w as usize * rect.h as usize * 4);
        for y in 0..rect.h {
            for x in 0..rect.w {
                rgba.extend_from_slice(&[shade, (x % 251) as u8, (y % 241) as u8, 255]);
            }
        }
        Ok(Frame {
            width: rect.w,
            height: rect.h,
            rgba,
        })
    }

    fn cursor_position(&self) -> Result<(i32, i32)> {
        Ok(self.lock().cursor)
    }

    fn send(&self, event: InputEvent) -> Result<()> {
        let mut inner = self.lock();
        if let InputEvent::MoveTo { x, y } = event {
            inner.cursor = (x, y);
        }
        inner.events.push(event);
        if let Some((n, stop)) = &inner.trip {
            if inner.events.len() >= *n {
                stop.trip();
            }
        }
        Ok(())
    }

    fn resolve_char(&self, ch: char) -> Option<CharKey> {
        self.lock().chars.get(&ch).copied()
    }

    fn windows(&self) -> Result<Vec<WindowInfo>> {
        Ok(self.lock().windows.clone())
    }

    fn focus_window(&self, needle: &str) -> Result<String> {
        let mut inner = self.lock();
        let needle = needle.to_lowercase();
        let found = inner
            .windows
            .iter()
            .position(|w| w.title.to_lowercase().contains(&needle));
        match found {
            Some(i) => {
                for (j, w) in inner.windows.iter_mut().enumerate() {
                    w.foreground = i == j;
                }
                Ok(inner.windows[i].title.clone())
            }
            None => Err(OsError::InvalidArgument(format!(
                "no window title contains {needle:?}"
            ))),
        }
    }
}
