//! One chat turn: `claude -p --output-format stream-json` (architecture §5).
//!
//! The prompt goes in on stdin. Each stdout line is one JSON event, turned into a
//! [`ChatEvent`] for the chat window. The session id from the first event is kept so
//! the next turn continues the same conversation with `--resume`.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{claude, mcp, paths, proc};

/// Added to Claude Code's system prompt for Relay chats.
pub const SYSTEM_PROMPT: &str =
    "You are running inside Relay, a desktop operator on the owner's Windows PC. \
For work in desktop apps, hand whole UI tasks to Relay's local operator with relay-screen's ui_task \
tool (one goal in plain words, with the app's name) and keep the deciding and writing for yourself. \
If it fails or no local model is downloaded, finish with the direct relay-screen tools, preferring \
UI Automation and OCR tools over raw clicks. For websites, prefer the relay-browser tools (the owner's browser, driven through \
its page structure) and connectors for services. Keep replies short and say what you did.";

#[derive(Debug, Clone, Default)]
pub struct TurnOptions {
    pub resume: Option<String>,
    pub model: String,
    pub effort: String,
    /// Added to the system prompt, e.g. why the browser lane can't open pages now.
    pub note: Option<String>,
}

/// The `claude` arguments for a turn (the prompt itself goes on stdin).
pub fn args(opts: &TurnOptions) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        // Text arrives as it is written, so the chat shows it right away.
        "--include-partial-messages",
        "--permission-prompt-tool",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(format!("mcp__{}__approve", mcp::RELAY_SCREEN));
    args.push("--append-system-prompt".into());
    args.push(match &opts.note {
        Some(note) => format!("{SYSTEM_PROMPT} {note}"),
        None => SYSTEM_PROMPT.into(),
    });
    if let Some(id) = &opts.resume {
        args.push("--resume".into());
        args.push(id.clone());
    }
    if !opts.model.is_empty() {
        args.push("--model".into());
        args.push(opts.model.clone());
    }
    if !opts.effort.is_empty() {
        args.push("--effort".into());
        args.push(opts.effort.clone());
    }
    args
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Started {
        session_id: String,
        model: String,
        mcp_servers: Vec<String>,
    },
    Text {
        text: String,
    },
    /// Part of the text Claude is writing now; the full `Text` follows.
    TextDelta {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        text: String,
        /// A screenshot the tool returned, as a data URL.
        image: Option<String>,
    },
    Done {
        is_error: bool,
        result: String,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
    },
    Error {
        message: String,
    },
    /// Claude couldn't run, so the local operator takes the task (D-067).
    Fallback {
        reason: String,
    },
    /// Progress from the local operator.
    LocalNote {
        text: String,
    },
    LocalStep {
        step: u64,
        thought: String,
        action: String,
        result: String,
    },
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut t: String = text.chars().take(max).collect();
        t.push('…');
        t
    }
}

/// Turn one stream-json line into zero or more chat events.
pub fn parse_line(line: &str) -> Vec<ChatEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![];
    };
    let s = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match v.get("type").and_then(Value::as_str) {
        Some("system") if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            let servers = v
                .get("mcp_servers")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|m| s(m, "name")).collect())
                .unwrap_or_default();
            vec![ChatEvent::Started {
                session_id: s(&v, "session_id"),
                model: s(&v, "model"),
                mcp_servers: servers,
            }]
        }
        Some("assistant") => content(&v)
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => Some(ChatEvent::Text {
                    text: s(block, "text"),
                }),
                Some("tool_use") => Some(ChatEvent::ToolUse {
                    id: s(block, "id"),
                    name: s(block, "name"),
                    input: truncate(
                        &block.get("input").map(Value::to_string).unwrap_or_default(),
                        400,
                    ),
                }),
                _ => None,
            })
            .collect(),
        Some("user") => content(&v)
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .map(|block| {
                let (text, image) = tool_result_body(block.get("content"));
                ChatEvent::ToolResult {
                    tool_use_id: s(block, "tool_use_id"),
                    is_error: block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    text: truncate(&text, 1200),
                    image,
                }
            })
            .collect(),
        // Subagents stream too; only Claude's own reply goes to the chat.
        Some("stream_event") if v["parent_tool_use_id"].is_null() => {
            let e = &v["event"];
            match (e["type"].as_str(), e["delta"]["type"].as_str()) {
                (Some("content_block_delta"), Some("text_delta")) => vec![ChatEvent::TextDelta {
                    text: s(&e["delta"], "text"),
                }],
                _ => vec![],
            }
        }
        Some("result") => vec![ChatEvent::Done {
            is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
            result: s(&v, "result"),
            cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
            duration_ms: v.get("duration_ms").and_then(Value::as_u64),
        }],
        _ => vec![],
    }
}

