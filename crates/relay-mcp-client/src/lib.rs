//! A minimal MCP client over stdio: start a server process, list its tools, call them.
//!
//! Relay uses it to drive tool servers (the Playwright browser server first). Server output is
//! untrusted data: it is returned as text and never interpreted here.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex as AsyncMutex};
use tracing::{debug, warn};

const PROTOCOL: &str = "2025-06-18";

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("could not start {0}: {1}")]
    Spawn(String, String),
    #[error("{0} did not answer in time")]
    Timeout(String),
    #[error("{0} stopped unexpectedly")]
    Closed(String),
    #[error("{0} reported an error: {1}")]
    Server(String, String),
}

/// How to start a server.
#[derive(Debug, Clone, Default)]
pub struct ServerCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

/// What a tool call returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolResult {
    pub text: String,
    /// PNG/JPEG images as base64 (not interpreted here).
    pub images: Vec<String>,
    pub is_error: bool,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

pub struct McpClient {
    name: String,
    stdin: AsyncMutex<ChildStdin>,
    pending: Pending,
    next: AtomicU64,
    child: Mutex<Option<Child>>,
    tools: Vec<ToolInfo>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl McpClient {
    /// Start the server, complete the handshake and list its tools.
    pub async fn start(
        name: &str,
        cmd: &ServerCommand,
        startup: Duration,
    ) -> Result<Arc<Self>, McpError> {
        let mut command = Command::new(&cmd.program);
        command
            .args(&cmd.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for (k, v) in &cmd.env {
            command.env(k, v);
        }
        if let Some(cwd) = &cmd.cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command
            .spawn()
            .map_err(|e| McpError::Spawn(name.to_string(), e.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn(name.to_string(), "no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn(name.to_string(), "no stdout".into()))?;
        let pending: Pending = Arc::default();
        {
            let pending = pending.clone();
            let name = name.to_string();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        debug!("{name}: non-JSON line ignored");
                        continue;
                    };
                    if let Some(id) = msg.get("id").and_then(Value::as_u64) {
                        if msg.get("method").is_none() {
                            if let Some(tx) = lock(&pending).remove(&id) {
                                let _ = tx.send(msg);
                            }
                        }
                    }
                }
                // The server went away: wake everything waiting.
                lock(&pending).clear();
            });
        }
        let mut client = Self {
            name: name.to_string(),
            stdin: AsyncMutex::new(stdin),
            pending,
            next: AtomicU64::new(1),
            child: Mutex::new(Some(child)),
            tools: Vec::new(),
        };
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL,
                    "capabilities": {},
                    "clientInfo": {"name": "relay", "version": env!("CARGO_PKG_VERSION")}
                }),
                startup,
            )
            .await?;
        client.notify("notifications/initialized").await?;
        let listed = client.request("tools/list", json!({}), startup).await?;
        client.tools = serde_json::from_value(listed["tools"].clone()).unwrap_or_default();
        Ok(Arc::new(client))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tools(&self) -> &[ToolInfo] {
        &self.tools
    }

    pub fn has_tool(&self, tool: &str) -> bool {
        self.tools.iter().any(|t| t.name == tool)
    }

    async fn send(&self, message: Value) -> Result<(), McpError> {
        let mut line = message.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| McpError::Closed(self.name.clone()))?;
        stdin
            .flush()
            .await
            .map_err(|_| McpError::Closed(self.name.clone()))
    }

    async fn notify(&self, method: &str) -> Result<(), McpError> {
        self.send(json!({"jsonrpc": "2.0", "method": method})).await
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, McpError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        if let Err(e) = self
            .send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await
        {
            lock(&self.pending).remove(&id);
            return Err(e);
        }
        let reply = match tokio::time::timeout(timeout, rx).await {
            Err(_) => {
                lock(&self.pending).remove(&id);
                return Err(McpError::Timeout(self.name.clone()));
            }
            Ok(Err(_)) => return Err(McpError::Closed(self.name.clone())),
            Ok(Ok(v)) => v,
        };
        if let Some(err) = reply.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(McpError::Server(self.name.clone(), message.to_string()));
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Call a tool. A tool that reports `isError` still returns `Ok` with `is_error` set.
    pub async fn call(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<ToolResult, McpError> {
        let result = self
            .request(
                "tools/call",
                json!({"name": tool, "arguments": arguments}),
                timeout,
            )
            .await?;
        let mut out = ToolResult {
            is_error: result["isError"].as_bool().unwrap_or(false),
            ..ToolResult::default()
        };
        if let Some(parts) = result["content"].as_array() {
            for part in parts {
                match part["type"].as_str() {
                    Some("text") => {
                        if !out.text.is_empty() {
                            out.text.push('\n');
                        }
                        out.text.push_str(part["text"].as_str().unwrap_or(""));
                    }
                    Some("image") => {
                        if let Some(d) = part["data"].as_str() {
                            out.images.push(d.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(out)
    }

    /// Stop the server process.
    pub async fn shutdown(&self) {
        let child = lock(&self.child).take();
        if let Some(mut child) = child {
            if let Err(e) = child.kill().await {
                warn!("{}: could not stop the server: {e}", self.name);
            }
        }
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Some(child) = lock(&self.child).as_mut() {
            let _ = child.start_kill();
        }
    }
}
