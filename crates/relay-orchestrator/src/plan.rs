//! The structured execution plan: the only thing AI output is allowed to turn into actions.
//!
//! Plans are untrusted proposals. They are parsed against a closed set of typed actions
//! (there is no "run this shell command" action), validated, and then executed by Rust.

use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_STEPS: usize = 40;
const MAX_TEXT: usize = 4_000;

/// Which element of an application a step refers to. Resolved against the live UI when the step runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct UiTarget {
    /// Part of the window title the element lives in.
    #[serde(default)]
    pub window: Option<String>,
    /// Accessible name of the element (case-insensitive, exact or contained).
    #[serde(default)]
    pub name: Option<String>,
    /// Control type such as `button`, `edit`, `checkbox`, `menu_item`, `tab`, `link`.
    #[serde(default)]
    pub role: Option<String>,
    /// Stable automation id, when the app provides one.
    #[serde(default)]
    pub automation_id: Option<String>,
}

impl UiTarget {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.role.is_none() && self.automation_id.is_none()
    }
}

/// Something that must be true after a step for it to count as done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum Verification {
    WindowExists {
        title: String,
    },
    WindowActive {
        title: String,
    },
    FileExists {
        path: String,
    },
    FileMissing {
        path: String,
    },
    UiElementExists {
        target: UiTarget,
    },
    /// An element's text or value contains the given text.
    UiTextContains {
        target: UiTarget,
        text: String,
    },
}

/// The closed set of things a plan may ask Relay to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "tool", rename_all = "snake_case")]
pub enum PlanAction {
    /// Start an installed application by name (Start menu entry or executable name).
    OpenApp {
        name: String,
    },
    /// Open a web address in the default browser (http, https and mailto only).
    OpenUrl {
        url: String,
    },
    /// Bring a window to the front.
    FocusWindow {
        title: String,
    },
    /// Search for files by name under a folder.
    FindFiles {
        query: String,
        #[serde(default)]
        root: Option<String>,
        #[serde(default)]
        max_results: Option<u32>,
    },
    ListDir {
        path: String,
    },
    /// Read a text file (sensitive: its content may be shown to the AI).
    ReadTextFile {
        path: String,
        #[serde(default)]
        max_bytes: Option<u32>,
    },
    CreateDir {
        path: String,
    },
    MovePath {
        from: String,
        to: String,
    },
    /// Create or replace a text file.
    WriteTextFile {
        path: String,
        content: String,
        #[serde(default)]
        overwrite: bool,
    },
    /// Move files to the Recycle Bin / Trash (recoverable, but still destructive).
    MoveToTrash {
        paths: Vec<String>,
    },
    UiClick {
        target: UiTarget,
    },
    UiSetText {
        target: UiTarget,
        text: String,
    },
    /// Type text into whatever has keyboard focus.
    TypeText {
        text: String,
    },
    /// Press a key or chord such as `ctrl+s` or `Return`.
    PressKeys {
        keys: String,
    },
    /// Wait until a condition holds.
    WaitFor {
        condition: Verification,
        #[serde(default)]
        timeout_secs: Option<u32>,
    },
}

