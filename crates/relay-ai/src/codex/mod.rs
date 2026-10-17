//! OpenAI Codex CLI provider: runs `codex exec --json` and streams its JSONL events.
//!
//! Relay never reads Codex's credential files: sign-in state comes only from `codex login status`
//! and the owner signs in themselves with `codex login`.

mod args;
mod parser;

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;

pub use args::{build_args, relay_mcp, toml_string, MCP_TOKEN_ENV};
pub use parser::CodexProtocol;

use crate::process::{probe, spawn, Probe, ProcessSpec};
use crate::{AiError, AiProvider, ProviderCaps, ProviderStatus, RunHandle, RunRequest};

/// Environment variable that overrides where the CLI is found.
pub const PATH_OVERRIDE_ENV: &str = "RELAY_CODEX_PATH";
const STATUS_TIMEOUT: Duration = Duration::from_secs(20);

/// Find the CLI: explicit override, `PATH`, then the default npm global folder.
pub fn locate(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::var_os(PATH_OVERRIDE_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    let names: &[&str] = if cfg!(windows) {
        &["codex.exe", "codex.cmd"]
    } else {
        &["codex"]
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    for dir in dirs {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(names[0])
}

#[derive(Debug, Clone)]
pub struct CodexProvider {
    program: PathBuf,
}

impl CodexProvider {
    pub fn with_program(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }

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
    std::env::temp_dir().join(format!("relay-codex-{}-{nanos}", std::process::id()))
}

#[async_trait]
impl AiProvider for CodexProvider {
    fn name(&self) -> &str {
        "Codex"
    }

    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            id: "codex",
            label: "Codex",
            structured_output: true,
            session_resume: true,
            mcp_tools: true,
            // `exec --json` delivers whole messages, not token deltas.
            streaming_deltas: false,
            reasoning_levels: vec!["low", "medium", "high", "xhigh"],
            models: Vec::new(),
        }
    }

    async fn status(&self) -> ProviderStatus {
        let (login, version) = tokio::join!(
            probe(&self.program, &["login", "status"], STATUS_TIMEOUT),
            probe(&self.program, &["--version"], STATUS_TIMEOUT)
        );
        let version = match version {
            Probe::Done {
                success: true,
                stdout,
            } => stdout
                .split_whitespace()
                .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .map(str::to_string),
            _ => None,
        };
        match login {
            Probe::NotFound => ProviderStatus {
                detail: "Codex was not found. Install it with `npm install -g @openai/codex`, then restart Relay.".into(),
                ..ProviderStatus::default()
            },
            Probe::TimedOut => ProviderStatus {
                installed: true,
                detail: "Timed out checking the sign-in status.".into(),
                version,
                ..ProviderStatus::default()
            },
            Probe::Failed(e) => ProviderStatus {
                detail: format!("Could not start Codex: {e}"),
                ..ProviderStatus::default()
            },
            // The output text of `login status` is undocumented; the exit code is the signal.
            Probe::Done { success, stdout } => ProviderStatus {
                installed: true,
                logged_in: success,
                detail: if success {
                    "Ready".into()
                } else {
                    "Codex is installed but you are not signed in. Run `codex login` in a terminal.".into()
                },
                version,
                auth_method: success
                    .then(|| stdout.lines().next().map(|l| l.trim().to_string()))
                    .flatten()
                    .filter(|l| !l.is_empty() && l.len() < 80),
            },
        }
    }

    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError> {
        let scratch = scratch_dir();
        tokio::fs::create_dir_all(&scratch)
            .await
            .map_err(|e| AiError::Io(format!("creating scratch dir: {e}")))?;
        let schema_path = match &request.output_schema {
            Some(schema) => {
                let p = scratch.join("schema.json");
                tokio::fs::write(&p, schema.to_string())
                    .await
                    .map_err(|e| AiError::Io(format!("writing the output schema: {e}")))?;
                Some(p)
            }
            None => None,
        };
        let mut env = std::collections::BTreeMap::new();
        if !request.plan_only {
            if let Some((_, Some(token))) = relay_mcp(&request) {
                env.insert(MCP_TOKEN_ENV.to_string(), token);
            }
        }
        // Instructions go in front of the prompt: Codex has no system-prompt flag for `exec`.
        let prompt = match &request.system_prompt_append {
            Some(text) => format!("{text}\n\n---\n\n{}", request.prompt),
            None => request.prompt.clone(),
        };
        let spec = ProcessSpec {
            program: self.program.clone(),
            args: build_args(&request, schema_path.as_deref()),
            env,
            work_dir: request.work_dir.clone(),
            stdin: prompt,
            cleanup_dir: Some(scratch),
        };
        spawn(spec, CodexProtocol::new(), request.timeouts)
    }
}
