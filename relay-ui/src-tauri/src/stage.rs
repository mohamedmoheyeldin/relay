//! Keeps Relay's window out of the way while Claude or the local operator works on the
//! screen: minimized when the first on-screen step starts, back when the run ends. Only
//! a window Relay itself minimized is brought back.

use std::sync::atomic::{AtomicBool, Ordering};

use relay_core::chat::ChatEvent;
use tauri::{AppHandle, Manager};

static ASIDE: AtomicBool = AtomicBool::new(false);

/// relay-screen tools that only report, so Relay's window can stay.
const QUIET: [&str; 6] = [
    "ping",
    "safety_status",
    "list_displays",
    "list_apps",
    "list_windows",
    "approve",
];

/// Whether this event means a run is about to look at or act on the screen.
pub fn acts_on_screen(event: &ChatEvent) -> bool {
    match event {
        ChatEvent::ToolUse { name, .. } => name
            .strip_prefix("mcp__relay-screen__")
            .is_some_and(|tool| !QUIET.contains(&tool)),
        ChatEvent::Fallback { .. } | ChatEvent::LocalStep { .. } => true,
        _ => false,
    }
}

/// Called for every chat event; `None` means the turn finished.
pub fn on_chat(app: &AppHandle, event: Option<&ChatEvent>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    match event {
        Some(event) if acts_on_screen(event) => {
            let showing =
                window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true);
            if showing && !ASIDE.swap(true, Ordering::SeqCst) {
                let _ = window.minimize();
            }
        }
        None if ASIDE.swap(false, Ordering::SeqCst) => {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        _ => {}
    }
}

/// After an approval card: step aside again if the run had put Relay away.
pub fn after_approval(app: &AppHandle) {
    if ASIDE.load(Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.minimize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str) -> ChatEvent {
        ChatEvent::ToolUse {
            id: "t".into(),
            name: name.into(),
            input: "{}".into(),
        }
    }

    #[test]
    fn steps_on_the_screen_put_relay_aside() {
        assert!(acts_on_screen(&tool("mcp__relay-screen__screenshot")));
        assert!(acts_on_screen(&tool("mcp__relay-screen__ui_task")));
        assert!(!acts_on_screen(&tool("mcp__relay-screen__safety_status")));
        assert!(!acts_on_screen(&tool("mcp__claude_ai_Dice__search_jobs")));
        assert!(!acts_on_screen(&ChatEvent::Text { text: "hi".into() }));
    }
}
