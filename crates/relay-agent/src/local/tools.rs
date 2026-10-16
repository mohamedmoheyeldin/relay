//! Tools the local model can call: OS chores (shell, files, opening things) plus the desktop tools.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use relay_mcp::actions::ToolError;
use relay_mcp::safety::truncate;
use relay_mcp::tools::tool_specs;
use relay_mcp::{CallCtx, Content, ToolHost};
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_OUTPUT: usize = 8000;
const DEFAULT_SHELL_SECS: u64 = 60;
const MAX_SHELL_SECS: u64 = 600;

/// Result of one local tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum LocalOutcome {
    Text {
        text: String,
        is_error: bool,
    },
    /// A screenshot for vision models, with a caption.
    Image {
        png_base64: String,
        text: String,
    },
    /// The model asked for a stronger engine.
    Escalate {
        reason: String,
    },
}

impl LocalOutcome {
    fn ok(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            is_error: false,
        }
    }
    fn err(text: impl Into<String>) -> Self {
        Self::Text {
            text: text.into(),
            is_error: true,
        }
    }
}

fn function(name: &str, description: &str, parameters: Value) -> Value {
    json!({"type": "function", "function": {"name": name, "description": description, "parameters": parameters}})
}

fn props(p: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": p, "required": required})
}

/// Tool definitions in the shape Ollama expects.
pub fn tool_definitions(vision: bool) -> Vec<Value> {
    let mut out = vec![
        function(
            "powershell",
            "Run a PowerShell command on this Windows PC and get its output. Use for files, processes, settings, installs, network checks and most OS chores.",
            props(
                json!({
                    "command": {"type": "string"},
                    "timeout_seconds": {"type": "integer", "description": "Default 60, max 600."}
                }),
                &["command"],
            ),
        ),
        function(
            "read_file",
            "Read a text file.",
            props(json!({"path": {"type": "string"}}), &["path"]),
        ),
        function(
            "write_file",
            "Write (or append to) a text file, creating folders as needed.",
            props(
                json!({"path": {"type": "string"}, "content": {"type": "string"}, "append": {"type": "boolean"}}),
                &["path", "content"],
            ),
        ),
        function(
            "list_dir",
            "List the files and folders in a directory.",
            props(json!({"path": {"type": "string"}}), &["path"]),
        ),
        function(
            "open",
            "Open a file, folder, app or URL with its default program.",
            props(json!({"target": {"type": "string"}}), &["target"]),
        ),
        function(
            "escalate",
            "Hand the task to the stronger cloud assistant. Use when the task needs web research, browsing a site, long reasoning, writing or coding, or when you are stuck or failing repeatedly.",
            props(json!({"reason": {"type": "string"}}), &["reason"]),
        ),
    ];
    for spec in tool_specs() {
        if matches!(spec.name, "approve" | "ask_user" | "computer_batch") {
            continue;
        }
        if !vision && matches!(spec.name, "screenshot" | "zoom") {
            continue;
        }
        out.push(function(spec.name, spec.description, spec.schema));
    }
    out
}

fn string_arg(args: &Value, key: &str) -> Result<String, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::new(format!("{key} is required and must be a string")))
}

/// Run a PowerShell command with a time limit; killed when `cancel` fires.
pub async fn run_powershell(command: &str, secs: u64, cancel: &CancellationToken) -> LocalOutcome {
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        command,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return LocalOutcome::err(format!("could not start PowerShell: {e}")),
    };
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let read_all = async {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut out).await;
        }
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut err).await;
        }
        (out, err)
    };
    let work = async {
        let (out, err) = read_all.await;
        let status = child.wait().await;
        (out, err, status)
    };
    let limit = Duration::from_secs(secs.clamp(1, MAX_SHELL_SECS));
    tokio::select! {
        _ = cancel.cancelled() => LocalOutcome::err("stopped by the owner"),
        timed = tokio::time::timeout(limit, work) => match timed {
            Err(_) => LocalOutcome::err(format!("timed out after {} s", limit.as_secs())),
            Ok((out, err, status)) => {
                let mut text = String::from_utf8_lossy(&out).into_owned();
                let err = String::from_utf8_lossy(&err);
                if !err.trim().is_empty() {
                    text.push_str("\n[stderr]\n");
                    text.push_str(&err);
                }
                let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
                let text = truncate(text.trim(), MAX_OUTPUT);
                if code == 0 {
                    LocalOutcome::ok(if text.is_empty() { "(no output)".to_string() } else { text })
                } else {
                    LocalOutcome::err(format!("exit code {code}\n{text}"))
                }
            }
        }
    }
}

