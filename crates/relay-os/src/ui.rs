//! Platform-neutral model of application UI controls (UI Automation on Windows, Accessibility on macOS).

use serde::Serialize;

use crate::types::Rect;

/// What to look for. All present fields must match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiSelector {
    /// Part of the window title.
    pub window: Option<String>,
    /// Accessible name: exact match preferred, otherwise contained (case-insensitive).
    pub name: Option<String>,
    /// Normalised control type (see [`role_name`]).
    pub role: Option<String>,
    pub automation_id: Option<String>,
}

impl UiSelector {
    pub fn is_empty(&self) -> bool {
        self.name.is_none() && self.role.is_none() && self.automation_id.is_none()
    }
}

/// One control as read from the accessibility tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UiElement {
    pub name: String,
    /// Normalised control type such as `button`, `edit`, `menu_item`.
    pub role: String,
    pub automation_id: String,
    pub rect: Rect,
    pub enabled: bool,
    /// Current value or text, when the control exposes one.
    pub value: Option<String>,
}

/// Map a Windows UI Automation control-type id to a stable lowercase role.
pub fn role_name(control_type: i32) -> &'static str {
    match control_type {
        50000 => "button",
        50001 => "calendar",
        50002 => "checkbox",
        50003 => "combobox",
        50004 => "edit",
        50005 => "link",
        50006 => "image",
        50007 => "list_item",
        50008 => "list",
        50009 => "menu",
        50010 => "menu_bar",
        50011 => "menu_item",
        50012 => "progress_bar",
        50013 => "radio_button",
        50014 => "scroll_bar",
        50015 => "slider",
        50016 => "spinner",
        50017 => "status_bar",
        50018 => "tab",
        50019 => "tab_item",
        50020 => "text",
        50021 => "tool_bar",
        50022 => "tool_tip",
        50023 => "tree",
        50024 => "tree_item",
        50026 => "group",
        50028 => "data_grid",
        50029 => "data_item",
        50030 => "document",
        50031 => "split_button",
        50032 => "window",
        50033 => "pane",
        50034 => "header",
        50035 => "header_item",
        50036 => "table",
        50037 => "title_bar",
        50038 => "separator",
        _ => "custom",
    }
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Whether an element satisfies a selector. Name matching is case-insensitive, exact or contained.
pub fn matches(sel: &UiSelector, el: &UiElement) -> bool {
    if let Some(id) = &sel.automation_id {
        if !el.automation_id.eq_ignore_ascii_case(id.trim()) {
            return false;
        }
    }
    if let Some(role) = &sel.role {
        if norm(role) != el.role {
            return false;
        }
    }
    if let Some(name) = &sel.name {
        let (want, have) = (name.trim().to_lowercase(), el.name.to_lowercase());
        if want.is_empty() || !(have == want || have.contains(&want)) {
            return false;
        }
    }
    true
}

/// Rank matches: exact name first, then enabled controls, then shorter names.
pub fn rank(sel: &UiSelector, mut found: Vec<UiElement>) -> Vec<UiElement> {
    let want = sel
        .name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    found.sort_by_key(|e| {
        (
            e.name.to_lowercase() != want,
            !e.enabled,
            e.name.chars().count(),
        )
    });
    found
}

/// One line per control for planners: `role "name" [id]`, indented by depth.
pub fn outline_line(depth: usize, el: &UiElement) -> String {
    let id = if el.automation_id.is_empty() {
        String::new()
    } else {
        format!(" [{}]", el.automation_id)
    };
    let name: String = el.name.chars().take(60).collect();
    format!(
        "{}{} \"{}\"{}",
        "  ".repeat(depth.min(8)),
        el.role,
        name,
        id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(name: &str, role: &str, id: &str, enabled: bool) -> UiElement {
        UiElement {
            name: name.into(),
            role: role.into(),
            automation_id: id.into(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            enabled,
            value: None,
        }
    }

    #[test]
    fn roles_are_normalised() {
        assert_eq!(role_name(50000), "button");
        assert_eq!(role_name(50011), "menu_item");
        assert_eq!(role_name(99999), "custom");
        assert_eq!(norm("Menu Item"), "menu_item");
    }

    #[test]
    fn selectors_match_by_every_field() {
        let save = el("Save As…", "button", "btnSave", true);
        let sel = |name: Option<&str>, role: Option<&str>, id: Option<&str>| UiSelector {
            window: None,
            name: name.map(str::to_string),
            role: role.map(str::to_string),
            automation_id: id.map(str::to_string),
        };
        assert!(
            matches(&sel(Some("save"), None, None), &save),
            "contained, case-insensitive"
        );
        assert!(matches(
            &sel(Some("Save As…"), Some("Button"), Some("BTNSAVE")),
            &save
        ));
        assert!(!matches(&sel(Some("Cancel"), None, None), &save));
        assert!(!matches(&sel(None, Some("edit"), None), &save));
        assert!(!matches(&sel(None, None, Some("other")), &save));
        assert!(
            !matches(&sel(Some("  "), None, None), &save),
            "an empty name never matches"
        );
        assert!(sel(None, None, None).is_empty());
    }

    #[test]
    fn ranking_prefers_exact_enabled_short() {
        let sel = UiSelector {
            name: Some("Save".into()),
            ..UiSelector::default()
        };
        let ranked = rank(
            &sel,
            vec![
                el("Save all files", "button", "", true),
                el("Save", "button", "", false),
                el("Save", "button", "", true),
            ],
        );
        assert_eq!((ranked[0].name.as_str(), ranked[0].enabled), ("Save", true));
        assert_eq!(
            (ranked[1].name.as_str(), ranked[1].enabled),
            ("Save", false)
        );
        assert_eq!(ranked[2].name, "Save all files");
    }

    #[test]
    fn outline_lines_are_compact() {
        let l = outline_line(2, &el("Name", "edit", "txtName", true));
        assert_eq!(l, "    edit \"Name\" [txtName]");
        assert_eq!(
            outline_line(0, &el("OK", "button", "", true)),
            "button \"OK\""
        );
    }
}
