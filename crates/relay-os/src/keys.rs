//! Key name and chord parsing (`"ctrl+shift+t"`, `"Page_Down"`, `"ctrl++"`).

use crate::types::{OsError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Super,
}

impl Modifier {
    pub fn vk(self) -> u16 {
        match self {
            Modifier::Shift => 0x10,
            Modifier::Ctrl => 0x11,
            Modifier::Alt => 0x12,
            Modifier::Super => 0x5B,
        }
    }

    pub fn extended(self) -> bool {
        matches!(self, Modifier::Super)
    }

    fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "shift" | "shift_l" | "shift_r" => Some(Modifier::Shift),
            "ctrl" | "control" | "control_l" | "control_r" | "ctrl_l" | "ctrl_r" | "ctl" => {
                Some(Modifier::Ctrl)
            }
            "alt" | "alt_l" | "alt_r" | "option" => Some(Modifier::Alt),
            "super" | "super_l" | "super_r" | "win" | "windows" | "meta" | "cmd" | "command" => {
                Some(Modifier::Super)
            }
            _ => None,
        }
    }
}

/// Non-character keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Return,
    KeypadEnter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Up,
    Right,
    Down,
    CapsLock,
    NumLock,
    ScrollLock,
    Menu,
    PrintScreen,
    Pause,
    F(u8),
    Keypad(u8),
    KeypadAdd,
    KeypadSubtract,
    KeypadMultiply,
    KeypadDivide,
    KeypadDecimal,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNext,
    MediaPrev,
    MediaStop,
    MediaPlayPause,
}

impl Named {
    /// Windows virtual-key code.
    pub fn vk(self) -> u16 {
        match self {
            Named::Return | Named::KeypadEnter => 0x0D,
            Named::Tab => 0x09,
            Named::Escape => 0x1B,
            Named::Space => 0x20,
            Named::Backspace => 0x08,
            Named::Delete => 0x2E,
            Named::Insert => 0x2D,
            Named::Home => 0x24,
            Named::End => 0x23,
            Named::PageUp => 0x21,
            Named::PageDown => 0x22,
            Named::Left => 0x25,
            Named::Up => 0x26,
            Named::Right => 0x27,
            Named::Down => 0x28,
            Named::CapsLock => 0x14,
            Named::NumLock => 0x90,
            Named::ScrollLock => 0x91,
            Named::Menu => 0x5D,
            Named::PrintScreen => 0x2C,
            Named::Pause => 0x13,
            Named::F(n) => 0x70 + u16::from(n.clamp(1, 24)) - 1,
            Named::Keypad(d) => 0x60 + u16::from(d.min(9)),
            Named::KeypadMultiply => 0x6A,
            Named::KeypadAdd => 0x6B,
            Named::KeypadSubtract => 0x6D,
            Named::KeypadDecimal => 0x6E,
            Named::KeypadDivide => 0x6F,
            Named::VolumeMute => 0xAD,
            Named::VolumeDown => 0xAE,
            Named::VolumeUp => 0xAF,
            Named::MediaNext => 0xB0,
            Named::MediaPrev => 0xB1,
            Named::MediaStop => 0xB2,
            Named::MediaPlayPause => 0xB3,
        }
    }