impl PlanAction {
    /// The tool name as it appears in schemas, permissions and the audit log.
    pub fn tool(&self) -> &'static str {
        match self {
            PlanAction::OpenApp { .. } => "open_app",
            PlanAction::OpenUrl { .. } => "open_url",
            PlanAction::FocusWindow { .. } => "focus_window",
            PlanAction::FindFiles { .. } => "find_files",
            PlanAction::ListDir { .. } => "list_dir",
            PlanAction::ReadTextFile { .. } => "read_text_file",
            PlanAction::CreateDir { .. } => "create_dir",
            PlanAction::MovePath { .. } => "move_path",
            PlanAction::WriteTextFile { .. } => "write_text_file",
            PlanAction::MoveToTrash { .. } => "move_to_trash",
            PlanAction::UiClick { .. } => "ui_click",
            PlanAction::UiSetText { .. } => "ui_set_text",
            PlanAction::TypeText { .. } => "type_text",
            PlanAction::PressKeys { .. } => "press_keys",
            PlanAction::WaitFor { .. } => "wait_for",
        }
    }

    /// Short human description for the timeline and confirmation prompts.
    pub fn describe(&self) -> String {
        let t = |s: &str| {
            let s = s.trim();
            if s.chars().count() > 80 {
                format!("{}…", s.chars().take(80).collect::<String>())
            } else {
                s.to_string()
            }
        };
        let target = |u: &UiTarget| {
            u.name
                .clone()
                .or_else(|| u.automation_id.clone())
                .or_else(|| u.role.clone())
                .unwrap_or_else(|| "a control".into())
        };
        match self {
            PlanAction::OpenApp { name } => format!("Open {}", t(name)),
            PlanAction::OpenUrl { url } => format!("Open {}", t(url)),
            PlanAction::FocusWindow { title } => format!("Switch to “{}”", t(title)),
            PlanAction::FindFiles { query, root, .. } => match root {
                Some(r) => format!("Find “{}” in {}", t(query), t(r)),
                None => format!("Find “{}”", t(query)),
            },
            PlanAction::ListDir { path } => format!("List {}", t(path)),
            PlanAction::ReadTextFile { path, .. } => format!("Read {}", t(path)),
            PlanAction::CreateDir { path } => format!("Create folder {}", t(path)),
            PlanAction::MovePath { from, to } => format!("Move {} to {}", t(from), t(to)),
            PlanAction::WriteTextFile { path, .. } => format!("Write {}", t(path)),
            PlanAction::MoveToTrash { paths } => {
                format!("Move {} item(s) to the Recycle Bin", paths.len())
            }
            PlanAction::UiClick { target: u } => format!("Click “{}”", t(&target(u))),
            PlanAction::UiSetText { target: u, text } => {
                format!("Enter “{}” in “{}”", t(text), t(&target(u)))
            }
            PlanAction::TypeText { text } => format!("Type “{}”", t(text)),
            PlanAction::PressKeys { keys } => format!("Press {}", t(keys)),
            PlanAction::WaitFor { .. } => "Wait for the screen to be ready".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanStep {
    /// Unique short id such as `s1`.
    pub id: String,
    /// One-line explanation shown to the owner.
    pub title: String,
    pub action: PlanAction,
    /// How to check the step worked. Omit when the action's own result is enough.
    #[serde(default)]
    pub verify: Option<Verification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionPlan {
    pub objective: String,
    pub steps: Vec<PlanStep>,
    /// Anything the owner should know (assumptions, things the plan cannot do).
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("the AI did not return a plan I can read: {0}")]
    Unreadable(String),
    #[error("the plan is empty")]
    Empty,
    #[error("the plan has {0} steps; the limit is {MAX_STEPS}")]
    TooLong(usize),
    #[error("step id {0:?} is used twice")]
    DuplicateId(String),
    #[error("step {0:?} is invalid: {1}")]
    BadStep(String, String),
}

/// JSON Schema handed to providers that support structured output.
pub fn plan_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(ExecutionPlan)).unwrap_or(Value::Null)
}

/// Pull the JSON document out of model output (plain JSON, or JSON inside a code fence or prose).
pub fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        return Some(trimmed);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    (end > start).then(|| &trimmed[start..=end])
}

/// Parse and validate a plan from model output.
pub fn parse_plan(text: &str) -> Result<ExecutionPlan, PlanError> {
    let json =
        extract_json(text).ok_or_else(|| PlanError::Unreadable("no JSON object found".into()))?;
    let plan: ExecutionPlan =
        serde_json::from_str(json).map_err(|e| PlanError::Unreadable(e.to_string()))?;
    validate(&plan)?;
    Ok(plan)
}

fn check_text(id: &str, what: &str, value: &str) -> Result<(), PlanError> {
    if value.trim().is_empty() {
        return Err(PlanError::BadStep(id.into(), format!("{what} is empty")));
    }
    if value.chars().count() > MAX_TEXT {
        return Err(PlanError::BadStep(id.into(), format!("{what} is too long")));
    }
    if value.contains('\0') {
        return Err(PlanError::BadStep(
            id.into(),
            format!("{what} contains a NUL character"),
        ));
    }
    Ok(())
}

