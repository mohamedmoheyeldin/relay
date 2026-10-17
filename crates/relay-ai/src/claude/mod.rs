//! Claude Code provider: runs `claude -p` headless and streams its JSON output.

mod args;
mod parser;

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

pub use args::build_args;
pub use parser::{classify_error, ClaudeProtocol};

use crate::process::{probe, spawn, Probe, ProcessSpec};
use crate::{AiError, AiProvider, ProviderCaps, ProviderStatus, RunHandle, RunRequest};

/// Environment variable that overrides where the CLI is found.
pub const PATH_OVERRIDE_ENV: &str = "RELAY_CLAUDE_PATH";
const STATUS_TIMEOUT: Duration = Duration::from_secs(20);

/// Find the CLI: explicit override, then `PATH`, then the per-user install folder.
pub fn locate(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os(PATH_OVERRIDE_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        for name in names {
            let candidate = PathBuf::from(&home).join(".local").join("bin").join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(names[0])
}

/// Claude Code as an [`AiProvider`].
#[derive(Debug, Clone)]
pub struct ClaudeProvider {
    program: PathBuf,
}

impl ClaudeProvider {
    /// Use a specific executable (tests point this at a fake CLI).
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// Locate the CLI automatically.
    pub fn discover() -> Self {
        Self::with_program(locate(None))
    }

    pub fn program(&self) -> &Path {
        &self.program
    }
}

fn scratch_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("relay-run-{}-{nanos}", std::process::id()))
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            id: "claude",
            label: "Claude Code",
            structured_output: true,
            session_resume: true,
            mcp_tools: true,
            streaming_deltas: true,
            reasoning_levels: vec!["low", "medium", "high", "xhigh", "max"],
            models: vec!["sonnet", "opus", "haiku"],
        }
    }

    async fn status(&self) -> ProviderStatus {
        let (auth, version) = tokio::join!(
            probe(&self.program, &["auth", "status"], STATUS_TIMEOUT),
            probe(&self.program, &["--version"], STATUS_TIMEOUT)
        );
        let version = match version {
            Probe::Done {
                success: true,
                stdout,
            } => stdout.split_whitespace().next().map(str::to_string),
            _ => None,
        };
        match auth {
            Probe::NotFound => ProviderStatus {
                installed: false,
                detail: format!(
                    "Claude Code was not found ({}). Install it, then restart Relay.",
                    self.program.display()
                ),
                ..ProviderStatus::default()
            },
            Probe::TimedOut => ProviderStatus {
                installed: true,
                detail: "Timed out checking the sign-in status.".into(),
                version,
                ..ProviderStatus::default()
            },
            Probe::Failed(e) => ProviderStatus {
                detail: format!("Could not start Claude Code: {e}"),
                ..ProviderStatus::default()
            },
            Probe::Done { success, stdout } => {
                let json = serde_json::from_str::<serde_json::Value>(stdout.trim()).ok();
                let logged_in = json
                    .as_ref()
                    .and_then(|v| v.get("loggedIn").and_then(serde_json::Value::as_bool))
                    .unwrap_or(success);
                let auth_method = json
                    .as_ref()
                    .and_then(|v| v.get("authMethod").and_then(serde_json::Value::as_str))
                    .map(str::to_string);
                ProviderStatus {
                    installed: true,
                    logged_in,
                    detail: if logged_in {
                        "Ready".into()
                    } else {
                        "Claude Code is installed but you are not signed in. Run `claude auth login` in a terminal.".into()
                    },
                    version,
                    auth_method,
                }
            }
        }
    }
    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError> {
        let scratch = scratch_dir();
        tokio::fs::create_dir_all(&scratch)
            .await
            .map_err(|e| AiError::Io(format!("creating scratch dir: {e}")))?;
        let mcp_path = scratch.join("mcp.json");
        let config = json!({ "mcpServers": request.mcp_servers });
        tokio::fs::write(&mcp_path, config.to_string())
            .await
            .map_err(|e| AiError::Io(format!("writing MCP config: {e}")))?;
        let system_path = match &request.system_prompt_append {
            Some(text) => {
                let p = scratch.join("system.md");
                tokio::fs::write(&p, text)
                    .await
                    .map_err(|e| AiError::Io(format!("writing system prompt: {e}")))?;
                Some(p)
            }
            None => None,
        };
        let spec = ProcessSpec {
            program: self.program.clone(),
            args: build_args(&request, &mcp_path, system_path.as_deref()),
            env: Default::default(),
            work_dir: request.work_dir.clone(),
            stdin: request.prompt.clone(),
            cleanup_dir: Some(scratch),
        };
        spawn(spec, ClaudeProtocol::new(), request.timeouts)
    }
}