    /// Whether the extended-key flag must be set so the key is not read as its numpad twin.
    pub fn extended(self) -> bool {
        matches!(
            self,
            Named::KeypadEnter
                | Named::Delete
                | Named::Insert
                | Named::Home
                | Named::End
                | Named::PageUp
                | Named::PageDown
                | Named::Left
                | Named::Up
                | Named::Right
                | Named::Down
                | Named::NumLock
                | Named::Menu
                | Named::PrintScreen
                | Named::KeypadDivide
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Named(Named),
    Char(char),
}

/// A parsed chord: modifiers (press order, de-duplicated) plus an optional final key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Vec<Modifier>,
    pub key: Option<Key>,
}

fn push_unique(mods: &mut Vec<Modifier>, m: Modifier) {
    if !mods.contains(&m) {
        mods.push(m);
    }
}

/// Parse a single key name (no modifiers).
pub fn parse_key(name: &str) -> Result<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(Key::Char(c));
    }
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "return" | "enter" | "kp_enter_main" => Named::Return,
        "kp_enter" | "keypad_enter" => Named::KeypadEnter,
        "tab" => Named::Tab,
        "escape" | "esc" => Named::Escape,
        "space" => Named::Space,
        "backspace" | "back_space" => Named::Backspace,
        "delete" | "del" => Named::Delete,
        "insert" | "ins" => Named::Insert,
        "home" => Named::Home,
        "end" => Named::End,
        "page_up" | "pageup" | "pgup" | "prior" => Named::PageUp,
        "page_down" | "pagedown" | "pgdn" | "next" => Named::PageDown,
        "left" => Named::Left,
        "up" => Named::Up,
        "right" => Named::Right,
        "down" => Named::Down,
        "caps_lock" | "capslock" => Named::CapsLock,
        "num_lock" | "numlock" => Named::NumLock,
        "scroll_lock" | "scrolllock" => Named::ScrollLock,
        "menu" | "apps" => Named::Menu,
        "print" | "printscreen" | "print_screen" => Named::PrintScreen,
        "pause" | "break" => Named::Pause,
        "kp_add" => Named::KeypadAdd,
        "kp_subtract" => Named::KeypadSubtract,
        "kp_multiply" => Named::KeypadMultiply,
        "kp_divide" => Named::KeypadDivide,
        "kp_decimal" => Named::KeypadDecimal,
        "xf86audiomute" | "volume_mute" => Named::VolumeMute,
        "xf86audiolowervolume" | "volume_down" => Named::VolumeDown,
        "xf86audioraisevolume" | "volume_up" => Named::VolumeUp,
        "xf86audionext" | "media_next" => Named::MediaNext,
        "xf86audioprev" | "media_prev" => Named::MediaPrev,
        "xf86audiostop" | "media_stop" => Named::MediaStop,
        "xf86audioplay" | "media_play_pause" => Named::MediaPlayPause,
        // Spelled-out punctuation resolves to the character itself.
        "plus" => return Ok(Key::Char('+')),
        "minus" => return Ok(Key::Char('-')),
        "equal" => return Ok(Key::Char('=')),
        "comma" => return Ok(Key::Char(',')),
        "period" => return Ok(Key::Char('.')),
        "slash" => return Ok(Key::Char('/')),
        "backslash" => return Ok(Key::Char('\\')),
        "semicolon" => return Ok(Key::Char(';')),
        "apostrophe" => return Ok(Key::Char('\'')),
        "quotedbl" => return Ok(Key::Char('"')),
        "grave" => return Ok(Key::Char('`')),
        "bracketleft" => return Ok(Key::Char('[')),
        "bracketright" => return Ok(Key::Char(']')),
        "underscore" => return Ok(Key::Char('_')),
        other => {
            if let Some(n) = other.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
                if (1..=24).contains(&n) {
                    return Ok(Key::Named(Named::F(n)));
                }
            }
            if let Some(d) = other.strip_prefix("kp_").and_then(|d| d.parse::<u8>().ok()) {
                if d <= 9 {
                    return Ok(Key::Named(Named::Keypad(d)));
                }
            }
            return Err(OsError::InvalidArgument(format!(
                "unknown key name {name:?}"
            )));
        }
    };
    Ok(Key::Named(named))
}

/// Parse a chord such as `ctrl+shift+t`, `alt+Tab`, `ctrl++`, `+`, `super`.
pub fn parse_chord(input: &str) -> Result<Chord> {
    let s = input.trim();
    if s.is_empty() {
        return Err(OsError::InvalidArgument("empty key".into()));
    }
    let (head, last) = if s == "+" {
        ("", "+")
    } else if let Some(h) = s.strip_suffix("++") {
        (h, "+")
    } else {
        match s.rfind('+') {
            Some(i) => (&s[..i], &s[i + 1..]),
            None => ("", s),
        }
    };
    if last.is_empty() {
        return Err(OsError::InvalidArgument(format!(
            "key {input:?} ends with '+'; write the key name (e.g. \"plus\")"
        )));
    }
    let mut modifiers = Vec::new();
    if !head.is_empty() {
        for token in head.split('+') {
            let m = Modifier::parse(token.trim()).ok_or_else(|| {
                OsError::InvalidArgument(format!(
                    "{token:?} in {input:?} is not a modifier; only the last element may be a non-modifier"
                ))
            })?;
            push_unique(&mut modifiers, m);
        }
    }
    if let Some(m) = Modifier::parse(last) {
        push_unique(&mut modifiers, m);
        return Ok(Chord {
            modifiers,
            key: None,
        });
    }
    Ok(Chord {
        modifiers,
        key: Some(parse_key(last)?),
    })
}

