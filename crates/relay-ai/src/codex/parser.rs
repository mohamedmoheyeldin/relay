//! Parser for `codex exec --json` (one JSON event per line).
//!
//! The stream is item-level: messages arrive whole, there are no token deltas. Unknown event
//! and item types are ignored so newer CLI versions keep working.

use serde_json::Value;

use crate::claude::classify_error;
use crate::process::StreamProtocol;
use crate::{AiEvent, Finish};

#[derive(Default)]
pub struct CodexProtocol {
    thread_id: Option<String>,
    last_message: String,
    failure: Option<String>,
}

impl CodexProtocol {
    pub fn new() -> Self {
        Self::default()
    }
}

fn text_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn mcp_result_text(item: &Value) -> String {
    if let Some(err) = item
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
    {
        return err.to_string();
    }
    item.get("result")
        .and_then(|r| r.get("content"))
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

impl StreamProtocol for CodexProtocol {
    fn on_line(&mut self, line: &str) -> Vec<AiEvent> {
        let line = line.trim();
        if !line.starts_with('{') {
            return Vec::new();
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "thread.started" => {
                let id = text_of(&event, "thread_id");
                self.thread_id = Some(id.clone());
                vec![AiEvent::Started {
                    session_id: id,
                    model: None,
                }]
            }
            "item.started" | "item.completed" => {
                let item = &event["item"];
                let completed = kind == "item.completed";
                let id = text_of(item, "id");
                match item.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "agent_message" if completed => {
                        let text = text_of(item, "text");
                        if text.is_empty() {
                            Vec::new()
                        } else {
                            self.last_message = text.clone();
                            vec![AiEvent::Text(text)]
                        }
                    }
                    "command_execution" => {
                        if completed {
                            let output = text_of(item, "aggregated_output");
                            let failed = item
                                .get("exit_code")
                                .and_then(Value::as_i64)
                                .is_some_and(|c| c != 0)
                                || text_of(item, "status") == "declined";
                            vec![AiEvent::ToolResult {
                                id,
                                is_error: failed,
                                content: output,
                            }]
                        } else {
                            vec![AiEvent::ToolUse {
                                id,
                                name: "command".into(),
                                input: serde_json::json!({ "command": text_of(item, "command") }),
                            }]
                        }
                    }
                    "mcp_tool_call" => {
                        if completed {
                            let failed = text_of(item, "status") == "failed";
                            vec![AiEvent::ToolResult {
                                id,
                                is_error: failed,
                                content: mcp_result_text(item),
                            }]
                        } else {
                            vec![AiEvent::ToolUse {
                                id,
                                name: format!(
                                    "mcp__{}__{}",
                                    text_of(item, "server"),
                                    text_of(item, "tool")
                                ),
                                input: item.get("arguments").cloned().unwrap_or(Value::Null),
                            }]
                        }
                    }
                    "error" if completed => {
                        self.failure = Some(text_of(item, "message"));
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            "turn.completed" => {
                if let Some(msg) = self.failure.take().filter(|_| self.last_message.is_empty()) {
                    return vec![AiEvent::Finished(Finish::Failed(classify_error(
                        &msg, None,
                    )))];
                }
                vec![AiEvent::Finished(Finish::Completed {
                    session_id: self.thread_id.clone(),
                    result: self.last_message.clone(),
                    cost_usd: None,
                })]
            }
            "turn.failed" => {
                let message = event
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("the turn failed")
                    .to_string();
                vec![AiEvent::Finished(Finish::Failed(classify_error(
                    &message, None,
                )))]
            }
            "error" => {
                // Reconnect notices also arrive as errors; only remember the last one.
                self.failure = Some(text_of(&event, "message"));
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_exit(&mut self, exit_code: Option<i32>, stderr: &str) -> Finish {
        if exit_code == Some(0) && !self.last_message.is_empty() {
            return Finish::Completed {
                session_id: self.thread_id.clone(),
                result: self.last_message.clone(),
                cost_usd: None,
            };
        }
        let detail = self
            .failure
            .clone()
            .filter(|m| !m.is_empty())
            .or_else(|| (!stderr.trim().is_empty()).then(|| stderr.trim().to_string()))
            .unwrap_or_else(|| match exit_code {
                Some(c) => format!("Codex exited with code {c} and no answer"),
                None => "Codex was terminated".to_string(),
            });
        Finish::Failed(classify_error(&detail, exit_code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AiError;

    fn feed(lines: &[&str]) -> Vec<AiEvent> {
        let mut p = CodexProtocol::new();
        lines.iter().flat_map(|l| p.on_line(l)).collect()
    }

    // Lines quoted from the official non-interactive docs where available.
    const STARTED: &str =
        r#"{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}"#;
    const TURN_STARTED: &str = r#"{"type":"turn.started"}"#;
    const CMD_STARTED: &str = r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}"#;
    const COMPLETED: &str = r#"{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":0}}"#;

    #[test]
    fn a_normal_turn() {
        let ev = feed(&[
            STARTED,
            TURN_STARTED,
            CMD_STARTED,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","aggregated_output":"README.md\n","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"Done."}}"#,
            COMPLETED,
        ]);
        assert_eq!(
            ev[0],
            AiEvent::Started {
                session_id: "0199a213-81c0-7800-8aa1-bbab2a035a53".into(),
                model: None
            }
        );
        assert!(matches!(&ev[1], AiEvent::ToolUse { name, .. } if name == "command"));
        assert!(
            matches!(&ev[2], AiEvent::ToolResult { is_error: false, content, .. } if content == "README.md\n")
        );
        assert_eq!(ev[3], AiEvent::Text("Done.".into()));
        assert_eq!(
            ev[4],
            AiEvent::Finished(Finish::Completed {
                session_id: Some("0199a213-81c0-7800-8aa1-bbab2a035a53".into()),
                result: "Done.".into(),
                cost_usd: None
            })
        );
    }

    #[test]
    fn failed_and_declined_commands_are_marked() {
        let ev = feed(&[
            r#"{"type":"item.completed","item":{"id":"a","type":"command_execution","command":"x","aggregated_output":"boom","exit_code":2,"status":"failed"}}"#,
            r#"{"type":"item.completed","item":{"id":"b","type":"command_execution","command":"y","aggregated_output":"","exit_code":null,"status":"declined"}}"#,
        ]);
        assert!(ev
            .iter()
            .all(|e| matches!(e, AiEvent::ToolResult { is_error: true, .. })));
    }

    #[test]
    fn mcp_calls_are_named_like_the_other_provider() {
        let ev = feed(&[
            r#"{"type":"item.started","item":{"id":"m","type":"mcp_tool_call","server":"relay","tool":"screenshot","arguments":{},"status":"in_progress"}}"#,
            r#"{"type":"item.completed","item":{"id":"m","type":"mcp_tool_call","server":"relay","tool":"screenshot","arguments":{},"result":{"content":[{"type":"text","text":"OK"}]},"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"n","type":"mcp_tool_call","server":"relay","tool":"x","error":{"message":"denied"},"status":"failed"}}"#,
        ]);
        assert!(
            matches!(&ev[0], AiEvent::ToolUse { name, .. } if name == "mcp__relay__screenshot")
        );
        assert!(
            matches!(&ev[1], AiEvent::ToolResult { content, is_error: false, .. } if content == "OK")
        );
        assert!(
            matches!(&ev[2], AiEvent::ToolResult { content, is_error: true, .. } if content == "denied")
        );
    }

    #[test]
    fn failures_and_login_problems_are_classified() {
        let ev = feed(&[
            r#"{"type":"turn.failed","error":{"message":"Not logged in. Run codex login"}}"#,
        ]);
        assert!(matches!(
            &ev[0],
            AiEvent::Finished(Finish::Failed(AiError::NotLoggedIn(_)))
        ));
        let ev = feed(&[r#"{"type":"turn.failed","error":{"message":"stream disconnected"}}"#]);
        assert!(matches!(
            &ev[0],
            AiEvent::Finished(Finish::Failed(AiError::Cli { .. }))
        ));
    }

    #[test]
    fn an_error_event_before_an_empty_turn_is_reported() {
        let ev = feed(&[r#"{"type":"error","message":"model not found"}"#, COMPLETED]);
        match &ev[0] {
            AiEvent::Finished(Finish::Failed(AiError::Cli { message, .. })) => {
                assert_eq!(message, "model not found")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn noise_and_unknown_events_are_ignored() {
        let ev = feed(&[
            "WARN something on stdout",
            "",
            r#"{"type":"item.updated","item":{"id":"t","type":"todo_list","items":[]}}"#,
            r#"{"type":"future.event","x":1}"#,
            r#"{"type":"item.completed","item":{"id":"w","type":"web_search","query":"q"}}"#,
        ]);
        assert!(ev.is_empty());
    }

    #[test]
    fn exit_without_events_uses_stderr_or_code() {
        let mut p = CodexProtocol::new();
        assert!(matches!(
            p.on_exit(Some(1), "Error: not logged in"),
            Finish::Failed(AiError::NotLoggedIn(_))
        ));
        assert!(matches!(
            p.on_exit(Some(3), ""),
            Finish::Failed(AiError::Cli {
                exit_code: Some(3),
                ..
            })
        ));
        let mut q = CodexProtocol::new();
        q.on_line(
            r#"{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"hi"}}"#,
        );
        assert!(
            matches!(q.on_exit(Some(0), ""), Finish::Completed { ref result, .. } if result == "hi")
        );
    }
}