fn check_action(id: &str, action: &PlanAction) -> Result<(), PlanError> {
    let target = |u: &UiTarget| {
        if u.is_empty() {
            Err(PlanError::BadStep(
                id.into(),
                "a UI target needs a name, role or automation id".into(),
            ))
        } else {
            Ok(())
        }
    };
    match action {
        PlanAction::OpenApp { name } => check_text(id, "app name", name),
        PlanAction::OpenUrl { url } => {
            check_text(id, "url", url)?;
            let lower = url.trim().to_ascii_lowercase();
            if lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("mailto:")
            {
                Ok(())
            } else {
                Err(PlanError::BadStep(
                    id.into(),
                    "only http, https and mailto links are allowed".into(),
                ))
            }
        }
        PlanAction::FocusWindow { title } => check_text(id, "window title", title),
        PlanAction::FindFiles { query, .. } => check_text(id, "query", query),
        PlanAction::ListDir { path }
        | PlanAction::ReadTextFile { path, .. }
        | PlanAction::CreateDir { path } => check_text(id, "path", path),
        PlanAction::MovePath { from, to } => {
            check_text(id, "source path", from)?;
            check_text(id, "target path", to)
        }
        PlanAction::WriteTextFile { path, content, .. } => {
            check_text(id, "path", path)?;
            if content.len() > 200_000 {
                return Err(PlanError::BadStep(id.into(), "content is too large".into()));
            }
            Ok(())
        }
        PlanAction::MoveToTrash { paths } => {
            if paths.is_empty() || paths.len() > 500 {
                return Err(PlanError::BadStep(
                    id.into(),
                    "give between 1 and 500 paths".into(),
                ));
            }
            paths.iter().try_for_each(|p| check_text(id, "path", p))
        }
        PlanAction::UiClick { target: u } => target(u),
        PlanAction::UiSetText { target: u, text } => {
            target(u)?;
            if text.chars().count() > MAX_TEXT {
                return Err(PlanError::BadStep(id.into(), "text is too long".into()));
            }
            Ok(())
        }
        PlanAction::TypeText { text } => check_text(id, "text", text),
        PlanAction::PressKeys { keys } => check_text(id, "keys", keys),
        PlanAction::WaitFor { timeout_secs, .. } => match timeout_secs {
            Some(0) | Some(301..) => Err(PlanError::BadStep(
                id.into(),
                "timeout must be 1 to 300 seconds".into(),
            )),
            _ => Ok(()),
        },
    }
}

