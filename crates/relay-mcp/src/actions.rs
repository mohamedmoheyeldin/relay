//! Typed desktop actions parsed from MCP tool arguments, with strict validation.

use std::time::Duration;

use relay_os::driver::ScrollDirection;
use relay_os::keys::{parse_chord, parse_modifiers, Chord, Modifier};
use relay_os::types::MouseButton;
use serde_json::{Map, Value};

use crate::safety::truncate;

/// An error message intended for the AI model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub type Point = (i64, i64);

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Screenshot,
    Zoom {
        region: [i64; 4],
    },
    CursorPosition,
    Wait {
        duration: Duration,
    },
    MouseMove {
        at: Point,
    },
    Click {
        at: Option<Point>,
        button: MouseButton,
        count: u8,
        mods: Vec<Modifier>,
        destructive: bool,
    },
    Drag {
        from: Point,
        to: Point,
        mods: Vec<Modifier>,
        destructive: bool,
    },
    MouseDown,
    MouseUp,
    Scroll {
        at: Option<Point>,
        direction: ScrollDirection,
        amount: u32,
        mods: Vec<Modifier>,
    },
    Type {
        text: String,
        destructive: bool,
    },
    Key {
        raw: String,
        chord: Chord,
        repeat: u32,
        destructive: bool,
    },
    HoldKey {
        raw: String,
        chord: Chord,
        duration: Duration,
    },
    ListWindows,
    FocusWindow {
        title: String,
    },
    ListDisplays,
    SelectDisplay {
        id: u32,
    },
}

impl Action {
    /// Whether the action changes the desktop (reads work even after a stop).
    pub fn is_act(&self) -> bool {
        !matches!(
            self,
            Action::Screenshot
                | Action::Zoom { .. }
                | Action::CursorPosition
                | Action::Wait { .. }
                | Action::ListWindows
                | Action::ListDisplays
                | Action::SelectDisplay { .. }
        )
    }

    /// Plain-words description shown in approval prompts and the activity log.
    pub fn describe(&self) -> String {
        let at = |p: &Option<Point>| match p {
            Some((x, y)) => format!(" at ({x}, {y})"),
            None => " at the current pointer".to_string(),
        };
        match self {
            Action::Screenshot => "Take a screenshot".into(),
            Action::Zoom { region } => format!("Zoom into {region:?}"),
            Action::CursorPosition => "Read the pointer position".into(),
            Action::Wait { duration } => format!("Wait {:.1} s", duration.as_secs_f64()),
            Action::MouseMove { at } => format!("Move the pointer to ({}, {})", at.0, at.1),
            Action::Click {
                at: p,
                button,
                count,
                ..
            } => {
                let kind = match (button, count) {
                    (MouseButton::Left, 1) => "Left click",
                    (MouseButton::Left, 2) => "Double click",
                    (MouseButton::Left, _) => "Triple click",
                    (MouseButton::Right, _) => "Right click",
                    (MouseButton::Middle, _) => "Middle click",
                };
                format!("{kind}{}", at(p))
            }
            Action::Drag { from, to, .. } => {
                format!("Drag from ({}, {}) to ({}, {})", from.0, from.1, to.0, to.1)
            }
            Action::MouseDown => "Hold the left mouse button".into(),
            Action::MouseUp => "Release the left mouse button".into(),
            Action::Scroll {
                at: p,
                direction,
                amount,
                ..
            } => format!("Scroll {direction:?} by {amount}{}", at(p)),
            Action::Type { text, .. } => format!("Type \"{}\"", truncate(text, 300)),
            Action::Key { raw, repeat, .. } => {
                if *repeat > 1 {
                    format!("Press {raw} {repeat} times")
                } else {
                    format!("Press {raw}")
                }
            }
            Action::HoldKey { raw, duration, .. } => {
                format!("Hold {raw} for {:.1} s", duration.as_secs_f64())
            }
            Action::ListWindows => "List windows".into(),
            Action::FocusWindow { title } => format!("Bring the window \"{title}\" to the front"),
            Action::ListDisplays => "List displays".into(),
            Action::SelectDisplay { id } => format!("Switch to display {id}"),
        }
    }