fn content(v: &Value) -> Vec<Value> {
    v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn tool_result_body(content: Option<&Value>) -> (String, Option<String>) {
    match content {
        Some(Value::String(text)) => (text.clone(), None),
        Some(Value::Array(blocks)) => {
            let mut text = String::new();
            let mut image = None;
            for b in blocks {
                match b.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(b.get("text").and_then(Value::as_str).unwrap_or_default());
                    }
                    Some("image") if image.is_none() => {
                        let src = b.get("source");
                        let media = src
                            .and_then(|s| s.get("media_type"))
                            .and_then(Value::as_str)
                            .unwrap_or("image/png");
                        if let Some(data) = src.and_then(|s| s.get("data")).and_then(Value::as_str)
                        {
                            image = Some(format!("data:{media};base64,{data}"));
                        }
                    }
                    _ => {}
                }
            }
            (text, image)
        }
        _ => (String::new(), None),
    }
}

/// A running turn. Dropping it does not stop Claude; call [`Turn::stop`].
pub struct Turn {
    child: tokio::process::Child,
}

impl Turn {
    pub fn new(child: tokio::process::Child) -> Self {
        Self { child }
    }

    pub async fn stop(&mut self) {
        let _ = self.child.kill().await;
    }
}

/// Start a turn. Events go to `on_event` until the process ends; the returned
/// handle can stop it early.
pub async fn start(
    prompt: &str,
    opts: &TurnOptions,
    on_event: impl Fn(ChatEvent) + Send + Sync + 'static,
) -> Result<(Turn, tokio::task::JoinHandle<()>)> {
    let claude: PathBuf = claude::find().context("Claude Code is not installed")?;
    let workdir = paths::work_dir();
    std::fs::create_dir_all(&workdir)?;
    let mut cmd = proc::command(&claude);
    cmd.current_dir(&workdir)
        .args(args(opts))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    let mut child = cmd.spawn().context("could not start claude")?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin.write_all(prompt.as_bytes()).await?;
    drop(stdin);
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let task = tokio::spawn(async move {
        let err_task = tokio::spawn(async move {
            let mut all = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                all.push_str(&line);
                all.push('\n');
            }
            all
        });
        let mut saw_done = false;
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            for event in parse_line(&line) {
                saw_done |= matches!(event, ChatEvent::Done { .. });
                on_event(event);
            }
        }
        let stderr = err_task.await.unwrap_or_default();
        if !saw_done {
            let message = if stderr.trim().is_empty() {
                "Claude stopped before finishing.".to_string()
            } else {
                truncate(stderr.trim(), 800)
            };
            on_event(ChatEvent::Error { message });
        }
    });
    Ok((Turn { child }, task))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_args() {
        let a = args(&TurnOptions {
            resume: Some("abc".into()),
            model: "opus".into(),
            effort: String::new(),
            note: Some("Lane note.".into()),
        });
        assert_eq!(a[0], "-p");
        assert!(a.contains(&"mcp__relay-screen__approve".to_string()));
        assert!(a
            .iter()
            .any(|x| x.starts_with("You are running inside Relay") && x.ends_with("Lane note.")));
        let i = a.iter().position(|x| x == "--resume").unwrap();
        assert_eq!(a[i + 1], "abc");
        assert!(a.contains(&"--model".to_string()));
        assert!(!a.contains(&"--effort".to_string()));
    }

    #[test]
    fn streams_text_deltas() {
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Open"}},"session_id":"s1"}"#;
        assert_eq!(
            parse_line(delta),
            vec![ChatEvent::TextDelta {
                text: "Open".into()
            }]
        );
        let start = r#"{"type":"stream_event","event":{"type":"message_start","message":{}}}"#;
        assert!(parse_line(start).is_empty());
        assert!(args(&TurnOptions::default()).contains(&"--include-partial-messages".to_string()));
    }

    #[test]
    fn parses_stream_json() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-x","mcp_servers":[{"name":"relay-screen","status":"connected"}]}"#;
        assert_eq!(
            parse_line(init),
            vec![ChatEvent::Started {
                session_id: "s1".into(),
                model: "claude-x".into(),
                mcp_servers: vec!["relay-screen".into()],
            }]
        );
        let assistant = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Opening Notepad"},{"type":"tool_use","id":"t1","name":"mcp__relay-screen__launch_app","input":{"name":"Notepad"}}]}}"#;
        let ev = parse_line(assistant);
        assert_eq!(ev.len(), 2);
        assert_eq!(
            ev[0],
            ChatEvent::Text {
                text: "Opening Notepad".into()
            }
        );
        assert!(matches!(&ev[1], ChatEvent::ToolUse { name, .. } if name.ends_with("launch_app")));
        let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"ok"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}]}}"#;
        match &parse_line(user)[0] {
            ChatEvent::ToolResult {
                text,
                image,
                is_error,
                ..
            } => {
                assert_eq!(text, "ok");
                assert_eq!(image.as_deref(), Some("data:image/png;base64,AAAA"));
                assert!(!is_error);
            }
            other => panic!("unexpected {other:?}"),
        }
        let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.01,"duration_ms":1200}"#;
        assert_eq!(
            parse_line(result),
            vec![ChatEvent::Done {
                is_error: false,
                result: "Done.".into(),
                cost_usd: Some(0.01),
                duration_ms: Some(1200),
            }]
        );
        assert!(parse_line("not json").is_empty());
    }
}
