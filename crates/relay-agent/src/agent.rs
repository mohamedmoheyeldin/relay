//! The orchestrator: one task at a time, from the user's message to the finished run.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use relay_ai::{
    AiError, AiEvent, AiProvider, Finish, RunRequest, SessionMode, TimeoutKind, Timeouts,
};
use relay_mcp::actions::parse_action;
use relay_mcp::safety::truncate;
use relay_mcp::{KillSwitch, McpServer, PERMISSION_TOOL, SERVER_NAME};
use relay_protocol::{ConfirmAnswer, ErrorKind, RunId, RunOutcome, StatusInfo, UiEvent};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::bridge::{ConfirmBridge, EventSink};
use crate::prompt::SYSTEM_PROMPT;

/// Settings for the agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Working directory handed to the AI CLI.
    pub work_dir: PathBuf,
    /// Optional `{"mcpServers": {...}}` file with the owner's own MCP servers (Gmail, GitHub, ...).
    pub user_mcp_config: Option<PathBuf>,
    pub timeouts: Timeouts,
    pub model: Option<String>,
    /// Shown in the UI so the owner knows how to stop the agent.
    pub kill_hotkey: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubmitError {
    #[error("Relay is already working on a task. Stop it first or wait for it to finish.")]
    Busy,
    #[error("There is nothing to do: the message is empty.")]
    Empty,
}

struct Active {
    stop: CancellationToken,
}

#[derive(Default)]
struct State {
    active: Option<Active>,
    session_id: Option<String>,
    next_run: RunId,
}

pub struct Agent {
    provider: Arc<dyn AiProvider>,
    mcp: Arc<McpServer>,
    kill: KillSwitch,
    bridge: Arc<ConfirmBridge>,
    config: AgentConfig,
    sink: EventSink,
    state: Mutex<State>,
}

/// Merge the owner's MCP servers with Relay's own; Relay's entry always wins.
pub fn merge_servers(relay: Value, user: Map<String, Value>) -> Value {
    let mut merged = user;
    merged.remove(SERVER_NAME);
    if let Value::Object(own) = relay {
        merged.extend(own);
    }
    Value::Object(merged)
}