    /// Whether the AI itself flagged the action as destructive.
    pub fn flagged_destructive(&self) -> bool {
        match self {
            Action::Click { destructive, .. }
            | Action::Drag { destructive, .. }
            | Action::Type { destructive, .. }
            | Action::Key { destructive, .. } => *destructive,
            _ => false,
        }
    }

    /// Whether the keys pressed are well known to delete things or close apps.
    pub fn is_dangerous_key(&self) -> bool {
        let Action::Key { chord, .. } = self else {
            return false;
        };
        use relay_os::keys::{Key, Named};
        let has = |m: Modifier| chord.modifiers.contains(&m);
        match chord.key {
            Some(Key::Named(Named::Delete)) => has(Modifier::Shift),
            Some(Key::Named(Named::F(4))) => has(Modifier::Alt),
            _ => false,
        }
    }

    /// The point the action will touch, in screenshot pixels, if any.
    pub fn target(&self) -> Option<Point> {
        match self {
            Action::Click { at, .. } | Action::Scroll { at, .. } => *at,
            Action::MouseMove { at } => Some(*at),
            Action::Drag { to, .. } => Some(*to),
            _ => None,
        }
    }
}

fn check_keys(obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), ToolError> {
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ToolError::new(format!(
                "unknown parameter {key:?}; allowed: {}",
                if allowed.is_empty() {
                    "none".to_string()
                } else {
                    allowed.join(", ")
                }
            )));
        }
    }
    Ok(())
}

fn as_int(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_i64().or_else(|| {
            n.as_f64()
                .filter(|f| f.fract() == 0.0 && f.abs() < 1e12)
                .map(|f| f as i64)
        }),
        _ => None,
    }
}

fn point(obj: &Map<String, Value>, key: &str) -> Result<Option<Point>, ToolError> {
    let Some(value) = obj.get(key) else {
        return Ok(None);
    };
    let bad = || ToolError::new(format!("{key} must be [x, y] with two whole numbers"));
    let arr = value.as_array().ok_or_else(bad)?;
    if arr.len() != 2 {
        return Err(bad());
    }
    let x = as_int(&arr[0]).ok_or_else(bad)?;
    let y = as_int(&arr[1]).ok_or_else(bad)?;
    Ok(Some((x, y)))
}

fn required_point(obj: &Map<String, Value>, key: &str) -> Result<Point, ToolError> {
    point(obj, key)?.ok_or_else(|| ToolError::new(format!("{key} is required")))
}

fn string(obj: &Map<String, Value>, key: &str) -> Result<Option<String>, ToolError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(ToolError::new(format!("{key} must be a string"))),
    }
}

fn required_string(obj: &Map<String, Value>, key: &str) -> Result<String, ToolError> {
    string(obj, key)?.ok_or_else(|| ToolError::new(format!("{key} is required")))
}

fn boolean(obj: &Map<String, Value>, key: &str) -> Result<bool, ToolError> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(ToolError::new(format!("{key} must be true or false"))),
    }
}

fn mods(obj: &Map<String, Value>) -> Result<Vec<Modifier>, ToolError> {
    match string(obj, "modifiers")? {
        None => Ok(Vec::new()),
        Some(s) if s.trim().is_empty() => Ok(Vec::new()),
        Some(s) => parse_modifiers(&s).map_err(|e| ToolError::new(e.to_string())),
    }
}

fn seconds(obj: &Map<String, Value>, key: &str) -> Result<Duration, ToolError> {
    let secs = obj
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| ToolError::new(format!("{key} must be a number of seconds")))?;
    if !(secs > 0.0 && secs <= 300.0) {
        return Err(ToolError::new(format!(
            "{key} must be greater than 0 and at most 300 seconds"
        )));
    }
    Ok(Duration::from_secs_f64(secs))
}