fn single_quote(text: &str) -> String {
    text.replace('\'', "''")
}

/// Executes local tools; desktop tools are forwarded to the shared [`ToolHost`].
pub struct LocalTools {
    host: Arc<ToolHost>,
}

impl LocalTools {
    pub fn new(host: Arc<ToolHost>) -> Self {
        Self { host }
    }

    /// Ask the safety layer (a no-op when approvals are off).
    async fn allowed(&self, ctx: &CallCtx, tool: &str, input: Value) -> Result<(), String> {
        let out = self
            .host
            .call(ctx, "approve", &json!({"tool_name": tool, "input": input}))
            .await;
        let verdict: Value = serde_json::from_str(&out.joined_text()).unwrap_or(Value::Null);
        if verdict["behavior"] == "allow" {
            Ok(())
        } else {
            Err(verdict["message"]
                .as_str()
                .unwrap_or("not allowed")
                .to_string())
        }
    }

    pub async fn execute(
        &self,
        ctx: &CallCtx,
        name: &str,
        args: &Value,
        cancel: &CancellationToken,
    ) -> LocalOutcome {
        match self.execute_inner(ctx, name, args, cancel).await {
            Ok(outcome) => outcome,
            Err(e) => LocalOutcome::err(e.0),
        }
    }

    async fn execute_inner(
        &self,
        ctx: &CallCtx,
        name: &str,
        args: &Value,
        cancel: &CancellationToken,
    ) -> Result<LocalOutcome, ToolError> {
        match name {
            "powershell" => {
                let command = string_arg(args, "command")?;
                self.allowed(ctx, "PowerShell", json!({"command": command}))
                    .await
                    .map_err(ToolError::new)?;
                let secs = args
                    .get("timeout_seconds")
                    .and_then(Value::as_u64)
                    .unwrap_or(DEFAULT_SHELL_SECS);
                Ok(run_powershell(&command, secs, cancel).await)
            }
            "open" => {
                let target = string_arg(args, "target")?;
                let command = format!("Start-Process -FilePath '{}'", single_quote(&target));
                self.allowed(ctx, "PowerShell", json!({"command": command}))
                    .await
                    .map_err(ToolError::new)?;
                Ok(run_powershell(&command, 20, cancel).await)
            }
            "read_file" => {
                let path = string_arg(args, "path")?;
                match tokio::fs::read(&path).await {
                    Ok(bytes) => Ok(LocalOutcome::ok(truncate(
                        &String::from_utf8_lossy(&bytes),
                        20_000,
                    ))),
                    Err(e) => Ok(LocalOutcome::err(format!("cannot read {path}: {e}"))),
                }
            }
            "write_file" => {
                let path = string_arg(args, "path")?;
                let content = string_arg(args, "content")?;
                let append = args.get("append").and_then(Value::as_bool).unwrap_or(false);
                self.allowed(ctx, "Write", json!({"file_path": path}))
                    .await
                    .map_err(ToolError::new)?;
                Ok(write_file(&path, &content, append).await)
            }
            "list_dir" => {
                let path = string_arg(args, "path")?;
                Ok(list_dir(&path).await)
            }
            "escalate" => Ok(LocalOutcome::Escalate {
                reason: string_arg(args, "reason")
                    .unwrap_or_else(|_| "needs a stronger model".into()),
            }),
            other => {
                if matches!(other, "approve" | "ask_user" | "computer_batch") {
                    return Err(ToolError::new(format!("unknown tool {other:?}")));
                }
                let out = self.host.call(ctx, other, args).await;
                let image = out.content.iter().find_map(|c| match c {
                    Content::Image { png_base64 } => Some(png_base64.clone()),
                    Content::Text(_) => None,
                });
                let text = out.joined_text();
                Ok(match image {
                    Some(png_base64) if !out.is_error => LocalOutcome::Image { png_base64, text },
                    _ => LocalOutcome::Text {
                        text,
                        is_error: out.is_error,
                    },
                })
            }
        }
    }
}