/// Parse a modifier-only list as used by click/scroll/drag (`"ctrl+shift"`, `"win"`).
pub fn parse_modifiers(input: &str) -> Result<Vec<Modifier>> {
    let mut out = Vec::new();
    for token in input.split('+') {
        let token = token.trim();
        let m = Modifier::parse(token).ok_or_else(|| {
            OsError::InvalidArgument(format!(
                "{token:?} is not a modifier (use shift, ctrl, alt or super)"
            ))
        })?;
        push_unique(&mut out, m);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(s: &str) -> Chord {
        parse_chord(s).unwrap()
    }

    #[test]
    fn simple_names_and_aliases() {
        assert_eq!(chord("Return").key, Some(Key::Named(Named::Return)));
        assert_eq!(chord("enter").key, Some(Key::Named(Named::Return)));
        assert_eq!(chord("Page_Down").key, Some(Key::Named(Named::PageDown)));
        assert_eq!(chord("pgup").key, Some(Key::Named(Named::PageUp)));
        assert_eq!(chord("F5").key, Some(Key::Named(Named::F(5))));
        assert_eq!(chord("f24").key, Some(Key::Named(Named::F(24))));
        assert_eq!(chord("kp_7").key, Some(Key::Named(Named::Keypad(7))));
        assert_eq!(chord("ESC").key, Some(Key::Named(Named::Escape)));
    }

    #[test]
    fn chords_keep_press_order_and_dedupe() {
        let c = chord("ctrl+shift+t");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(c.key, Some(Key::Char('t')));
        let c = chord("Control_L+ctrl+a");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
        let c = chord("cmd+space");
        assert_eq!(c.modifiers, vec![Modifier::Super]);
        assert_eq!(c.key, Some(Key::Named(Named::Space)));
    }

    #[test]
    fn plus_key_forms() {
        assert_eq!(chord("+").key, Some(Key::Char('+')));
        let c = chord("ctrl++");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
        assert_eq!(c.key, Some(Key::Char('+')));
        assert_eq!(chord("ctrl+plus").key, Some(Key::Char('+')));
    }

    #[test]
    fn modifier_only_chords() {
        let c = chord("super");
        assert_eq!(c.modifiers, vec![Modifier::Super]);
        assert_eq!(c.key, None);
        let c = chord("ctrl+shift");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl, Modifier::Shift]);
        assert_eq!(c.key, None);
    }

    #[test]
    fn uppercase_character_is_preserved() {
        assert_eq!(chord("A").key, Some(Key::Char('A')));
    }

    #[test]
    fn invalid_chords_are_rejected() {
        for bad in ["", "   ", "ctrl+", "a+b", "nonsense", "ctrl++a", "f25"] {
            assert!(parse_chord(bad).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn extended_flag_set_for_navigation_keys() {
        for n in [
            Named::Delete,
            Named::Insert,
            Named::Home,
            Named::End,
            Named::PageUp,
            Named::PageDown,
            Named::Left,
            Named::Right,
            Named::Up,
            Named::Down,
            Named::KeypadEnter,
        ] {
            assert!(n.extended(), "{n:?}");
        }
        assert!(!Named::Return.extended());
        assert!(!Named::Tab.extended());
        assert!(Modifier::Super.extended());
    }

    #[test]
    fn virtual_key_codes() {
        assert_eq!(Named::Return.vk(), 0x0D);
        assert_eq!(Named::F(1).vk(), 0x70);
        assert_eq!(Named::F(12).vk(), 0x7B);
        assert_eq!(Named::Keypad(0).vk(), 0x60);
        assert_eq!(Modifier::Ctrl.vk(), 0x11);
    }

    #[test]
    fn modifier_lists() {
        assert_eq!(
            parse_modifiers("ctrl+shift").unwrap(),
            vec![Modifier::Ctrl, Modifier::Shift]
        );
        assert_eq!(parse_modifiers("win").unwrap(), vec![Modifier::Super]);
        assert!(parse_modifiers("ctrl+a").is_err());
        assert!(parse_modifiers("").is_err());
    }
}