/// Structural validation (permissions are checked separately, at execution time).
pub fn validate(plan: &ExecutionPlan) -> Result<(), PlanError> {
    if plan.steps.is_empty() {
        return Err(PlanError::Empty);
    }
    if plan.steps.len() > MAX_STEPS {
        return Err(PlanError::TooLong(plan.steps.len()));
    }
    let mut seen = HashSet::new();
    for step in &plan.steps {
        if step.id.trim().is_empty() || !seen.insert(step.id.clone()) {
            return Err(PlanError::DuplicateId(step.id.clone()));
        }
        check_text(&step.id, "title", &step.title)?;
        check_action(&step.id, &step.action)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan_json() -> Value {
        json!({
            "objective": "Open notepad and type hello",
            "steps": [
                {"id": "s1", "title": "Open Notepad", "action": {"tool": "open_app", "name": "Notepad"},
                 "verify": {"check": "window_exists", "title": "Notepad"}},
                {"id": "s2", "title": "Type", "action": {"tool": "type_text", "text": "hello"}}
            ],
            "notes": "assumes Notepad is installed"
        })
    }

    #[test]
    fn a_valid_plan_parses() {
        let plan = parse_plan(&plan_json().to_string()).unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].action.tool(), "open_app");
        assert!(matches!(
            plan.steps[0].verify,
            Some(Verification::WindowExists { .. })
        ));
        assert_eq!(plan.steps[1].action.describe(), "Type “hello”");
    }

    #[test]
    fn json_inside_fences_or_prose_is_found() {
        let wrapped = format!("Here is the plan:\n```json\n{}\n```\nDone.", plan_json());
        assert!(parse_plan(&wrapped).is_ok());
        assert!(matches!(
            parse_plan("no json here"),
            Err(PlanError::Unreadable(_))
        ));
        assert!(matches!(
            parse_plan("{ not valid"),
            Err(PlanError::Unreadable(_))
        ));
    }

    #[test]
    fn unknown_tools_are_rejected_there_is_no_shell_action() {
        for tool in ["run_shell", "powershell", "exec", "delete_everything"] {
            let mut v = plan_json();
            v["steps"][0]["action"] = json!({"tool": tool, "command": "rm -rf /"});
            assert!(parse_plan(&v.to_string()).is_err(), "{tool}");
        }
        let n = plan_schema().to_string();
        assert!(!n.contains("run_shell") && !n.contains("powershell"));
    }

    #[test]
    fn structural_rules() {
        let mut v = plan_json();
        v["steps"] = json!([]);
        assert_eq!(parse_plan(&v.to_string()), Err(PlanError::Empty));
        let mut v = plan_json();
        v["steps"][1]["id"] = json!("s1");
        assert!(matches!(
            parse_plan(&v.to_string()),
            Err(PlanError::DuplicateId(_))
        ));
        let mut v = plan_json();
        v["steps"][0]["action"]["name"] = json!("   ");
        assert!(matches!(
            parse_plan(&v.to_string()),
            Err(PlanError::BadStep(..))
        ));
        let many: Vec<Value> = (0..41)
            .map(|i| json!({"id": format!("s{i}"), "title": "t", "action": {"tool": "press_keys", "keys": "a"}}))
            .collect();
        let v = json!({"objective": "x", "steps": many});
        assert_eq!(parse_plan(&v.to_string()), Err(PlanError::TooLong(41)));
    }

    #[test]
    fn urls_are_limited_to_safe_schemes() {
        for (url, ok) in [
            ("https://a.com", true),
            ("mailto:a@b.c", true),
            ("file:///C:/x", false),
            ("javascript:1", false),
            ("calc.exe", false),
        ] {
            let v = json!({"objective": "x", "steps": [{"id": "s1", "title": "t", "action": {"tool": "open_url", "url": url}}]});
            assert_eq!(parse_plan(&v.to_string()).is_ok(), ok, "{url}");
        }
    }

    #[test]
    fn ui_targets_and_waits_are_validated() {
        let v = json!({"objective": "x", "steps": [{"id": "s1", "title": "t", "action": {"tool": "ui_click", "target": {}}}]});
        assert!(matches!(
            parse_plan(&v.to_string()),
            Err(PlanError::BadStep(..))
        ));
        let v = json!({"objective": "x", "steps": [{"id": "s1", "title": "t", "action": {"tool": "ui_click", "target": {"name": "Save"}}}]});
        assert!(parse_plan(&v.to_string()).is_ok());
        let v = json!({"objective": "x", "steps": [{"id": "s1", "title": "t", "action": {"tool": "wait_for", "condition": {"check": "window_exists", "title": "A"}, "timeout_secs": 999}}]});
        assert!(matches!(
            parse_plan(&v.to_string()),
            Err(PlanError::BadStep(..))
        ));
    }

    #[test]
    fn the_schema_lists_every_tool_for_structured_output() {
        let s = plan_schema().to_string();
        for tool in [
            "open_app",
            "find_files",
            "move_to_trash",
            "ui_click",
            "wait_for",
            "press_keys",
        ] {
            assert!(s.contains(tool), "{tool}");
        }
    }

    #[test]
    fn descriptions_are_short() {
        let a = PlanAction::TypeText {
            text: "x".repeat(500),
        };
        assert!(a.describe().chars().count() < 100);
        assert_eq!(
            PlanAction::MoveToTrash {
                paths: vec!["a".into(), "b".into()]
            }
            .describe(),
            "Move 2 item(s) to the Recycle Bin"
        );
    }
}