/// Read the owner's MCP server map. A missing file is fine; a broken one is reported.
pub async fn load_user_servers(path: &std::path::Path) -> Result<Map<String, Value>, String> {
    let text = match tokio::fs::read_to_string(path).await {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;
    match value.get("mcpServers") {
        Some(Value::Object(map)) => Ok(map.clone()),
        Some(_) => Err(format!(
            "mcpServers in {} must be an object",
            path.display()
        )),
        None => Ok(Map::new()),
    }
}

fn error_to_ui(error: &AiError) -> (ErrorKind, String) {
    match error {
        AiError::NotInstalled { .. } => (
            ErrorKind::CliNotInstalled,
            "Claude Code was not found. Install it, make sure `claude` runs in a terminal, then restart Relay.".into(),
        ),
        AiError::NotLoggedIn(_) => (
            ErrorKind::NotLoggedIn,
            "You are not signed in to Claude Code. Open a terminal, run `claude auth login`, then send your request again.".into(),
        ),
        AiError::Timeout(kind) => (
            ErrorKind::Timeout,
            match kind {
                TimeoutKind::Total => "The task took too long and was stopped.".into(),
                TimeoutKind::Idle => "Claude Code stopped responding and was stopped.".into(),
            },
        ),
        AiError::Cancelled => (ErrorKind::Cancelled, "The task was stopped.".into()),
        other => (ErrorKind::Cli, other.to_string()),
    }
}

fn tool_display(name: &str, input: &Value) -> (String, String) {
    if let Some(short) = name.strip_prefix("mcp__relay__") {
        let summary = parse_action(short, input)
            .map(|a| a.describe())
            .unwrap_or_else(|_| short.to_string());
        return (short.to_string(), summary);
    }
    let brief = [
        "command",
        "file_path",
        "path",
        "url",
        "pattern",
        "query",
        "description",
    ]
    .iter()
    .find_map(|k| input.get(*k).and_then(Value::as_str))
    .map(|s| truncate(s, 200))
    .unwrap_or_default();
    (name.to_string(), brief)
}

/// Translate a provider event to chat events (not including `Finished`).
pub fn map_event(run_id: RunId, event: &AiEvent) -> Vec<UiEvent> {
    match event {
        AiEvent::Started { .. } | AiEvent::Finished(_) => Vec::new(),
        AiEvent::TextDelta(text) => vec![UiEvent::AssistantDelta {
            run_id,
            text: text.clone(),
        }],
        AiEvent::Text(text) => vec![UiEvent::AssistantMessage {
            run_id,
            text: text.clone(),
        }],
        AiEvent::ToolUse { id, name, input } => {
            let (name, summary) = tool_display(name, input);
            vec![UiEvent::ToolStarted {
                run_id,
                tool_id: id.clone(),
                name,
                summary,
            }]
        }
        AiEvent::ToolResult {
            id,
            is_error,
            content,
        } => vec![UiEvent::ToolFinished {
            run_id,
            tool_id: id.clone(),
            ok: !is_error,
            summary: truncate(content, 300),
        }],
    }
}

impl Agent {
    pub fn new(
        provider: Arc<dyn AiProvider>,
        mcp: Arc<McpServer>,
        kill: KillSwitch,
        bridge: Arc<ConfirmBridge>,
        config: AgentConfig,
        sink: EventSink,
    ) -> Arc<Self> {
        Arc::new(Self {
            provider,
            mcp,
            kill,
            bridge,
            config,
            sink,
            state: Mutex::new(State {
                next_run: 1,
                ..State::default()
            }),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn emit(&self, event: UiEvent) {
        (self.sink)(event);
    }

    pub fn is_busy(&self) -> bool {
        self.state().active.is_some()
    }

    /// Forget the conversation so the next task starts fresh.
    pub fn new_conversation(&self) {
        self.state().session_id = None;
    }

    /// The owner's click on a confirmation card.
    pub fn answer(&self, answer: ConfirmAnswer) -> bool {
        self.bridge.answer(answer)
    }

    /// Stop whatever the agent is doing, immediately.
    pub fn stop(&self) {
        info!("stop requested");
        self.kill.trip();
        self.bridge.deny_all();
        if let Some(active) = &self.state().active {
            active.stop.cancel();
        }
    }

    pub async fn status(&self) -> StatusInfo {
        let provider = self.provider.status().await;
        StatusInfo {
            provider: self.provider.name().to_string(),
            ready: provider.ready(),
            detail: provider.detail,
            busy: self.is_busy(),
            kill_hotkey: self.config.kill_hotkey.clone(),
        }
    }

    /// Start a task. Returns at once; progress arrives as events.
    pub fn submit(self: &Arc<Self>, text: &str) -> Result<RunId, SubmitError> {
        let prompt = text.trim().to_string();
        if prompt.is_empty() {
            return Err(SubmitError::Empty);
        }
        let (run_id, stop, session) = {
            let mut state = self.state();
            if state.active.is_some() {
                return Err(SubmitError::Busy);
            }
            let run_id = state.next_run;
            state.next_run += 1;
            let stop = CancellationToken::new();
            state.active = Some(Active { stop: stop.clone() });
            let session = match &state.session_id {
                Some(id) => SessionMode::Resume(id.clone()),
                None => SessionMode::New(uuid::Uuid::new_v4().to_string()),
            };
            (run_id, stop, session)
        };
        // A new task is the owner's explicit go-ahead, so the earlier stop is lifted.
        self.kill.rearm();
        self.emit(UiEvent::RunStarted { run_id });
        let this = self.clone();
        tokio::spawn(async move {
            this.run_task(run_id, prompt, session, stop).await;
        });
        Ok(run_id)
    }

    async fn run_task(
        self: Arc<Self>,
        run_id: RunId,
        prompt: String,
        session: SessionMode,
        stop: CancellationToken,
    ) {
        let grant = self.mcp.grant(run_id);
        let user_servers = match &self.config.user_mcp_config {
            Some(path) => load_user_servers(path).await.unwrap_or_else(|problem| {
                warn!("{problem}");
                self.emit(UiEvent::Error {
                    run_id: Some(run_id),
                    kind: ErrorKind::Internal,
                    message: format!("Your MCP server list was ignored: {problem}"),
                });
                Map::new()
            }),
            None => Map::new(),
        };
        let request = RunRequest {
            prompt,
            session,
            work_dir: self.config.work_dir.clone(),
            mcp_servers: merge_servers(grant.server_config(), user_servers),
            permission_prompt_tool: Some(PERMISSION_TOOL.to_string()),
            allowed_tools: vec![format!("mcp__{SERVER_NAME}")],
            system_prompt_append: Some(SYSTEM_PROMPT.to_string()),
            model: self.config.model.clone(),
            timeouts: self.config.timeouts,
        };

        let outcome = match self.provider.run(request).await {
            Err(error) => Finish::Failed(error),
            Ok(mut handle) => {
                let mut cancelled = false;
                loop {
                    tokio::select! {
                        _ = stop.cancelled(), if !cancelled => {
                            cancelled = true;
                            handle.cancel();
                        }
                        event = handle.events.recv() => match event {
                            None => break Finish::Failed(AiError::Io("the AI process ended unexpectedly".into())),
                            Some(AiEvent::Finished(finish)) => break finish,
                            Some(event) => {
                                if let AiEvent::Started { session_id, .. } = &event {
                                    self.state().session_id = Some(session_id.clone());
                                }
                                for ui in map_event(run_id, &event) {
                                    self.emit(ui);
                                }
                            }
                        }
                    }
                }
            }
        };

        self.mcp.revoke(&grant);
        self.bridge.deny_all();
        let run_outcome = match &outcome {
            Finish::Completed { session_id, .. } => {
                if let Some(id) = session_id {
                    self.state().session_id = Some(id.clone());
                }
                RunOutcome::Completed
            }
            Finish::Cancelled => RunOutcome::Cancelled,
            Finish::TimedOut(kind) => {
                let (kind, message) = error_to_ui(&AiError::Timeout(*kind));
                self.emit(UiEvent::Error {
                    run_id: Some(run_id),
                    kind,
                    message,
                });
                RunOutcome::TimedOut
            }
            Finish::Failed(error) => {
                if error.to_string().contains("No conversation found") {
                    self.state().session_id = None;
                }
                let (kind, message) = error_to_ui(error);
                self.emit(UiEvent::Error {
                    run_id: Some(run_id),
                    kind,
                    message,
                });
                RunOutcome::Failed
            }
        };
        self.state().active = None;
        self.emit(UiEvent::RunFinished {
            run_id,
            outcome: run_outcome,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn relay_server_entry_cannot_be_overridden() {
        let relay = json!({"relay": {"type": "http", "url": "http://127.0.0.1:1/mcp"}});
        let mut user = Map::new();
        user.insert("relay".into(), json!({"command": "evil"}));
        user.insert("github".into(), json!({"command": "gh-mcp"}));
        let merged = merge_servers(relay, user);
        assert_eq!(merged["relay"]["type"], "http");
        assert_eq!(merged["github"]["command"], "gh-mcp");
    }

    #[test]
    fn events_map_to_chat_events() {
        let tool = map_event(
            3,
            &AiEvent::ToolUse {
                id: "t".into(),
                name: "mcp__relay__left_click".into(),
                input: json!({"coordinate": [4, 5]}),
            },
        );
        assert_eq!(
            tool,
            vec![UiEvent::ToolStarted {
                run_id: 3,
                tool_id: "t".into(),
                name: "left_click".into(),
                summary: "Left click at (4, 5)".into()
            }]
        );
        let other = map_event(
            3,
            &AiEvent::ToolUse {
                id: "u".into(),
                name: "Bash".into(),
                input: json!({"command": "dir"}),
            },
        );
        assert!(
            matches!(&other[0], UiEvent::ToolStarted { name, summary, .. } if name == "Bash" && summary == "dir")
        );
        assert!(map_event(
            3,
            &AiEvent::Started {
                session_id: "s".into(),
                model: None
            }
        )
        .is_empty());
        let res = map_event(
            3,
            &AiEvent::ToolResult {
                id: "t".into(),
                is_error: true,
                content: "x".repeat(1000),
            },
        );
        assert!(
            matches!(&res[0], UiEvent::ToolFinished { ok: false, summary, .. } if summary.chars().count() <= 301)
        );
    }

    #[test]
    fn errors_get_actionable_messages() {
        let (kind, msg) = error_to_ui(&AiError::NotLoggedIn("x".into()));
        assert_eq!(kind, ErrorKind::NotLoggedIn);
        assert!(msg.contains("claude auth login"));
        let (kind, msg) = error_to_ui(&AiError::NotInstalled {
            program: "claude".into(),
        });
        assert_eq!(kind, ErrorKind::CliNotInstalled);
        assert!(msg.contains("Install"));
        assert_eq!(
            error_to_ui(&AiError::Timeout(TimeoutKind::Idle)).0,
            ErrorKind::Timeout
        );
        assert_eq!(
            error_to_ui(&AiError::Cli {
                message: "boom".into(),
                exit_code: Some(1)
            }),
            (ErrorKind::Cli, "boom".to_string())
        );
    }

    #[tokio::test]
    async fn user_server_file_is_read_leniently() {
        let dir = std::env::temp_dir().join(format!("relay-agent-cfg-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let missing = dir.join("none.json");
        assert!(load_user_servers(&missing).await.unwrap().is_empty());
        let good = dir.join("good.json");
        tokio::fs::write(&good, r#"{"mcpServers":{"gmail":{"command":"x"}}}"#)
            .await
            .unwrap();
        assert!(load_user_servers(&good)
            .await
            .unwrap()
            .contains_key("gmail"));
        let bad = dir.join("bad.json");
        tokio::fs::write(&bad, "{nope").await.unwrap();
        assert!(load_user_servers(&bad).await.is_err());
        let wrong = dir.join("wrong.json");
        tokio::fs::write(&wrong, r#"{"mcpServers":[]}"#)
            .await
            .unwrap();
        assert!(load_user_servers(&wrong).await.is_err());
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
