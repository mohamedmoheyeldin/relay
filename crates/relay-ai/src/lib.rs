//! AI providers for Relay.
//!
//! A provider drives an AI command-line tool as a child process and turns its output into a
//! stream of [`AiEvent`]s. Only Claude Code is implemented today ([`claude::ClaudeProvider`]);
//! new CLIs plug in by implementing [`AiProvider`].

pub mod claude;
pub mod codex;
pub mod error;
pub mod process;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub use error::{AiError, TimeoutKind};

/// How the provider should treat conversation history for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionMode {
    /// Start a new conversation with this id (a UUID the caller generated).
    New(String),
    /// Continue an existing conversation.
    Resume(String),
}

/// Time limits for one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Maximum wall-clock time for the whole run.
    pub total: Duration,
    /// Maximum time without any output.
    pub idle: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            total: Duration::from_secs(30 * 60),
            idle: Duration::from_secs(5 * 60),
        }
    }
}

/// Everything needed to start one run.
#[derive(Debug, Clone)]
pub struct RunRequest {
    /// Identifies the task for tool calls and the audit log.
    pub run_id: u64,
    pub prompt: String,
    pub session: SessionMode,
    /// Working directory of the child process.
    pub work_dir: PathBuf,
    /// The `mcpServers` map to expose to the AI (name -> server definition).
    pub mcp_servers: Value,
    /// Name of the MCP tool that answers permission prompts, if any.
    pub permission_prompt_tool: Option<String>,
    /// Tool name patterns that never need a prompt.
    pub allowed_tools: Vec<String>,
    /// Tools the AI must not use at all (e.g. ones that stop to ask the owner questions).
    pub disallowed_tools: Vec<String>,
    /// Extra instructions appended to the system prompt.
    pub system_prompt_append: Option<String>,
    pub model: Option<String>,
    /// Reasoning effort (provider specific: low, medium, high, ...).
    pub reasoning: Option<String>,
    /// JSON Schema the final answer must satisfy (structured output).
    pub output_schema: Option<Value>,
    /// Planning only: the provider must not use tools or change anything; it just answers.
    pub plan_only: bool,
    pub timeouts: Timeouts,
}

/// One item in a run's output stream.
#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    Started {
        session_id: String,
        model: Option<String>,
    },
    /// Streaming fragment of assistant text.
    TextDelta(String),
    /// A complete assistant text block.
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        is_error: bool,
        content: String,
    },
    /// Always the last event of a run.
    Finished(Finish),
}

/// How a run ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Finish {
    Completed {
        session_id: Option<String>,
        result: String,
        cost_usd: Option<f64>,
    },
    Cancelled,
    TimedOut(TimeoutKind),
    Failed(AiError),
    /// The engine decided the task needs a stronger one (used by the local model).
    Escalate {
        reason: String,
        /// What it already did, to hand over to the next engine.
        summary: String,
    },
}

/// A running AI process: its event stream and a way to cancel it.
pub struct RunHandle {
    pub events: mpsc::Receiver<AiEvent>,
    cancel: CancellationToken,
}

impl RunHandle {
    /// Build a handle around an event channel and a cancellation token (used by providers).
    pub fn new(events: mpsc::Receiver<AiEvent>, cancel: CancellationToken) -> Self {
        Self { events, cancel }
    }

    /// Stop the run and kill the whole process tree.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// A token that cancels this run; cheap to clone and hand to other tasks.
    pub fn canceller(&self) -> CancellationToken {
        self.cancel.clone()
    }
}

/// Whether the provider can currently run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProviderStatus {
    pub installed: bool,
    pub logged_in: bool,
    pub detail: String,
    /// The CLI's version string, when it could be read.
    pub version: Option<String>,
    /// How the owner signed in (for display only; credentials are never read).
    pub auth_method: Option<String>,
}

/// What a provider can do, so the UI and router never assume the two CLIs are the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderCaps {
    pub id: &'static str,
    pub label: &'static str,
    pub structured_output: bool,
    pub session_resume: bool,
    pub mcp_tools: bool,
    /// Real token-by-token text deltas (otherwise whole messages arrive at once).
    pub streaming_deltas: bool,
    pub reasoning_levels: Vec<&'static str>,
    /// Suggested model names (the CLI accepts others).
    pub models: Vec<&'static str>,
}

impl ProviderStatus {
    pub fn ready(&self) -> bool {
        self.installed && self.logged_in
    }
}

/// An AI command-line tool Relay can drive.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Short display name.
    fn name(&self) -> &str;
    /// What this provider supports.
    fn capabilities(&self) -> ProviderCaps;
    /// Check installation and login without starting a conversation.
    async fn status(&self) -> ProviderStatus;
    /// Start a run. Errors that happen before the process exists (not installed) come back here;
    /// everything later arrives as [`AiEvent::Finished`].
    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError>;
}