fn bounded(
    obj: &Map<String, Value>,
    key: &str,
    default: i64,
    min: i64,
    max: i64,
) -> Result<i64, ToolError> {
    let n = match obj.get(key) {
        None | Some(Value::Null) => default,
        Some(v) => {
            as_int(v).ok_or_else(|| ToolError::new(format!("{key} must be a whole number")))?
        }
    };
    if !(min..=max).contains(&n) {
        return Err(ToolError::new(format!("{key} must be {min}..={max}")));
    }
    Ok(n)
}

fn chord(raw: &str) -> Result<Chord, ToolError> {
    parse_chord(raw).map_err(|e| ToolError::new(e.to_string()))
}

/// Parse a tool call into an [`Action`], rejecting unknown tools and parameters.
pub fn parse_action(name: &str, args: &Value) -> Result<Action, ToolError> {
    let empty = Map::new();
    let obj = match args {
        Value::Object(o) => o,
        Value::Null => &empty,
        _ => return Err(ToolError::new("arguments must be an object")),
    };
    let click = |button: MouseButton, count: u8| -> Result<Action, ToolError> {
        check_keys(obj, &["coordinate", "modifiers", "destructive"])?;
        Ok(Action::Click {
            at: point(obj, "coordinate")?,
            button,
            count,
            mods: mods(obj)?,
            destructive: boolean(obj, "destructive")?,
        })
    };
    match name {
        "screenshot" => {
            check_keys(obj, &[])?;
            Ok(Action::Screenshot)
        }
        "zoom" => {
            check_keys(obj, &["region"])?;
            let bad = || ToolError::new("region must be [x0, y0, x1, y1] with whole numbers");
            let arr = obj
                .get("region")
                .and_then(Value::as_array)
                .ok_or_else(bad)?;
            if arr.len() != 4 {
                return Err(bad());
            }
            let mut region = [0i64; 4];
            for (slot, v) in region.iter_mut().zip(arr) {
                *slot = as_int(v).ok_or_else(bad)?;
            }
            Ok(Action::Zoom { region })
        }
        "cursor_position" => {
            check_keys(obj, &[])?;
            Ok(Action::CursorPosition)
        }
        "wait" => {
            check_keys(obj, &["duration"])?;
            Ok(Action::Wait {
                duration: seconds(obj, "duration")?,
            })
        }
        "mouse_move" => {
            check_keys(obj, &["coordinate"])?;
            Ok(Action::MouseMove {
                at: required_point(obj, "coordinate")?,
            })
        }
        "left_click" => click(MouseButton::Left, 1),
        "right_click" => click(MouseButton::Right, 1),
        "middle_click" => click(MouseButton::Middle, 1),
        "double_click" => click(MouseButton::Left, 2),
        "triple_click" => click(MouseButton::Left, 3),
        "left_click_drag" => {
            check_keys(
                obj,
                &["start_coordinate", "coordinate", "modifiers", "destructive"],
            )?;
            Ok(Action::Drag {
                from: required_point(obj, "start_coordinate")?,
                to: required_point(obj, "coordinate")?,
                mods: mods(obj)?,
                destructive: boolean(obj, "destructive")?,
            })
        }
        "left_mouse_down" => {
            check_keys(obj, &[])?;
            Ok(Action::MouseDown)
        }
        "left_mouse_up" => {
            check_keys(obj, &[])?;
            Ok(Action::MouseUp)
        }
        "scroll" => {
            check_keys(obj, &["coordinate", "direction", "amount", "modifiers"])?;
            let direction = ScrollDirection::parse(&required_string(obj, "direction")?)
                .map_err(|e| ToolError::new(e.to_string()))?;
            Ok(Action::Scroll {
                at: point(obj, "coordinate")?,
                direction,
                amount: bounded(obj, "amount", 3, 1, 100)? as u32,
                mods: mods(obj)?,
            })
        }
        "type" => {
            check_keys(obj, &["text", "destructive"])?;
            let text = required_string(obj, "text")?;
            if text.is_empty() {
                return Err(ToolError::new("text must not be empty"));
            }
            Ok(Action::Type {
                text,
                destructive: boolean(obj, "destructive")?,
            })
        }
        "key" => {
            check_keys(obj, &["keys", "repeat", "destructive"])?;
            let raw = required_string(obj, "keys")?;
            Ok(Action::Key {
                chord: chord(&raw)?,
                raw,
                repeat: bounded(obj, "repeat", 1, 1, 100)? as u32,
                destructive: boolean(obj, "destructive")?,
            })
        }
        "hold_key" => {
            check_keys(obj, &["keys", "duration"])?;
            let raw = required_string(obj, "keys")?;
            Ok(Action::HoldKey {
                chord: chord(&raw)?,
                raw,
                duration: seconds(obj, "duration")?,
            })
        }
        "list_windows" => {
            check_keys(obj, &[])?;
            Ok(Action::ListWindows)
        }
        "focus_window" => {
            check_keys(obj, &["title"])?;
            Ok(Action::FocusWindow {
                title: required_string(obj, "title")?,
            })
        }
        "list_displays" => {
            check_keys(obj, &[])?;
            Ok(Action::ListDisplays)
        }
        "select_display" => {
            check_keys(obj, &["display"])?;
            Ok(Action::SelectDisplay {
                id: bounded(obj, "display", 1, 1, 64)? as u32,
            })
        }
        other => Err(ToolError::new(format!("unknown action {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ok(name: &str, args: Value) -> Action {
        parse_action(name, &args).unwrap()
    }
    fn err(name: &str, args: Value) -> String {
        parse_action(name, &args).unwrap_err().0
    }

    #[test]
    fn clicks_parse_buttons_counts_and_modifiers() {
        let a = ok(
            "double_click",
            json!({"coordinate":[10,20],"modifiers":"ctrl+shift"}),
        );
        assert_eq!(
            a,
            Action::Click {
                at: Some((10, 20)),
                button: MouseButton::Left,
                count: 2,
                mods: vec![Modifier::Ctrl, Modifier::Shift],
                destructive: false
            }
        );
        assert!(matches!(
            ok("right_click", json!({})),
            Action::Click {
                button: MouseButton::Right,
                at: None,
                ..
            }
        ));
        assert!(matches!(
            ok("triple_click", json!({"coordinate":[1,1]})),
            Action::Click { count: 3, .. }
        ));
    }

    #[test]
    fn coordinates_must_be_whole_number_pairs() {
        for bad in [
            json!({"coordinate":[1.5,2]}),
            json!({"coordinate":[1]}),
            json!({"coordinate":[1,2,3]}),
            json!({"coordinate":"1,2"}),
            json!({"coordinate":[true,2]}),
            json!({"coordinate":["1",2]}),
        ] {
            assert!(
                err("left_click", bad.clone()).contains("coordinate"),
                "{bad}"
            );
        }
        assert_eq!(
            ok("mouse_move", json!({"coordinate":[3.0, 4.0]})),
            Action::MouseMove { at: (3, 4) }
        );
    }

    #[test]
    fn unknown_parameters_and_actions_are_rejected_clearly() {
        assert!(err("left_click", json!({"colour":"red"})).contains("unknown parameter"));
        assert!(err("screenshot", json!({"x":1})).contains("unknown parameter"));
        assert!(err("teleport", json!({})).contains("unknown action"));
        assert!(err("screenshot", json!(5)).contains("object"));
    }

    #[test]
    fn required_fields_are_enforced() {
        assert!(err("mouse_move", json!({})).contains("required"));
        assert!(err("type", json!({})).contains("required"));
        assert!(err("type", json!({"text":""})).contains("empty"));
        assert!(err("key", json!({})).contains("required"));
        assert!(err("scroll", json!({})).contains("direction"));
        assert!(err("left_click_drag", json!({"coordinate":[1,1]})).contains("start_coordinate"));
    }

    #[test]
    fn scroll_amount_and_direction_are_bounded() {
        assert!(matches!(
            ok("scroll", json!({"direction":"down"})),
            Action::Scroll { amount: 3, .. }
        ));
        assert!(err("scroll", json!({"direction":"down","amount":0})).contains("amount"));
        assert!(err("scroll", json!({"direction":"down","amount":101})).contains("amount"));
        assert!(err("scroll", json!({"direction":"sideways"})).contains("direction"));
    }

    #[test]
    fn durations_must_be_in_range() {
        assert!(matches!(
            ok("wait", json!({"duration":0.5})),
            Action::Wait { .. }
        ));
        for bad in [
            json!({"duration":0}),
            json!({"duration":301}),
            json!({"duration":-1}),
            json!({}),
        ] {
            assert!(parse_action("wait", &bad).is_err(), "{bad}");
        }
        assert!(parse_action("hold_key", &json!({"keys":"shift","duration":301})).is_err());
    }

    #[test]
    fn key_chords_are_validated_at_parse_time() {
        let a = ok("key", json!({"keys":"ctrl+s","repeat":2}));
        assert!(matches!(a, Action::Key { repeat: 2, .. }));
        assert!(err("key", json!({"keys":"ctrl+"})).contains("ends with"));
        assert!(err("key", json!({"keys":"a","repeat":0})).contains("repeat"));
        assert!(err("left_click", json!({"modifiers":"ctrl+a"})).contains("modifier"));
    }

    #[test]
    fn acts_and_reads_are_distinguished() {
        for read in [
            "screenshot",
            "cursor_position",
            "list_windows",
            "list_displays",
        ] {
            assert!(!ok(read, json!({})).is_act(), "{read}");
        }
        assert!(!ok("zoom", json!({"region":[0,0,5,5]})).is_act());
        assert!(!ok("wait", json!({"duration":1})).is_act());
        for act in ["left_click", "left_mouse_down", "left_mouse_up"] {
            assert!(ok(act, json!({})).is_act(), "{act}");
        }
        assert!(ok("type", json!({"text":"a"})).is_act());
        assert!(ok("focus_window", json!({"title":"x"})).is_act());
    }

    #[test]
    fn descriptions_are_plain_and_truncated() {
        assert_eq!(
            ok("left_click", json!({"coordinate":[100,50]})).describe(),
            "Left click at (100, 50)"
        );
        assert_eq!(
            ok("key", json!({"keys":"ctrl+s","repeat":2})).describe(),
            "Press ctrl+s 2 times"
        );
        assert_eq!(
            ok(
                "left_click_drag",
                json!({"start_coordinate":[1,2],"coordinate":[3,4]})
            )
            .describe(),
            "Drag from (1, 2) to (3, 4)"
        );
        let long = "x".repeat(1000);
        let d = ok("type", json!({"text": long})).describe();
        assert!(d.chars().count() < 330, "{}", d.len());
    }

    #[test]
    fn destructive_flag_and_dangerous_keys() {
        assert!(ok("left_click", json!({"destructive":true})).flagged_destructive());
        assert!(!ok("left_click", json!({})).flagged_destructive());
        assert!(ok("key", json!({"keys":"shift+Delete"})).is_dangerous_key());
        assert!(ok("key", json!({"keys":"alt+F4"})).is_dangerous_key());
        assert!(!ok("key", json!({"keys":"Delete"})).is_dangerous_key());
        assert!(!ok("key", json!({"keys":"ctrl+c"})).is_dangerous_key());
        assert!(parse_action("left_click", &json!({"destructive":"yes"})).is_err());
    }

    #[test]
    fn target_points() {
        assert_eq!(
            ok("left_click", json!({"coordinate":[5,6]})).target(),
            Some((5, 6))
        );
        assert_eq!(ok("screenshot", json!({})).target(), None);
    }
}
