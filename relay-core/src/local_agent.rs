//! The local operator (D-067): when Claude can't run, the chat hands the task to
//! `relay-screen agent --json`, which drives the PC with the local model alone,
//! through the same approval prompt and kill switch Claude's actions use.

use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::chat::{ChatEvent, Turn};
use crate::settings::Settings;
use crate::{local_ai, mcp, paths, proc, system};

/// Claude's words when it can't run at all: signed out, sign-in expired, or offline.
/// Only then does the chat fall back to the local operator.
pub fn claude_unavailable(text: &str) -> bool {
    let t = text.to_lowercase();
    [
        "failed to authenticate",
        "oauth session expired",
        "could not be refreshed",
        "not logged in",
        "please run /login",
        "invalid api key",
        "authentication_error",
        "connection error",
        "unable to connect",
        "fetch failed",
        "getaddrinfo",
        "enotfound",
        "econnrefused",
        "econnreset",
        "etimedout",
        "network error",
    ]
    .iter()
    .any(|p| t.contains(p))
}

/// One JSON line from `relay-screen agent --json`, as chat events.
pub fn parse_line(line: &str) -> Vec<ChatEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let s = |k: &str| match v.get(k) {
        Some(Value::String(t)) => t.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    let steps = v.get("steps").and_then(Value::as_u64).unwrap_or(0);
    match v.get("event").and_then(Value::as_str) {
        Some("loading") => vec![ChatEvent::LocalNote {
            text: format!("Loading the local model {}…", s("model")),
        }],
        // The operator service's own start-up line has no model; it isn't news.
        Some("ready") if s("model").is_empty() => vec![],
        Some("ready") => vec![ChatEvent::LocalNote {
            text: match v.get("seconds").and_then(Value::as_f64) {
                Some(secs) => format!("{} is ready ({secs:.1} s).", s("model")),
                None => format!("{} is ready.", s("model")),
            },
        }],
        Some("step") => vec![ChatEvent::LocalStep {
            step: v.get("step").and_then(Value::as_u64).unwrap_or(0),
            // `say` is the step in plain words; older versions only had `thought`.
            thought: Some(s("say"))
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| s("thought")),
            action: s("action"),
            result: s("result"),
        }],
        Some("bad_reply") => vec![ChatEvent::LocalNote {
            text: format!(
                "The local model gave an unusable reply ({}); asking again.",
                s("error")
            ),
        }],
        Some("done") => vec![ChatEvent::Done {
            is_error: false,
            result: with_steps(&s("message"), steps),
            cost_usd: None,
            duration_ms: None,
        }],
        Some(kind @ ("failed" | "stopped")) => vec![ChatEvent::Done {
            is_error: true,
            result: if s("message").is_empty() {
                format!("The local AI {kind}.")
            } else {
                with_steps(&s("message"), steps)
            },
            cost_usd: None,
            duration_ms: None,
        }],
        Some("unavailable") => {
            let why = s("message");
            // Drop the command-line hint meant for terminal users.
            let why = why
                .split(" (relay-screen")
                .next()
                .unwrap_or_default()
                .trim_end();
            let mut message = format!("The local AI can't run yet. {why}");
            if !message.contains("Settings > Local AI") {
                message.push_str(
                    " Relay is downloading its model; you can follow it in Settings > Local AI.",
                );
            }
            vec![ChatEvent::Error { message }]
        }
        _ => vec![],
    }
}

fn with_steps(message: &str, steps: u64) -> String {
    match steps {
        0 => message.to_string(),
        1 => format!("{message} (1 step)"),
        n => format!("{message} ({n} steps)"),
    }
}

/// Start the local operator on `task`. Events go to `on_event` until it ends; the
/// returned handle stops it (the kill switch still works on its own).
pub async fn start(
    python: &Path,
    settings: &Settings,
    task: &str,
    on_event: impl Fn(ChatEvent) + Send + Sync + 'static,
) -> Result<(Turn, tokio::task::JoinHandle<()>)> {
    let mut cmd = proc::command(local_ai::cli_python(python));
    if let mcp::ServerSpec::Stdio { env, .. } = system::relay_screen_spec(python, settings) {
        cmd.envs(env);
    }
    let workdir = paths::work_dir();
    std::fs::create_dir_all(&workdir)?;
    cmd.current_dir(&workdir)
        .args(["-m", "relay_screen.server", "agent", "--json", "--"])
        .arg(task)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    let mut child = cmd.spawn().context("could not start the local AI")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let task = tokio::spawn(async move {
        let err_task = tokio::spawn(async move {
            let mut last = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    last = line;
                }
            }
            last
        });
        let mut finished = false;
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            for event in parse_line(&line) {
                finished |= matches!(event, ChatEvent::Done { .. } | ChatEvent::Error { .. });
                on_event(event);
            }
        }
        let last = err_task.await.unwrap_or_default();
        if !finished {
            let message = if last.trim().is_empty() {
                "The local AI stopped before finishing.".to_string()
            } else {
                format!("The local AI stopped: {}", last.trim())
            };
            on_event(ChatEvent::Error { message });
        }
    });
    Ok((Turn::new(child), task))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_when_claude_cannot_run() {
        assert!(claude_unavailable(
            "Failed to authenticate: OAuth session expired and could not be refreshed"
        ));
        assert!(claude_unavailable("API Error: Connection error."));
        assert!(!claude_unavailable("I opened Notepad."));
    }

    #[test]
    fn agent_lines_become_chat_events() {
        let step = parse_line(
            r#"{"event":"step","step":2,"thought":"Notepad is open","action":"type \"hi\"","result":"OK"}"#,
        );
        assert_eq!(
            step,
            vec![ChatEvent::LocalStep {
                step: 2,
                thought: "Notepad is open".into(),
                action: "type \"hi\"".into(),
                result: "OK".into(),
            }]
        );
        let done = parse_line(r#"{"event":"done","message":"Typed hi.","steps":3}"#);
        assert!(
            matches!(&done[0], ChatEvent::Done { is_error: false, result, .. } if result == "Typed hi. (3 steps)")
        );
        let stopped = parse_line(r#"{"event":"stopped","message":"","steps":1}"#);
        assert!(
            matches!(&stopped[0], ChatEvent::Done { is_error: true, result, .. } if result == "The local AI stopped.")
        );
        let none =
            parse_line(r#"{"event":"unavailable","message":"No local model is downloaded."}"#);
        assert!(
            matches!(&none[0], ChatEvent::Error { message } if message.contains("No local model"))
        );
        assert!(parse_line("not json").is_empty());
        let ready = parse_line(r#"{"event":"ready","model":"qwen3-vl-8b-instruct","seconds":4.2}"#);
        assert!(matches!(&ready[0], ChatEvent::LocalNote { text } if text.contains("4.2")));
        assert!(parse_line(r#"{"event":"ready"}"#).is_empty());
    }
}
