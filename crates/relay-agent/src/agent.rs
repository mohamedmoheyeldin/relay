//! The orchestrator: one task at a time, from the user's message to the finished run.
//!
//! A task goes to the local model or the cloud CLI (see [`crate::local::route`]). The local
//! model can hand a task over to the cloud CLI, which then continues with what was done.

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
use crate::local::{decide, Engine, LocalProvider, RouteMode};
use crate::prompt::SYSTEM_PROMPT;

/// Settings for the agent.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Working directory handed to the AI CLI.
    pub work_dir: PathBuf,
    /// Optional `{"mcpServers": {...}}` file with the owner's own MCP servers (Gmail, GitHub, ...).
    pub user_mcp_config: Option<PathBuf>,
    pub timeouts: Timeouts,
    /// Model override for the cloud CLI.
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
    /// Recent (request, final answer) pairs, so every engine has the conversation so far.
    history: Vec<(String, String)>,
}

const HISTORY_TURNS: usize = 6;

pub struct Agent {
    cloud: Arc<dyn AiProvider>,
    local: Arc<LocalProvider>,
    route: Mutex<RouteMode>,
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
        AiError::NotInstalled { program } => (
            ErrorKind::CliNotInstalled,
            if program.contains("Ollama") {
                "The local AI server is not running. Install Ollama and download a model in Settings, or switch Settings to Cloud only.".to_string()
            } else {
                "Claude Code was not found. Install it, make sure `claude` runs in a terminal, then restart Relay.".to_string()
            },
        ),
        AiError::NotLoggedIn(_) => (
            ErrorKind::NotLoggedIn,
            "You are not signed in to Claude Code. Open a terminal, run `claude auth login`, then send your request again.".into(),
        ),
        AiError::Timeout(kind) => (
            ErrorKind::Timeout,
            match kind {
                TimeoutKind::Total => "The task took too long and was stopped.".into(),
                TimeoutKind::Idle => "The AI stopped responding and was stopped.".into(),
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
    // Local-model tools use bare names; describe desktop actions the same way.
    if let Ok(action) = parse_action(name, input) {
        return (name.to_string(), action.describe());
    }
    let brief = [
        "command",
        "file_path",
        "path",
        "target",
        "url",
        "pattern",
        "query",
        "description",
        "reason",
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

/// Prefix a request with the recent conversation so an engine without its own memory has it.
pub fn with_context(history: &[(String, String)], prompt: &str) -> String {
    if history.is_empty() {
        return prompt.to_string();
    }
    let mut out = String::from("Earlier in this conversation:\n");
    for (ask, answer) in history {
        out.push_str(&format!(
            "- Owner: {}\n  You: {}\n",
            truncate(ask, 300),
            truncate(answer, 400)
        ));
    }
    out.push_str("\nCurrent request:\n");
    out.push_str(prompt);
    out
}

fn handoff_prompt(prompt: &str, reason: &str, summary: &str) -> String {
    format!(
        "{prompt}\n\n[A faster local assistant already started this and handed it over because: {reason}\nWhat it did so far:\n{summary}\nContinue from there; do not repeat finished steps.]"
    )
}

/// The collaborators an [`Agent`] is built from.
pub struct AgentParts {
    pub cloud: Arc<dyn AiProvider>,
    pub local: Arc<LocalProvider>,
    pub route: RouteMode,
    pub mcp: Arc<McpServer>,
    pub kill: KillSwitch,
    pub bridge: Arc<ConfirmBridge>,
}

impl Agent {
    pub fn new(parts: AgentParts, config: AgentConfig, sink: EventSink) -> Arc<Self> {
        Arc::new(Self {
            cloud: parts.cloud,
            local: parts.local,
            route: Mutex::new(parts.route),
            mcp: parts.mcp,
            kill: parts.kill,
            bridge: parts.bridge,
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

    pub fn route_mode(&self) -> RouteMode {
        *self.route.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_route_mode(&self, mode: RouteMode) {
        *self.route.lock().unwrap_or_else(|p| p.into_inner()) = mode;
    }

    pub fn local(&self) -> &Arc<LocalProvider> {
        &self.local
    }

    pub fn cloud(&self) -> &Arc<dyn AiProvider> {
        &self.cloud
    }

    /// Forget the conversation so the next task starts fresh.
    pub fn new_conversation(&self) {
        let mut state = self.state();
        state.session_id = None;
        state.history.clear();
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
        let cloud = self.cloud.status().await;
        let local = self.local.overview().await;
        StatusInfo {
            provider: self.cloud.name().to_string(),
            ready: cloud.ready() || local.active.is_some(),
            detail: if cloud.ready() {
                cloud.detail
            } else if let Some(m) = &local.active {
                format!("Local model {}", m.name)
            } else {
                cloud.detail
            },
            busy: self.is_busy(),
            kill_hotkey: self.config.kill_hotkey.clone(),
            local_model: local.active.map(|m| m.name),
            route_mode: self.route_mode().as_str().to_string(),
        }
    }

    /// Start a task. Returns at once; progress arrives as events.
    pub fn submit(self: &Arc<Self>, text: &str) -> Result<RunId, SubmitError> {
        let prompt = text.trim().to_string();
        if prompt.is_empty() {
            return Err(SubmitError::Empty);
        }
        let (run_id, stop) = {
            let mut state = self.state();
            if state.active.is_some() {
                return Err(SubmitError::Busy);
            }
            let run_id = state.next_run;
            state.next_run += 1;
            let stop = CancellationToken::new();
            state.active = Some(Active { stop: stop.clone() });
            (run_id, stop)
        };
        // A new task is the owner's explicit go-ahead, so the earlier stop is lifted.
        self.kill.rearm();
        self.emit(UiEvent::RunStarted { run_id });
        let this = self.clone();
        tokio::spawn(async move {
            this.run_task(run_id, prompt, stop).await;
        });
        Ok(run_id)
    }

    /// Run one engine to completion, forwarding its events to the chat.
    async fn drive(
        &self,
        run_id: RunId,
        provider: &Arc<dyn AiProvider>,
        request: RunRequest,
        stop: &CancellationToken,
        is_cloud: bool,
    ) -> Finish {
        match provider.run(request).await {
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
                                    if is_cloud && !session_id.is_empty() {
                                        self.state().session_id = Some(session_id.clone());
                                    }
                                }
                                for ui in map_event(run_id, &event) {
                                    self.emit(ui);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn request(
        &self,
        run_id: RunId,
        prompt: String,
        session: SessionMode,
        mcp_servers: Value,
        cloud: bool,
    ) -> RunRequest {
        RunRequest {
            run_id,
            prompt,
            session,
            work_dir: self.config.work_dir.clone(),
            mcp_servers,
            permission_prompt_tool: cloud.then(|| PERMISSION_TOOL.to_string()),
            allowed_tools: if cloud {
                vec![format!("mcp__{SERVER_NAME}")]
            } else {
                Vec::new()
            },
            // The owner wants uninterrupted work: never stop to ask questions.
            disallowed_tools: if cloud {
                vec!["AskUserQuestion".to_string()]
            } else {
                Vec::new()
            },
            system_prompt_append: cloud.then(|| SYSTEM_PROMPT.to_string()),
            model: if cloud {
                self.config.model.clone()
            } else {
                None
            },
            timeouts: self.config.timeouts,
        }
    }

    async fn run_task(self: Arc<Self>, run_id: RunId, prompt: String, stop: CancellationToken) {
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
        let mcp_servers = merge_servers(grant.server_config(), user_servers);
        let mode = self.route_mode();
        let overview = self.local.overview().await;
        let ability = overview.ability();
        let history = self.state().history.clone();
        let mut engine = decide(mode, &prompt, ability);
        let mut handoff: Option<(String, String)> = None;
        let mut local_name = overview.active.map(|m| m.name);

        let outcome = loop {
            let finish = match engine {
                Engine::Local => {
                    self.emit(UiEvent::Engine {
                        run_id,
                        kind: "local".into(),
                        name: local_name.clone().unwrap_or_else(|| "Local AI".into()),
                    });
                    let request = self.request(
                        run_id,
                        with_context(&history, &prompt),
                        SessionMode::New(uuid::Uuid::new_v4().to_string()),
                        Value::Null,
                        false,
                    );
                    let provider: Arc<dyn AiProvider> = self.local.clone();
                    self.drive(run_id, &provider, request, &stop, false).await
                }
                Engine::Cloud => {
                    self.emit(UiEvent::Engine {
                        run_id,
                        kind: "cloud".into(),
                        name: self.cloud.name().to_string(),
                    });
                    let (session, base) = {
                        let state = self.state();
                        match &state.session_id {
                            Some(id) => (SessionMode::Resume(id.clone()), prompt.clone()),
                            None => (
                                SessionMode::New(uuid::Uuid::new_v4().to_string()),
                                with_context(&state.history, &prompt),
                            ),
                        }
                    };
                    let text = match &handoff {
                        Some((reason, summary)) => handoff_prompt(&base, reason, summary),
                        None => base,
                    };
                    let request = self.request(run_id, text, session, mcp_servers.clone(), true);
                    let provider = self.cloud.clone();
                    self.drive(run_id, &provider, request, &stop, true).await
                }
            };
            // Local problems fall back to the cloud engine unless the owner chose local only.
            let fallback = match (&finish, engine, mode) {
                (Finish::Escalate { reason, summary }, Engine::Local, _) => {
                    Some((reason.clone(), summary.clone()))
                }
                (Finish::Failed(e), Engine::Local, RouteMode::Auto) => {
                    Some((e.to_string(), String::new()))
                }
                _ => None,
            };
            match fallback {
                Some((reason, summary)) if mode != RouteMode::LocalOnly && !stop.is_cancelled() => {
                    self.emit(UiEvent::Notice {
                        text: format!("Handing this to Claude: {reason}"),
                    });
                    handoff = Some((reason, summary));
                    engine = Engine::Cloud;
                    local_name = None;
                }
                _ => break finish,
            }
        };

        self.mcp.revoke(&grant);
        self.bridge.deny_all();
        let run_outcome = match &outcome {
            Finish::Completed {
                session_id, result, ..
            } => {
                let mut state = self.state();
                if let Some(id) = session_id {
                    state.session_id = Some(id.clone());
                }
                state.history.push((prompt.clone(), result.clone()));
                let excess = state.history.len().saturating_sub(HISTORY_TURNS);
                state.history.drain(..excess);
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
            Finish::Escalate { reason, .. } => {
                self.emit(UiEvent::Error {
                    run_id: Some(run_id),
                    kind: ErrorKind::Cli,
                    message: format!("The local model could not finish this ({reason}). Switch Settings to Auto or Cloud to let Claude handle it."),
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
        let local = map_event(
            3,
            &AiEvent::ToolUse {
                id: "l".into(),
                name: "left_click".into(),
                input: json!({"coordinate": [1, 2]}),
            },
        );
        assert!(
            matches!(&local[0], UiEvent::ToolStarted { summary, .. } if summary == "Left click at (1, 2)")
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
        let (_, msg) = error_to_ui(&AiError::NotInstalled {
            program: "a local AI server (Ollama) at x".into(),
        });
        assert!(msg.contains("Ollama"));
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

    #[test]
    fn context_and_handoff_prompts() {
        assert_eq!(with_context(&[], "do x"), "do x");
        let ctx = with_context(&[("a".into(), "b".into())], "do x");
        assert!(ctx.contains("Owner: a") && ctx.contains("You: b") && ctx.ends_with("do x"));
        let h = handoff_prompt("do x", "needs web", "ran dir");
        assert!(h.contains("needs web") && h.contains("ran dir") && h.starts_with("do x"));
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
