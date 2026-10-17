//! MCP over stdio: newline-delimited JSON-RPC 2.0. Standard output carries protocol messages only;
//! everything else goes to the log file.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use relay_mcp::{Content, ToolOutput};
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::toolbox::{Toolbox, INSTRUCTIONS};

const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];
const SERVER_NAME: &str = "relay";

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_result(out: &ToolOutput) -> Value {
    let parts: Vec<Value> = out
        .content
        .iter()
        .map(|c| match c {
            Content::Text(t) => json!({"type": "text", "text": t}),
            Content::Image { png_base64 } => {
                json!({"type": "image", "data": png_base64, "mimeType": "image/png"})
            }
        })
        .collect();
    json!({"content": parts, "isError": out.is_error})
}

type Pending = Arc<Mutex<HashMap<String, CancellationToken>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Serve one client until its input closes.
pub async fn serve<R, W>(toolbox: Arc<Toolbox>, reader: R, mut writer: W)
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Send + Unpin + 'static,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let out_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let mut line = message.to_string();
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                break;
            }
        }
    });

    let pending: Pending = Arc::default();
    let mut tasks = JoinSet::new();
    let mut lines = reader.lines();
    loop {
        let line = match lines.next_line().await {
            Ok(Some(l)) => l,
            Ok(None) => break,
            Err(e) => {
                warn!("reading stdin failed: {e}");
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            let _ = tx.send(error(&Value::Null, -32700, "parse error"));
            continue;
        };
        let items = match message {
            Value::Array(items) => items,
            single => vec![single],
        };
        for item in items {
            handle(&toolbox, item, &tx, &pending, &mut tasks);
        }
    }
    // The client went away: stop anything still running.
    for token in lock(&pending).values() {
        token.cancel();
    }
    while tasks.join_next().await.is_some() {}
    drop(tx);
    let _ = out_task.await;
}

fn handle(
    toolbox: &Arc<Toolbox>,
    message: Value,
    tx: &mpsc::UnboundedSender<Value>,
    pending: &Pending,
    tasks: &mut JoinSet<()>,
) {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = message.get("id").cloned() else {
        // Notification.
        if method == "notifications/cancelled" {
            if let Some(rid) = params.get("requestId") {
                if let Some(token) = lock(pending).get(&rid.to_string()) {
                    token.cancel();
                }
            }
        }
        debug!("notification {method}");
        return;
    };
    match method.as_str() {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = if SUPPORTED.contains(&asked) {
                asked
            } else {
                SUPPORTED[0]
            };
            let _ = tx.send(reply(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }),
            ));
        }
        "ping" => {
            let _ = tx.send(reply(&id, json!({})));
        }
        "tools/list" => {
            let _ = tx.send(reply(&id, json!({"tools": toolbox.list()})));
        }
        "tools/call" => {
            let Some(name) = params
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                let _ = tx.send(error(&id, -32602, "missing tool name"));
                return;
            };
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            let token = CancellationToken::new();
            let key = id.to_string();
            lock(pending).insert(key.clone(), token.clone());
            let (toolbox, tx, pending) = (toolbox.clone(), tx.clone(), pending.clone());
            tasks.spawn(async move {
                let out = toolbox.call(&name, &args, &token).await;
                lock(&pending).remove(&key);
                let _ = tx.send(reply(&id, tool_result(&out)));
            });
        }
        _ => {
            let _ = tx.send(error(&id, -32601, "method not found"));
        }
    }
}
