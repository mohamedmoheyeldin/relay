//! Parser for Claude Code's `stream-json` output.

use serde_json::Value;

use crate::process::StreamProtocol;
use crate::{AiError, AiEvent, Finish};

/// Decide whether an error message means the user is not signed in.
pub fn classify_error(message: &str, exit_code: Option<i32>) -> AiError {
    let lower = message.to_lowercase();
    const AUTH_MARKERS: [&str; 7] = [
        "not logged in",
        "please run /login",
        "invalid api key",
        "authentication_error",
        "failed to authenticate",
        "oauth token has expired",
        "not signed in",
    ];
    if AUTH_MARKERS.iter().any(|m| lower.contains(m)) {
        AiError::NotLoggedIn(message.trim().to_string())
    } else {
        AiError::Cli {
            message: message.trim().to_string(),
            exit_code,
        }
    }
}

#[derive(Default)]
pub struct ClaudeProtocol {
    session_id: Option<String>,
    last_text: String,
}

impl ClaudeProtocol {
    pub fn new() -> Self {
        Self::default()
    }
}

fn tool_result_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item.get("type").and_then(Value::as_str) {
                Some("text") => item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                Some("image") => "[image]".to_string(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl StreamProtocol for ClaudeProtocol {
    fn on_line(&mut self, line: &str) -> Vec<AiEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        // Output from sub-agents is not shown in the chat.
        if value
            .get("parent_tool_use_id")
            .is_some_and(|p| !p.is_null())
        {
            return Vec::new();
        }
        let kind = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "system" => {
                if value.get("subtype").and_then(Value::as_str) == Some("init") {
                    let session_id = value
                        .get("session_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    self.session_id = Some(session_id.clone());
                    return vec![AiEvent::Started {
                        session_id,
                        model: value
                            .get("model")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    }];
                }
                Vec::new()
            }
            "stream_event" => {
                let event = &value["event"];
                if event.get("type").and_then(Value::as_str) == Some("content_block_delta")
                    && event["delta"].get("type").and_then(Value::as_str) == Some("text_delta")
                {
                    if let Some(text) = event["delta"].get("text").and_then(Value::as_str) {
                        return vec![AiEvent::TextDelta(text.to_string())];
                    }
                }
                Vec::new()
            }
            "assistant" => {
                let mut out = Vec::new();
                let blocks = value["message"]["content"].as_array();
                for block in blocks.into_iter().flatten() {
                    match block.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                            if !text.is_empty() {
                                self.last_text = text.to_string();
                                out.push(AiEvent::Text(text.to_string()));
                            }
                        }
                        Some("tool_use") => out.push(AiEvent::ToolUse {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            input: block.get("input").cloned().unwrap_or(Value::Null),
                        }),
                        _ => {}
                    }
                }
                out
            }
            "user" => {
                let mut out = Vec::new();
                let blocks = value["message"]["content"].as_array();
                for block in blocks.into_iter().flatten() {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        out.push(AiEvent::ToolResult {
                            id: block
                                .get("tool_use_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            is_error: block
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            content: tool_result_text(block.get("content").unwrap_or(&Value::Null)),
                        });
                    }
                }
                out
            }
            "result" => {
                let is_error = value
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    || value
                        .get("subtype")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s.starts_with("error"));
                let mut result = value
                    .get("result")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                // With --json-schema the validated answer is in `structured_output`.
                if let Some(structured) = value.get("structured_output").filter(|s| !s.is_null()) {
                    result = structured.to_string();
                }
                let session_id = value
                    .get("session_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.session_id.clone());
                if is_error {
                    let message = if result.is_empty() {
                        value
                            .get("subtype")
                            .and_then(Value::as_str)
                            .unwrap_or("the AI tool reported an error")
                            .to_string()
                    } else {
                        result
                    };
                    vec![AiEvent::Finished(Finish::Failed(classify_error(
                        &message, None,
                    )))]
                } else {
                    vec![AiEvent::Finished(Finish::Completed {
                        session_id,
                        result,
                        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
                    })]
                }
            }
            _ => Vec::new(),
        }
    }

    fn on_exit(&mut self, exit_code: Option<i32>, stderr: &str) -> Finish {
        if exit_code == Some(0) {
            return Finish::Completed {
                session_id: self.session_id.clone(),
                result: self.last_text.clone(),
                cost_usd: None,
            };
        }
        let detail = stderr.trim();
        let message = if detail.is_empty() {
            match exit_code {
                Some(code) => format!("the AI tool exited with code {code} and no output"),
                None => "the AI tool was terminated".to_string(),
            }
        } else {
            detail.to_string()
        };
        Finish::Failed(classify_error(&message, exit_code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> (Vec<AiEvent>, ClaudeProtocol) {
        let mut p = ClaudeProtocol::new();
        let mut out = Vec::new();
        for l in lines {
            out.extend(p.on_line(l));
        }
        (out, p)
    }

    #[test]
    fn init_event_starts_the_session() {
        let (ev, _) = feed(&[
            r#"{"type":"system","subtype":"init","session_id":"s1","model":"m","tools":[]}"#,
        ]);
        assert_eq!(
            ev,
            vec![AiEvent::Started {
                session_id: "s1".into(),
                model: Some("m".into())
            }]
        );
    }

    #[test]
    fn other_system_events_are_ignored() {
        let (ev, _) = feed(&[
            r#"{"type":"system","subtype":"hook_started","session_id":"s"}"#,
            r#"{"type":"rate_limit_event"}"#,
            "not json at all",
            "",
        ]);
        assert!(ev.is_empty());
    }

    #[test]
    fn text_deltas_stream_and_assistant_text_completes() {
        let (ev, _) = feed(&[
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"po"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"pong"}]}}"#,
        ]);
        assert_eq!(
            ev,
            vec![
                AiEvent::TextDelta("po".into()),
                AiEvent::Text("pong".into())
            ]
        );
    }

    #[test]
    fn tool_use_and_result_are_paired_by_id() {
        let (ev, _) = feed(&[
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__relay__click","input":{"x":1}}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"OK"},{"type":"image","source":{}}]}]}}"#,
        ]);
        assert_eq!(
            ev[0],
            AiEvent::ToolUse {
                id: "t1".into(),
                name: "mcp__relay__click".into(),
                input: serde_json::json!({"x":1})
            }
        );
        assert_eq!(
            ev[1],
            AiEvent::ToolResult {
                id: "t1".into(),
                is_error: false,
                content: "OK\n[image]".into()
            }
        );
    }

    #[test]
    fn string_tool_results_and_error_flag() {
        let (ev, _) = feed(&[
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"denied"}]}}"#,
        ]);
        assert_eq!(
            ev[0],
            AiEvent::ToolResult {
                id: "t2".into(),
                is_error: true,
                content: "denied".into()
            }
        );
    }

    #[test]
    fn subagent_output_is_hidden() {
        let (ev, _) = feed(&[
            r#"{"type":"assistant","parent_tool_use_id":"abc","message":{"content":[{"type":"text","text":"inner"}]}}"#,
        ]);
        assert!(ev.is_empty());
    }

    #[test]
    fn successful_result_completes_with_cost() {
        let (ev, _) = feed(&[
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"s9","total_cost_usd":0.5}"#,
        ]);
        assert_eq!(
            ev,
            vec![AiEvent::Finished(Finish::Completed {
                session_id: Some("s9".into()),
                result: "done".into(),
                cost_usd: Some(0.5)
            })]
        );
    }

    #[test]
    fn login_failures_are_classified() {
        let (ev, _) = feed(&[
            r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#,
        ]);
        assert!(matches!(
            &ev[0],
            AiEvent::Finished(Finish::Failed(AiError::NotLoggedIn(_)))
        ));
    }

    #[test]
    fn other_errors_keep_their_message() {
        let (ev, _) = feed(&[
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":""}"#,
        ]);
        match &ev[0] {
            AiEvent::Finished(Finish::Failed(AiError::Cli { message, .. })) => {
                assert!(message.contains("error_during_execution"))
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn exit_without_result_uses_stderr() {
        let mut p = ClaudeProtocol::new();
        match p.on_exit(Some(1), "boom") {
            Finish::Failed(AiError::Cli { message, exit_code }) => {
                assert_eq!(message, "boom");
                assert_eq!(exit_code, Some(1));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            p.on_exit(Some(1), "Error: Not logged in"),
            Finish::Failed(AiError::NotLoggedIn(_))
        ));
        assert!(matches!(p.on_exit(Some(0), ""), Finish::Completed { .. }));
    }
}
