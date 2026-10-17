//! Pure key and input helpers for the macOS desktop (testable on any OS).
//!
//! The input [`Driver`](crate::driver::Driver) speaks Windows virtual-key codes (see `keys.rs`),
//! so the macOS layer translates them to macOS virtual key codes (`kVK_*`, Carbon `Events.h`).
//! A [`CharKey`](crate::types::CharKey) resolved on macOS carries a macOS key code directly,
//! marked with [`RAW_MAC_KEY`], because only the current keyboard layout knows which physical key
//! types a character.

use std::time::{Duration, Instant};

/// Marks a `vk` that already is a macOS key code (low byte). Windows virtual keys are below 0x100.
pub const RAW_MAC_KEY: u16 = 0x8000;

// CGEventFlags modifier bits.
pub const FLAG_SHIFT: u64 = 0x0002_0000;
pub const FLAG_CONTROL: u64 = 0x0004_0000;
pub const FLAG_OPTION: u64 = 0x0008_0000;
pub const FLAG_COMMAND: u64 = 0x0010_0000;

// macOS key codes used below (kVK_*).
pub const KC_RETURN: u16 = 0x24;
pub const KC_COMMAND: u16 = 0x37;
pub const KC_SHIFT: u16 = 0x38;
pub const KC_OPTION: u16 = 0x3A;
pub const KC_CONTROL: u16 = 0x3B;
pub const KC_RIGHT_COMMAND: u16 = 0x36;
pub const KC_RIGHT_SHIFT: u16 = 0x3C;
pub const KC_RIGHT_OPTION: u16 = 0x3D;
pub const KC_RIGHT_CONTROL: u16 = 0x3E;

/// macOS function-key codes for F1..F20 (there is no F21..F24 on macOS).
const F_KEYS: [u16; 20] = [
    0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F, 0x69, 0x6B, 0x71, 0x6A,
    0x40, 0x4F, 0x50, 0x5A,
];

/// macOS keypad digit codes for 0..9.
const KEYPAD: [u16; 10] = [0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5B, 0x5C];

/// macOS ANSI key codes for the letters A..Z.
const LETTERS: [u16; 26] = [
    0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23,
    0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
];

/// macOS ANSI key codes for the digits 0..9 on the main row.
const DIGITS: [u16; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];

/// macOS key code for a Windows virtual key, or `None` when macOS has no such key.
///
/// `extended` distinguishes the keypad Enter (extended Return) from the main Return. Letter,
/// digit and punctuation keys map by their US ANSI position.
pub fn mac_keycode(vk: u16, extended: bool) -> Option<u16> {
    if vk & RAW_MAC_KEY != 0 {
        return Some(vk & 0xFF);
    }
    let code = match vk {
        0x08 => 0x33,              // Backspace -> Delete
        0x09 => 0x30,              // Tab
        0x0D if extended => 0x4C,  // Keypad Enter
        0x0D => KC_RETURN,         // Return
        0x10 | 0xA0 => KC_SHIFT,   // Shift
        0xA1 => KC_RIGHT_SHIFT,    //
        0x11 | 0xA2 => KC_CONTROL, // Ctrl -> Control
        0xA3 => KC_RIGHT_CONTROL,  //
        0x12 | 0xA4 => KC_OPTION,  // Alt -> Option
        0xA5 => KC_RIGHT_OPTION,   //
        0x5B => KC_COMMAND,        // Win/Super -> Command
        0x5C => KC_RIGHT_COMMAND,  //
        0x14 => 0x39,              // Caps Lock
        0x1B => 0x35,              // Escape
        0x20 => 0x31,              // Space
        0x21 => 0x74,              // Page Up
        0x22 => 0x79,              // Page Down
        0x23 => 0x77,              // End
        0x24 => 0x73,              // Home
        0x25 => 0x7B,              // Left
        0x26 => 0x7E,              // Up
        0x27 => 0x7C,              // Right
        0x28 => 0x7D,              // Down
        0x2D => 0x72,              // Insert -> Help (same physical key on Apple keyboards)
        0x2E => 0x75,              // Delete -> Forward Delete
        0x30..=0x39 => DIGITS[usize::from(vk - 0x30)],
        0x41..=0x5A => LETTERS[usize::from(vk - 0x41)],
        0x60..=0x69 => KEYPAD[usize::from(vk - 0x60)],
        0x6A => 0x43, // Keypad *
        0x6B => 0x45, // Keypad +
        0x6D => 0x4E, // Keypad -
        0x6E => 0x41, // Keypad .
        0x6F => 0x4B, // Keypad /
        0x70..=0x83 => F_KEYS[usize::from(vk - 0x70)],
        0x90 => 0x47, // Num Lock -> Keypad Clear
        0xBA => 0x29, // ;
        0xBB => 0x18, // =
        0xBC => 0x2B, // ,
        0xBD => 0x1B, // -
        0xBE => 0x2F, // .
        0xBF => 0x2C, // /
        0xC0 => 0x32, // `
        0xDB => 0x21, // [
        0xDC => 0x2A, // backslash
        0xDD => 0x1E, // ]
        0xDE => 0x27, // '
        // Scroll Lock, Menu, Print Screen, Pause, F21..F24 and the media/volume keys have no
        // keyboard-event equivalent on macOS.
        _ => return None,
    };
    Some(code)
}

