//! High-level input (click, drag, scroll, type, key chords) built on [`Desktop::send`].
//!
//! The driver honours a [`StopFlag`]: once tripped, nothing new is pressed or moved, but
//! every release still goes out so no key or button is left held.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::keys::{Chord, Key, Modifier, Named};
use crate::types::{Desktop, InputEvent, MouseButton, OsError, Result};

/// Shared kill switch. Cloning shares the same flag.
#[derive(Debug, Clone, Default)]
pub struct StopFlag(Arc<AtomicBool>);

impl StopFlag {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn trip(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
    pub fn is_tripped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

impl ScrollDirection {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            other => Err(OsError::InvalidArgument(format!(
                "scroll direction {other:?} must be up, down, left or right"
            ))),
        }
    }
}

/// Timing knobs; tests use [`Timing::instant`].
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub char_delay: Duration,
    pub click_gap: Duration,
    pub drag_step: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            char_delay: Duration::from_millis(8),
            click_gap: Duration::from_millis(40),
            drag_step: Duration::from_millis(6),
        }
    }
}

impl Timing {
    pub fn instant() -> Self {
        Self {
            char_delay: Duration::ZERO,
            click_gap: Duration::ZERO,
            drag_step: Duration::ZERO,
        }
    }
}

const DRAG_STEPS: i32 = 12;
const WHEEL_NOTCH: i32 = 120;
pub const MAX_HOLD: Duration = Duration::from_secs(300);

pub struct Driver {
    desktop: Arc<dyn Desktop>,
    stop: StopFlag,
    timing: Timing,
}

impl Driver {
    pub fn new(desktop: Arc<dyn Desktop>, stop: StopFlag) -> Self {
        Self::with_timing(desktop, stop, Timing::default())
    }

    pub fn with_timing(desktop: Arc<dyn Desktop>, stop: StopFlag, timing: Timing) -> Self {
        Self {
            desktop,
            stop,
            timing,
        }
    }

    pub fn stop_flag(&self) -> &StopFlag {
        &self.stop
    }

    fn emit(&self, event: InputEvent) -> Result<()> {
        if self.stop.is_tripped() && !event.is_release() {
            return Err(OsError::Stopped);
        }
        self.desktop.send(event)
    }