async fn write_file(path: &str, content: &str, append: bool) -> LocalOutcome {
    use tokio::io::AsyncWriteExt;
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return LocalOutcome::err(format!("cannot create {}: {e}", parent.display()));
            }
        }
    }
    let result = if append {
        match tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
        {
            Ok(mut f) => f.write_all(content.as_bytes()).await,
            Err(e) => Err(e),
        }
    } else {
        tokio::fs::write(path, content).await
    };
    match result {
        Ok(()) => LocalOutcome::ok(format!("wrote {} bytes to {path}", content.len())),
        Err(e) => LocalOutcome::err(format!("cannot write {path}: {e}")),
    }
}

async fn list_dir(path: &str) -> LocalOutcome {
    let mut rd = match tokio::fs::read_dir(path).await {
        Ok(rd) => rd,
        Err(e) => return LocalOutcome::err(format!("cannot list {path}: {e}")),
    };
    let mut lines = Vec::new();
    while let Ok(Some(entry)) = rd.next_entry().await {
        let meta = entry.metadata().await.ok();
        let name = entry.file_name().to_string_lossy().into_owned();
        lines.push(match meta {
            Some(m) if m.is_dir() => format!("{name}/"),
            Some(m) => format!("{name}  ({} bytes)", m.len()),
            None => name,
        });
        if lines.len() >= 300 {
            lines.push("… (more entries not shown)".into());
            break;
        }
    }
    lines.sort();
    LocalOutcome::ok(if lines.is_empty() {
        "(empty)".to_string()
    } else {
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(vision: bool) -> Vec<String> {
        tool_definitions(vision)
            .iter()
            .map(|t| {
                t["function"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn definitions_include_chores_and_desktop_tools() {
        let n = names(true);
        for want in [
            "powershell",
            "read_file",
            "write_file",
            "list_dir",
            "open",
            "escalate",
            "left_click",
            "type",
            "screenshot",
        ] {
            assert!(n.contains(&want.to_string()), "{want}");
        }
        for hidden in ["approve", "ask_user", "computer_batch"] {
            assert!(!n.contains(&hidden.to_string()), "{hidden}");
        }
    }

    #[test]
    fn blind_models_do_not_get_screenshot_tools() {
        let n = names(false);
        assert!(!n.contains(&"screenshot".to_string()));
        assert!(!n.contains(&"zoom".to_string()));
        assert!(n.contains(&"list_windows".to_string()));
    }

    #[test]
    fn quotes_are_escaped_for_powershell() {
        assert_eq!(single_quote("it's"), "it''s");
    }

    #[tokio::test]
    async fn file_tools_round_trip() {
        let dir = std::env::temp_dir().join(format!("relay-local-{}", std::process::id()));
        let file = dir.join("sub").join("a.txt");
        let path = file.to_string_lossy().into_owned();
        assert!(matches!(
            write_file(&path, "hello", false).await,
            LocalOutcome::Text {
                is_error: false,
                ..
            }
        ));
        assert!(matches!(
            write_file(&path, " world", true).await,
            LocalOutcome::Text {
                is_error: false,
                ..
            }
        ));
        assert_eq!(
            tokio::fs::read_to_string(&file).await.unwrap(),
            "hello world"
        );
        match list_dir(&dir.join("sub").to_string_lossy()).await {
            LocalOutcome::Text { text, .. } => assert!(text.contains("a.txt")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            list_dir(&dir.join("missing").to_string_lossy()).await,
            LocalOutcome::Text { is_error: true, .. }
        ));
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn powershell_runs_reports_errors_and_times_out() {
        let cancel = CancellationToken::new();
        match run_powershell("Write-Output 'hi there'", 30, &cancel).await {
            LocalOutcome::Text { text, is_error } => {
                assert!(!is_error);
                assert!(text.contains("hi there"));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(run_powershell("exit 3", 30, &cancel).await, LocalOutcome::Text { is_error: true, text } if text.contains("exit code 3"))
        );
        let started = std::time::Instant::now();
        assert!(
            matches!(run_powershell("Start-Sleep 30", 1, &cancel).await, LocalOutcome::Text { is_error: true, text } if text.contains("timed out"))
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        let c2 = CancellationToken::new();
        let c3 = c2.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            c3.cancel();
        });
        assert!(
            matches!(run_powershell("Start-Sleep 30", 60, &c2).await, LocalOutcome::Text { is_error: true, text } if text.contains("stopped"))
        );
    }
}