/// The CGEventFlags bit a modifier key code holds down, or 0 for other keys.
pub fn modifier_flag(keycode: u16) -> u64 {
    match keycode {
        KC_SHIFT | KC_RIGHT_SHIFT => FLAG_SHIFT,
        KC_CONTROL | KC_RIGHT_CONTROL => FLAG_CONTROL,
        KC_OPTION | KC_RIGHT_OPTION => FLAG_OPTION,
        KC_COMMAND | KC_RIGHT_COMMAND => FLAG_COMMAND,
        _ => 0,
    }
}

/// Scroll lines for a wheel delta in 1/120 notches. One notch scrolls three lines, matching the
/// Windows default.
pub fn wheel_lines(delta: i32) -> i32 {
    let lines = (i64::from(delta) * 3) / 120;
    if lines == 0 && delta != 0 {
        delta.signum()
    } else {
        lines.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

/// Joins UTF-16 surrogate halves, which the driver sends as separate events, into one string
/// per key press so characters outside the BMP (emoji) type correctly.
#[derive(Debug, Default)]
pub struct SurrogateJoiner {
    high: Option<u16>,
}

impl SurrogateJoiner {
    /// Units to type for this event, or `None` when the event only completes later.
    pub fn push(&mut self, unit: u16, down: bool) -> Option<Vec<u16>> {
        let is_high = (0xD800..0xDC00).contains(&unit);
        let is_low = (0xDC00..0xE000).contains(&unit);
        match (down, is_high, is_low) {
            (true, true, _) => {
                self.high = Some(unit);
                None
            }
            // The release of a high half waits for the low half.
            (false, true, _) => None,
            (true, _, true) => self.high.take().map(|h| vec![h, unit]),
            (false, _, true) => Some(self.high.take().map_or(vec![unit], |h| vec![h, unit])),
            _ => {
                if down {
                    self.high = None;
                }
                Some(vec![unit])
            }
        }
    }
}

/// Counts consecutive clicks so a fast second press reaches apps as a double click (macOS reads
/// the click count from the event, not from timing).
#[derive(Debug, Default)]
pub struct ClickCounter {
    last: Option<(u8, (f64, f64), Instant, i64)>,
}

/// Longest gap between presses that still counts as a multi-click.
pub const MULTI_CLICK_GAP: Duration = Duration::from_millis(500);
/// Largest movement in points between presses that still counts as a multi-click.
pub const MULTI_CLICK_SLOP: f64 = 4.0;

impl ClickCounter {
    /// Click count for a press of `button` at `at` (points) at time `now`.
    pub fn press(&mut self, button: u8, at: (f64, f64), now: Instant) -> i64 {
        let count = match self.last {
            Some((b, pos, when, n))
                if b == button
                    && now.saturating_duration_since(when) <= MULTI_CLICK_GAP
                    && (pos.0 - at.0).abs() <= MULTI_CLICK_SLOP
                    && (pos.1 - at.1).abs() <= MULTI_CLICK_SLOP =>
            {
                n + 1
            }
            _ => 1,
        };
        self.last = Some((button, at, now, count));
        count
    }

    /// Click count to stamp on the matching release.
    pub fn current(&self) -> i64 {
        self.last.map_or(1, |(_, _, _, n)| n)
    }
}

/// Build the character table of a keyboard layout: `translate(keycode, shift, option)` returns
/// the character a key types (or `None` for dead keys and non-printing keys). Unmodified keys win
/// over shifted ones, shifted over Option, so the simplest way to type a character is chosen.
pub fn layout_table(
    mut translate: impl FnMut(u16, bool, bool) -> Option<char>,
) -> std::collections::HashMap<char, (u16, bool, bool)> {
    let mut table = std::collections::HashMap::new();
    for (shift, option) in [(false, false), (true, false), (false, true), (true, true)] {
        // Main-block keys only (0x00..=0x32), so keypad digits never shadow the number row.
        for keycode in 0..=0x32u16 {
            if let Some(ch) = translate(keycode, shift, option) {
                if !ch.is_control() {
                    table.entry(ch).or_insert((keycode, shift, option));
                }
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{parse_chord, Key, Modifier};

    #[test]
    fn named_keys_from_keys_rs_map() {
        for (name, code) in [
            ("Return", 0x24),
            ("Tab", 0x30),
            ("Escape", 0x35),
            ("space", 0x31),
            ("BackSpace", 0x33),
            ("Delete", 0x75),
            ("Home", 0x73),
            ("End", 0x77),
            ("Page_Up", 0x74),
            ("Page_Down", 0x79),
            ("Left", 0x7B),
            ("Up", 0x7E),
            ("Right", 0x7C),
            ("Down", 0x7D),
            ("F1", 0x7A),
            ("F5", 0x60),
            ("F12", 0x6F),
            ("F20", 0x5A),
            ("kp_0", 0x52),
            ("kp_9", 0x5C),
            ("kp_add", 0x45),
            ("kp_divide", 0x4B),
            ("kp_enter", 0x4C),
            ("caps_lock", 0x39),
        ] {
            let chord = parse_chord(name).unwrap();
            let Some(Key::Named(n)) = chord.key else {
                panic!("{name} is not a named key");
            };
            assert_eq!(mac_keycode(n.vk(), n.extended()), Some(code), "{name}");
        }
    }

    #[test]
    fn every_parsable_named_key_maps_or_is_known_missing() {
        let missing = [
            "scroll_lock",
            "menu",
            "print",
            "pause",
            "f21",
            "f24",
            "volume_mute",
            "volume_up",
            "media_next",
            "media_play_pause",
        ];
        for name in missing {
            let Some(Key::Named(n)) = parse_chord(name).unwrap().key else {
                panic!("{name}");
            };
            assert_eq!(mac_keycode(n.vk(), n.extended()), None, "{name}");
        }
    }

    #[test]
    fn modifiers_map_to_mac_modifiers() {
        assert_eq!(mac_keycode(Modifier::Super.vk(), true), Some(KC_COMMAND));
        assert_eq!(mac_keycode(Modifier::Ctrl.vk(), false), Some(KC_CONTROL));
        assert_eq!(mac_keycode(Modifier::Alt.vk(), false), Some(KC_OPTION));
        assert_eq!(mac_keycode(Modifier::Shift.vk(), false), Some(KC_SHIFT));
        assert_eq!(modifier_flag(KC_COMMAND), FLAG_COMMAND);
        assert_eq!(modifier_flag(KC_RIGHT_OPTION), FLAG_OPTION);
        assert_eq!(modifier_flag(0x00), 0);
    }

    #[test]
    fn letters_digits_and_punctuation_use_ansi_positions() {
        assert_eq!(mac_keycode(u16::from(b'A'), false), Some(0x00));
        assert_eq!(mac_keycode(u16::from(b'C'), false), Some(0x08));
        assert_eq!(mac_keycode(u16::from(b'Q'), false), Some(0x0C));
        assert_eq!(mac_keycode(u16::from(b'Z'), false), Some(0x06));
        assert_eq!(mac_keycode(u16::from(b'0'), false), Some(0x1D));
        assert_eq!(mac_keycode(u16::from(b'5'), false), Some(0x17));
        assert_eq!(mac_keycode(0xBD, false), Some(0x1B));
        assert_eq!(mac_keycode(0xDE, false), Some(0x27));
    }

    #[test]
    fn raw_mac_codes_pass_through() {
        assert_eq!(mac_keycode(RAW_MAC_KEY | 0x2E, false), Some(0x2E));
        assert_eq!(mac_keycode(RAW_MAC_KEY, false), Some(0x00));
    }

    #[test]
    fn wheel_notches_become_lines() {
        assert_eq!(wheel_lines(120), 3);
        assert_eq!(wheel_lines(-120), -3);
        assert_eq!(wheel_lines(10), 1);
        assert_eq!(wheel_lines(-10), -1);
        assert_eq!(wheel_lines(0), 0);
    }

    #[test]
    fn surrogates_join_into_one_press() {
        let mut j = SurrogateJoiner::default();
        let units: Vec<u16> = "😀".encode_utf16().collect();
        assert_eq!(j.push(units[0], true), None);
        assert_eq!(j.push(units[0], false), None);
        assert_eq!(j.push(units[1], true), Some(units.clone()));
        // The driver sends the low release last; it carries the pair.
        let mut j = SurrogateJoiner::default();
        j.push(units[0], true);
        assert_eq!(j.push(units[1], true), Some(units.clone()));
        assert_eq!(j.push(units[1], false), Some(vec![units[1]]));
        // BMP characters pass straight through.
        assert_eq!(j.push(u16::from(b'a'), true), Some(vec![u16::from(b'a')]));
        assert_eq!(j.push(u16::from(b'a'), false), Some(vec![u16::from(b'a')]));
    }

    #[test]
    fn double_clicks_are_counted() {
        let mut c = ClickCounter::default();
        let t0 = Instant::now();
        assert_eq!(c.press(0, (10.0, 10.0), t0), 1);
        assert_eq!(c.press(0, (11.0, 10.0), t0 + Duration::from_millis(100)), 2);
        assert_eq!(c.current(), 2);
        assert_eq!(c.press(0, (11.0, 10.0), t0 + Duration::from_millis(200)), 3);
        // Too late, too far, or another button starts over.
        assert_eq!(c.press(0, (11.0, 10.0), t0 + Duration::from_secs(2)), 1);
        assert_eq!(
            c.press(0, (50.0, 10.0), t0 + Duration::from_millis(2100)),
            1
        );
        assert_eq!(
            c.press(1, (50.0, 10.0), t0 + Duration::from_millis(2200)),
            1
        );
    }

    #[test]
    fn layout_table_prefers_unmodified_main_keys() {
        // A tiny fake layout: key 0 types a/A/å, key 1 types 1/!, keypad-range keys are ignored.
        let table = layout_table(|code, shift, option| match (code, shift, option) {
            (0, false, false) => Some('a'),
            (0, true, false) => Some('A'),
            (0, false, true) => Some('å'),
            (1, false, false) => Some('1'),
            (1, true, false) => Some('!'),
            (2, false, true) => Some('a'),
            (0x24, false, false) => Some('\r'),
            _ => None,
        });
        assert_eq!(table.get(&'a'), Some(&(0, false, false)));
        assert_eq!(table.get(&'A'), Some(&(0, true, false)));
        assert_eq!(table.get(&'å'), Some(&(0, false, true)));
        assert_eq!(table.get(&'!'), Some(&(1, true, false)));
        assert_eq!(table.get(&'\r'), None);
    }
}
