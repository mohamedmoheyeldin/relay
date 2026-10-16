//! MCP tool catalogue: names, descriptions and JSON schemas shown to the AI.

use serde_json::{json, Value};

/// Name the AI CLI uses for the permission-prompt tool (`mcp__<server>__<tool>`).
pub const SERVER_NAME: &str = "relay";
pub const PERMISSION_TOOL: &str = "mcp__relay__approve";

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

fn coord(desc: &str) -> Value {
    json!({
        "type": "array",
        "items": {"type": "integer"},
        "minItems": 2,
        "maxItems": 2,
        "description": desc
    })
}

fn modifiers() -> Value {
    json!({"type": "string", "description": "Modifier keys held during the action, e.g. \"ctrl\" or \"ctrl+shift\"."})
}

fn destructive() -> Value {
    json!({"type": "boolean", "description": "Set true if this action deletes, sends, submits, pays, publishes or otherwise cannot be undone. The owner is asked to approve first."})
}

fn object(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
}

fn click_spec(name: &'static str, description: &'static str) -> ToolSpec {
    ToolSpec {
        name,
        description,
        schema: object(
            json!({
                "coordinate": coord("[x, y] in screenshot pixels. Omit to click at the current pointer position."),
                "modifiers": modifiers(),
                "destructive": destructive()
            }),
            &[],
        ),
    }
}