    /// Sleep in short slices so a stop wakes us promptly.
    fn pause(&self, total: Duration) -> Result<()> {
        let end = Instant::now() + total;
        loop {
            if self.stop.is_tripped() {
                return Err(OsError::Stopped);
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            std::thread::sleep(left.min(Duration::from_millis(20)));
        }
    }

    /// Wait for `total`, returning early with `Stopped` if the kill switch trips.
    pub fn wait(&self, total: Duration) -> Result<()> {
        self.pause(total)
    }

    pub fn move_to(&self, x: i32, y: i32) -> Result<()> {
        self.emit(InputEvent::MoveTo { x, y })
    }

    /// Run `body` with `mods` held, always releasing them afterwards.
    fn with_modifiers<T>(&self, mods: &[Modifier], body: impl FnOnce() -> Result<T>) -> Result<T> {
        let mut pressed = Vec::new();
        let mut result = Ok(());
        for m in mods {
            match self.emit(InputEvent::Vk {
                vk: m.vk(),
                extended: m.extended(),
                down: true,
            }) {
                Ok(()) => pressed.push(*m),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        let out = match result {
            Ok(()) => body(),
            Err(e) => Err(e),
        };
        for m in pressed.iter().rev() {
            let _ = self.emit(InputEvent::Vk {
                vk: m.vk(),
                extended: m.extended(),
                down: false,
            });
        }
        out
    }

    pub fn mouse_down(&self, button: MouseButton) -> Result<()> {
        self.emit(InputEvent::Button { button, down: true })
    }

    pub fn mouse_up(&self, button: MouseButton) -> Result<()> {
        self.emit(InputEvent::Button {
            button,
            down: false,
        })
    }

    /// Click `count` times (1..=3) at a physical point.
    pub fn click(
        &self,
        at: Option<(i32, i32)>,
        button: MouseButton,
        count: u8,
        mods: &[Modifier],
    ) -> Result<()> {
        if !(1..=3).contains(&count) {
            return Err(OsError::InvalidArgument(format!(
                "click count {count} must be 1..=3"
            )));
        }
        if let Some((x, y)) = at {
            self.move_to(x, y)?;
        }
        self.with_modifiers(mods, || {
            for i in 0..count {
                if i > 0 {
                    self.pause(self.timing.click_gap)?;
                }
                self.mouse_down(button)?;
                self.mouse_up(button)?;
            }
            Ok(())
        })
    }

    /// Press at `from`, move in steps to `to`, release. The button is always released.
    pub fn drag(&self, from: (i32, i32), to: (i32, i32), mods: &[Modifier]) -> Result<()> {
        self.move_to(from.0, from.1)?;
        self.with_modifiers(mods, || {
            self.mouse_down(MouseButton::Left)?;
            let moved = (|| {
                for i in 1..=DRAG_STEPS {
                    let x = from.0 + (to.0 - from.0) * i / DRAG_STEPS;
                    let y = from.1 + (to.1 - from.1) * i / DRAG_STEPS;
                    self.move_to(x, y)?;
                    self.pause(self.timing.drag_step)?;
                }
                Ok(())
            })();
            let _ = self.mouse_up(MouseButton::Left);
            moved
        })
    }

    pub fn scroll(
        &self,
        at: Option<(i32, i32)>,
        direction: ScrollDirection,
        amount: u32,
        mods: &[Modifier],
    ) -> Result<()> {
        if !(1..=100).contains(&amount) {
            return Err(OsError::InvalidArgument(format!(
                "scroll amount {amount} must be 1..=100"
            )));
        }
        if let Some((x, y)) = at {
            self.move_to(x, y)?;
        }
        let (horizontal, sign) = match direction {
            ScrollDirection::Up => (false, 1),
            ScrollDirection::Down => (false, -1),
            ScrollDirection::Right => (true, 1),
            ScrollDirection::Left => (true, -1),
        };
        self.with_modifiers(mods, || {
            for _ in 0..amount {
                self.emit(InputEvent::Wheel {
                    horizontal,
                    delta: sign * WHEEL_NOTCH,
                })?;
            }
            Ok(())
        })
    }

    fn tap_vk(&self, vk: u16, extended: bool) -> Result<()> {
        self.emit(InputEvent::Vk {
            vk,
            extended,
            down: true,
        })?;
        self.emit(InputEvent::Vk {
            vk,
            extended,
            down: false,
        })
    }

    fn tap_unicode(&self, ch: char) -> Result<()> {
        let mut buf = [0u16; 2];
        for unit in ch.encode_utf16(&mut buf) {
            self.emit(InputEvent::Unicode {
                unit: *unit,
                down: true,
            })?;
            self.emit(InputEvent::Unicode {
                unit: *unit,
                down: false,
            })?;
        }
        Ok(())
    }

    /// Press and release one key, resolving characters through the keyboard layout.
    fn tap_key(&self, key: Key, held: &[Modifier]) -> Result<()> {
        match key {
            Key::Named(n) => self.tap_vk(n.vk(), n.extended()),
            Key::Char(ch) => match self.desktop.resolve_char(ch) {
                Some(ck) => {
                    let mut extra = Vec::new();
                    for (needed, m) in [
                        (ck.shift, Modifier::Shift),
                        (ck.ctrl, Modifier::Ctrl),
                        (ck.alt, Modifier::Alt),
                    ] {
                        if needed && !held.contains(&m) {
                            extra.push(m);
                        }
                    }
                    self.with_modifiers(&extra, || self.tap_vk(ck.vk, false))
                }
                None => self.tap_unicode(ch),
            },
        }
    }

    /// Type text one character at a time. Newline becomes Return, tab becomes Tab.
    pub fn type_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Err(OsError::InvalidArgument("text must not be empty".into()));
        }
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        continue;
                    }
                    self.tap_key(Key::Named(Named::Return), &[])?;
                }
                '\n' => self.tap_key(Key::Named(Named::Return), &[])?,
                '\t' => self.tap_key(Key::Named(Named::Tab), &[])?,
                other => self.tap_key(Key::Char(other), &[])?,
            }
            if !self.timing.char_delay.is_zero() {
                self.pause(self.timing.char_delay)?;
            }
        }
        Ok(())
    }

    /// Press a chord `repeat` times (1..=100).
    pub fn key(&self, chord: &Chord, repeat: u32) -> Result<()> {
        if !(1..=100).contains(&repeat) {
            return Err(OsError::InvalidArgument(format!(
                "repeat {repeat} must be 1..=100"
            )));
        }
        for _ in 0..repeat {
            self.with_modifiers(&chord.modifiers, || match chord.key {
                Some(key) => self.tap_key(key, &chord.modifiers),
                None => Ok(()),
            })?;
        }
        Ok(())
    }

    /// Hold a chord for `duration` (0 < d <= 300 s), then release.
    pub fn hold_key(&self, chord: &Chord, duration: Duration) -> Result<()> {
        if duration.is_zero() || duration > MAX_HOLD {
            return Err(OsError::InvalidArgument(
                "duration must be greater than 0 and at most 300 seconds".into(),
            ));
        }
        self.with_modifiers(&chord.modifiers, || match chord.key {
            Some(Key::Named(n)) => {
                self.emit(InputEvent::Vk {
                    vk: n.vk(),
                    extended: n.extended(),
                    down: true,
                })?;
                let waited = self.pause(duration);
                let _ = self.emit(InputEvent::Vk {
                    vk: n.vk(),
                    extended: n.extended(),
                    down: false,
                });
                waited
            }
            Some(Key::Char(ch)) => {
                let vk = self
                    .desktop
                    .resolve_char(ch)
                    .map(|ck| ck.vk)
                    .ok_or_else(|| {
                        OsError::InvalidArgument(format!("cannot hold {ch:?}: no layout key"))
                    })?;
                self.emit(InputEvent::Vk {
                    vk,
                    extended: false,
                    down: true,
                })?;
                let waited = self.pause(duration);
                let _ = self.emit(InputEvent::Vk {
                    vk,
                    extended: false,
                    down: false,
                });
                waited
            }
            None => self.pause(duration),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::parse_chord;
    use crate::testing::FakeDesktop;
    use crate::types::CharKey;

    fn rig() -> (Arc<FakeDesktop>, Driver) {
        let fake = Arc::new(FakeDesktop::new());
        let driver = Driver::with_timing(fake.clone(), StopFlag::new(), Timing::instant());
        (fake, driver)
    }

    fn down(vk: u16) -> InputEvent {
        InputEvent::Vk {
            vk,
            extended: false,
            down: true,
        }
    }
    fn up(vk: u16) -> InputEvent {
        InputEvent::Vk {
            vk,
            extended: false,
            down: false,
        }
    }

    #[test]
    fn click_moves_then_presses_and_releases() {
        let (fake, d) = rig();
        d.click(Some((5, 6)), MouseButton::Right, 1, &[]).unwrap();
        assert_eq!(
            fake.events(),
            vec![
                InputEvent::MoveTo { x: 5, y: 6 },
                InputEvent::Button {
                    button: MouseButton::Right,
                    down: true
                },
                InputEvent::Button {
                    button: MouseButton::Right,
                    down: false
                },
            ]
        );
    }

    #[test]
    fn double_click_with_modifier_wraps_presses() {
        let (fake, d) = rig();
        d.click(None, MouseButton::Left, 2, &[Modifier::Ctrl])
            .unwrap();
        let ev = fake.events();
        assert_eq!(ev.first(), Some(&down(0x11)));
        assert_eq!(ev.last(), Some(&up(0x11)));
        let presses = ev
            .iter()
            .filter(|e| matches!(e, InputEvent::Button { down: true, .. }))
            .count();
        assert_eq!(presses, 2);
    }

    #[test]
    fn click_count_is_validated() {
        let (fake, d) = rig();
        assert!(d.click(None, MouseButton::Left, 0, &[]).is_err());
        assert!(d.click(None, MouseButton::Left, 4, &[]).is_err());
        assert!(fake.events().is_empty());
    }

    #[test]
    fn scroll_down_is_negative_and_horizontal_right_is_positive() {
        let (fake, d) = rig();
        d.scroll(Some((1, 2)), ScrollDirection::Down, 3, &[])
            .unwrap();
        d.scroll(None, ScrollDirection::Right, 1, &[]).unwrap();
        let wheels: Vec<_> = fake
            .events()
            .into_iter()
            .filter_map(|e| match e {
                InputEvent::Wheel { horizontal, delta } => Some((horizontal, delta)),
                _ => None,
            })
            .collect();
        assert_eq!(
            wheels,
            vec![(false, -120), (false, -120), (false, -120), (true, 120)]
        );
        assert!(d.scroll(None, ScrollDirection::Up, 0, &[]).is_err());
        assert!(d.scroll(None, ScrollDirection::Up, 101, &[]).is_err());
        assert!(ScrollDirection::parse("sideways").is_err());
    }

    #[test]
    fn drag_releases_button_even_when_stopped_midway() {
        let fake = Arc::new(FakeDesktop::new());
        let stop = StopFlag::new();
        fake.trip_after(4, stop.clone());
        let d = Driver::with_timing(fake.clone(), stop, Timing::instant());
        let result = d.drag((0, 0), (100, 100), &[]);
        assert!(matches!(result, Err(OsError::Stopped)));
        let ev = fake.events();
        assert!(matches!(
            ev.last(),
            Some(InputEvent::Button {
                button: MouseButton::Left,
                down: false
            })
        ));
    }

    #[test]
    fn drag_steps_end_at_target() {
        let (fake, d) = rig();
        d.drag((10, 10), (110, 60), &[]).unwrap();
        let moves: Vec<_> = fake
            .events()
            .into_iter()
            .filter_map(|e| match e {
                InputEvent::MoveTo { x, y } => Some((x, y)),
                _ => None,
            })
            .collect();
        assert_eq!(moves.first(), Some(&(10, 10)));
        assert_eq!(moves.last(), Some(&(110, 60)));
        assert!(moves.len() > 5);
    }

    #[test]
    fn typing_uses_layout_keys_and_adds_shift() {
        let (fake, d) = rig();
        fake.map_char(
            'a',
            CharKey {
                vk: 0x41,
                shift: false,
                ctrl: false,
                alt: false,
            },
        );
        fake.map_char(
            '?',
            CharKey {
                vk: 0xBF,
                shift: true,
                ctrl: false,
                alt: false,
            },
        );
        d.type_text("a?").unwrap();
        assert_eq!(
            fake.events(),
            vec![
                down(0x41),
                up(0x41),
                down(0x10),
                down(0xBF),
                up(0xBF),
                up(0x10)
            ]
        );
    }

    #[test]
    fn typing_falls_back_to_unicode_including_surrogate_pairs() {
        let (fake, d) = rig();
        d.type_text("é😀").unwrap();
        let units: Vec<_> = fake
            .events()
            .into_iter()
            .filter_map(|e| match e {
                InputEvent::Unicode { unit, down: true } => Some(unit),
                _ => None,
            })
            .collect();
        assert_eq!(units, vec![0x00E9, 0xD83D, 0xDE00]);
    }

    #[test]
    fn typing_maps_newline_tab_and_crlf() {
        let (fake, d) = rig();
        d.type_text("\n\t\r\n").unwrap();
        let downs: Vec<_> = fake
            .events()
            .into_iter()
            .filter_map(|e| match e {
                InputEvent::Vk { vk, down: true, .. } => Some(vk),
                _ => None,
            })
            .collect();
        assert_eq!(downs, vec![0x0D, 0x09, 0x0D]);
        assert!(d.type_text("").is_err());
    }

    #[test]
    fn key_chord_presses_modifiers_around_the_key() {
        let (fake, d) = rig();
        d.key(&parse_chord("ctrl+shift+Tab").unwrap(), 2).unwrap();
        let ev = fake.events();
        assert_eq!(ev.len(), 12);
        assert_eq!(ev[0], down(0x11));
        assert_eq!(ev[1], down(0x10));
        assert_eq!(ev[2], down(0x09));
        assert_eq!(ev[3], up(0x09));
        assert_eq!(ev[4], up(0x10));
        assert_eq!(ev[5], up(0x11));
        assert!(d.key(&parse_chord("a").unwrap(), 0).is_err());
    }

    #[test]
    fn extended_keys_carry_the_flag() {
        let (fake, d) = rig();
        d.key(&parse_chord("Delete").unwrap(), 1).unwrap();
        assert_eq!(
            fake.events()[0],
            InputEvent::Vk {
                vk: 0x2E,
                extended: true,
                down: true
            }
        );
    }

    #[test]
    fn super_alone_is_pressed_and_released() {
        let (fake, d) = rig();
        d.key(&parse_chord("super").unwrap(), 1).unwrap();
        assert_eq!(
            fake.events(),
            vec![
                InputEvent::Vk {
                    vk: 0x5B,
                    extended: true,
                    down: true
                },
                InputEvent::Vk {
                    vk: 0x5B,
                    extended: true,
                    down: false
                }
            ]
        );
    }

    #[test]
    fn hold_key_releases_and_validates_bounds() {
        let (fake, d) = rig();
        d.hold_key(&parse_chord("shift").unwrap(), Duration::from_millis(5))
            .unwrap();
        assert_eq!(fake.events(), vec![down(0x10), up(0x10)]);
        assert!(d
            .hold_key(&parse_chord("shift").unwrap(), Duration::ZERO)
            .is_err());
        assert!(d
            .hold_key(&parse_chord("shift").unwrap(), Duration::from_secs(301))
            .is_err());
    }

    #[test]
    fn hold_key_wakes_and_releases_when_stopped() {
        let fake = Arc::new(FakeDesktop::new());
        let stop = StopFlag::new();
        let d = Driver::with_timing(fake.clone(), stop.clone(), Timing::instant());
        let tripper = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(60));
                stop.trip();
            })
        };
        let started = Instant::now();
        let result = d.hold_key(&parse_chord("Space").unwrap(), Duration::from_secs(30));
        tripper.join().unwrap();
        assert!(matches!(result, Err(OsError::Stopped)));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(fake.events().last(), Some(&up(0x20)));
    }

    #[test]
    fn stopped_driver_refuses_presses_but_allows_releases() {
        let (fake, d) = rig();
        d.stop_flag().trip();
        assert!(matches!(
            d.mouse_down(MouseButton::Left),
            Err(OsError::Stopped)
        ));
        assert!(matches!(d.move_to(1, 1), Err(OsError::Stopped)));
        assert!(d.mouse_up(MouseButton::Left).is_ok());
        assert_eq!(fake.events().len(), 1);
        d.stop_flag().reset();
        assert!(d.move_to(1, 1).is_ok());
    }

    #[test]
    fn stop_mid_typing_halts_within_one_character() {
        let fake = Arc::new(FakeDesktop::new());
        let stop = StopFlag::new();
        fake.trip_after(6, stop.clone());
        let d = Driver::with_timing(fake.clone(), stop, Timing::instant());
        let result = d.type_text("abcdefghij");
        assert!(matches!(result, Err(OsError::Stopped)));
        assert!(fake.events().len() < 20);
    }
}
