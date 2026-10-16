//! AI providers for Relay.
//!
//! A provider drives an AI command-line tool as a child process and turns its output into a
//! stream of [`AiEvent`]s. Only Claude Code is implemented today ([`claude::ClaudeProvider`]);
//! new CLIs plug in by implementing [`AiProvider`].

pub mod claude;
pub mod error;
pub mod process;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
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
    /// Extra instructions appended to the system prompt.
    pub system_prompt_append: Option<String>,
    pub model: Option<String>,
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
}

/// A running AI process: its event stream and a way to cancel it.
pub struct RunHandle {
    pub events: mpsc::Receiver<AiEvent>,
    cancel: CancellationToken,
}

impl RunHandle {
    pub(crate) fn new(events: mpsc::Receiver<AiEvent>, cancel: CancellationToken) -> Self {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStatus {
    pub installed: bool,
    pub logged_in: bool,
    pub detail: String,
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
    /// Check installation and login without starting a conversation.
    async fn status(&self) -> ProviderStatus;
    /// Start a run. Errors that happen before the process exists (not installed) come back here;
    /// everything later arrives as [`AiEvent::Finished`].
    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError>;
}