/// Every tool the server offers.
pub fn tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "screenshot",
            description: "Take a screenshot of the active display. All coordinates you use in other tools are pixels of the most recent screenshot, origin top-left. Look before you act, and again after anything that might change the screen.",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "zoom",
            description: "Zoom into a region of the screen at higher detail. Coordinates elsewhere stay in full-screenshot pixels.",
            schema: object(
                json!({"region": {"type": "array", "items": {"type": "integer"}, "minItems": 4, "maxItems": 4, "description": "[x0, y0, x1, y1] inclusive, in screenshot pixels."}}),
                &["region"],
            ),
        },
        ToolSpec {
            name: "cursor_position",
            description: "Where the pointer is, in screenshot pixels.",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "wait",
            description: "Wait for the screen to settle (0 to 300 seconds).",
            schema: object(json!({"duration": {"type": "number", "description": "Seconds."}}), &["duration"]),
        },
        ToolSpec {
            name: "mouse_move",
            description: "Move the pointer without clicking.",
            schema: object(json!({"coordinate": coord("[x, y] in screenshot pixels.")}), &["coordinate"]),
        },
        click_spec("left_click", "Left click. Replies OK with whether the screen visibly changed."),
        click_spec("right_click", "Right click to open a context menu."),
        click_spec("middle_click", "Middle click."),
        click_spec("double_click", "Double click, for example to open a file."),
        click_spec("triple_click", "Triple click, for example to select a line of text."),
        ToolSpec {
            name: "left_click_drag",
            description: "Press at start_coordinate, drag to coordinate, release.",
            schema: object(
                json!({
                    "start_coordinate": coord("[x, y] where the drag starts."),
                    "coordinate": coord("[x, y] where the drag ends."),
                    "modifiers": modifiers(),
                    "destructive": destructive()
                }),
                &["start_coordinate", "coordinate"],
            ),
        },
        ToolSpec {
            name: "left_mouse_down",
            description: "Press and hold the left button (pair with left_mouse_up).",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "left_mouse_up",
            description: "Release the left button.",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "scroll",
            description: "Scroll with the mouse wheel.",
            schema: object(
                json!({
                    "coordinate": coord("[x, y] to scroll over. Omit for the current pointer position."),
                    "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
                    "amount": {"type": "integer", "description": "Wheel notches, 1 to 100. Default 3."},
                    "modifiers": modifiers()
                }),
                &["direction"],
            ),
        },
        ToolSpec {
            name: "type",
            description: "Type text into the focused control. Newline presses Enter and tab presses Tab. Never type passwords or secrets; ask the owner to enter them.",
            schema: object(
                json!({"text": {"type": "string"}, "destructive": destructive()}),
                &["text"],
            ),
        },
        ToolSpec {
            name: "key",
            description: "Press a key or chord, e.g. \"Return\", \"ctrl+s\", \"alt+Tab\", \"ctrl+shift+t\", \"super\". Only the last element may be a non-modifier.",
            schema: object(
                json!({
                    "keys": {"type": "string"},
                    "repeat": {"type": "integer", "description": "Times to press, 1 to 100."},
                    "destructive": destructive()
                }),
                &["keys"],
            ),
        },
        ToolSpec {
            name: "hold_key",
            description: "Hold a key for a number of seconds, then release it.",
            schema: object(
                json!({"keys": {"type": "string"}, "duration": {"type": "number"}}),
                &["keys", "duration"],
            ),
        },
        ToolSpec {
            name: "list_windows",
            description: "List visible top-level windows with their boxes in screenshot pixels. * marks the window in front.",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "focus_window",
            description: "Bring the first window whose title contains the given text to the front. Take a new screenshot afterwards.",
            schema: object(json!({"title": {"type": "string"}}), &["title"]),
        },
        ToolSpec {
            name: "list_displays",
            description: "List displays and which one is active.",
            schema: object(json!({}), &[]),
        },
        ToolSpec {
            name: "select_display",
            description: "Make another display the active one for screenshots and coordinates.",
            schema: object(json!({"display": {"type": "integer"}}), &["display"]),
        },
        ToolSpec {
            name: "computer_batch",
            description: "Run several desktop actions in order in one call and get a final screenshot. Stops at the first failure; later steps report that they were not executed. Use only when you are sure of the sequence.",
            schema: object(
                json!({"actions": {"type": "array", "minItems": 1, "maxItems": 25, "items": {
                    "type": "object",
                    "properties": {"name": {"type": "string"}, "input": {"type": "object"}},
                    "required": ["name"]
                }}}),
                &["actions"],
            ),
        },
        ToolSpec {
            name: "ask_user",
            description: "Ask the owner to approve something before you do it. Use before anything destructive or irreversible: deleting, sending, submitting a form, paying, publishing. Returns whether they approved.",
            schema: object(
                json!({
                    "question": {"type": "string", "description": "What you are about to do, in plain words."},
                    "risk": {"type": "string", "enum": ["reversible", "destructive"]}
                }),
                &["question"],
            ),
        },
        ToolSpec {
            name: "approve",
            description: "Internal permission check used by the host application.",
            schema: json!({
                "type": "object",
                "properties": {
                    "tool_name": {"type": "string"},
                    "input": {"type": "object"},
                    "tool_use_id": {"type": "string"}
                },
                "required": ["tool_name", "input"]
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn names_are_unique_and_schemas_are_objects() {
        let specs = tool_specs();
        let names: HashSet<_> = specs.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), specs.len());
        for s in &specs {
            assert_eq!(s.schema["type"], "object", "{}", s.name);
            assert!(!s.description.is_empty());
        }
    }

    #[test]
    fn every_action_tool_parses_with_its_required_fields() {
        use crate::actions::parse_action;
        let args = |name: &str| -> Value {
            match name {
                "zoom" => json!({"region":[0,0,5,5]}),
                "wait" => json!({"duration":1}),
                "mouse_move" => json!({"coordinate":[1,1]}),
                "left_click_drag" => json!({"start_coordinate":[1,1],"coordinate":[2,2]}),
                "scroll" => json!({"direction":"up"}),
                "type" => json!({"text":"a"}),
                "key" => json!({"keys":"a"}),
                "hold_key" => json!({"keys":"shift","duration":1}),
                "focus_window" => json!({"title":"a"}),
                "select_display" => json!({"display":1}),
                _ => json!({}),
            }
        };
        for spec in tool_specs() {
            if matches!(spec.name, "computer_batch" | "ask_user" | "approve") {
                continue;
            }
            assert!(
                parse_action(spec.name, &args(spec.name)).is_ok(),
                "{} does not parse",
                spec.name
            );
        }
    }

    #[test]
    fn permission_tool_name_matches_server_and_tool() {
        assert_eq!(PERMISSION_TOOL, format!("mcp__{SERVER_NAME}__approve"));
        assert!(tool_specs().iter().any(|s| s.name == "approve"));
    }
}
