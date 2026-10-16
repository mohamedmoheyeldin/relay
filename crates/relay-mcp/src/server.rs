//! A minimal MCP server over streamable HTTP (JSON responses only), bound to loopback.
//!
//! Each run gets its own bearer token so a tool call is always attributed to a run, and tokens
//! die with the run.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use relay_protocol::RunId;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::host::{CallCtx, Content, ToolHost, ToolOutput};
use crate::tools::{tool_specs, SERVER_NAME};

const DEFAULT_PROTOCOL: &str = "2025-03-26";

const INSTRUCTIONS: &str = "Relay desktop tools. Take a screenshot before acting and after anything that may change the screen. Coordinates are pixels of the latest screenshot. Ask the owner (ask_user) before anything destructive or irreversible. If a tool says Relay was stopped, stop and report; do not work around it.";

/// Credentials one run uses to reach the server.
#[derive(Debug, Clone)]
pub struct RunGrant {
    pub url: String,
    pub token: String,
}

impl RunGrant {
    /// The `mcpServers` entry that makes the AI CLI connect with this grant.
    pub fn server_config(&self) -> Value {
        json!({
            SERVER_NAME: {
                "type": "http",
                "url": self.url,
                "headers": { "Authorization": format!("Bearer {}", self.token) }
            }
        })
    }
}

type Tokens = Arc<Mutex<HashMap<String, RunId>>>;

#[derive(Clone)]
struct AppState {
    host: Arc<ToolHost>,
    tokens: Tokens,
}

/// The running server.
pub struct McpServer {
    addr: SocketAddr,
    tokens: Tokens,
    shutdown: CancellationToken,
}

impl McpServer {
    /// Bind to a random loopback port and start serving.
    pub async fn start(host: Arc<ToolHost>) -> std::io::Result<Self> {
        let tokens: Tokens = Arc::default();
        let state = AppState {
            host,
            tokens: tokens.clone(),
        };
        let app = Router::new()
            .route("/mcp", post(handle).get(not_allowed).delete(not_allowed))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let addr = listener.local_addr()?;
        let shutdown = CancellationToken::new();
        let stop = shutdown.clone();
        tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async move {
                stop.cancelled().await;
            });
            if let Err(e) = serve.await {
                debug!("MCP server stopped: {e}");
            }
        });
        info!("MCP server listening on {addr}");
        Ok(Self {
            addr,
            tokens,
            shutdown,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Create credentials for a run.
    pub fn grant(&self, run_id: RunId) -> RunGrant {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        if let Ok(mut map) = self.tokens.lock() {
            map.insert(token.clone(), run_id);
        }
        RunGrant {
            url: format!("http://{}/mcp", self.addr),
            token,
        }
    }

    /// Invalidate a run's credentials.
    pub fn revoke(&self, grant: &RunGrant) {
        if let Ok(mut map) = self.tokens.lock() {
            map.remove(&grant.token);
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

async fn not_allowed() -> StatusCode {
    StatusCode::METHOD_NOT_ALLOWED
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

async fn handle(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    // Browsers always send Origin; the AI CLI never does. Refuse web pages (DNS rebinding).
    if headers.contains_key(header::ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let run_id =
        bearer(&headers).and_then(|t| state.tokens.lock().ok().and_then(|m| m.get(t).copied()));
    let Some(run_id) = run_id else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(message) = serde_json::from_slice::<Value>(&body) else {
        return rpc_response(Some(error_reply(Value::Null, -32700, "parse error")));
    };
    let ctx = CallCtx { run_id };
    let reply = match message {
        Value::Array(items) => {
            let mut replies = Vec::new();
            for item in items {
                if let Some(r) = handle_message(&state.host, &ctx, item).await {
                    replies.push(r);
                }
            }
            if replies.is_empty() {
                None
            } else {
                Some(Value::Array(replies))
            }
        }
        single => handle_message(&state.host, &ctx, single).await,
    };
    rpc_response(reply)
}

fn rpc_response(reply: Option<Value>) -> Response {
    match reply {
        Some(value) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            value.to_string(),
        )
            .into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn error_reply(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn content_json(output: &ToolOutput) -> Value {
    let parts: Vec<Value> = output
        .content
        .iter()
        .map(|c| match c {
            Content::Text(t) => json!({"type": "text", "text": t}),
            Content::Image { png_base64 } => {
                json!({"type": "image", "data": png_base64, "mimeType": "image/png"})
            }
        })
        .collect();
    json!({"content": parts, "isError": output.is_error})
}

/// Handle one JSON-RPC message; `None` means it was a notification.
pub async fn handle_message(host: &ToolHost, ctx: &CallCtx, message: Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = match id {
        Some(id) => id,
        None => return None, // notification (e.g. notifications/initialized)
    };
    let ok = |result: Value| Some(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(DEFAULT_PROTOCOL);
            ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS
            }))
        }
        "ping" => ok(json!({})),
        "tools/list" => {
            // With approvals off there is nothing to ask, so the AI is not offered the tool.
            let asking = host.approval_mode() != crate::ApprovalMode::Off;
            let tools: Vec<Value> = tool_specs()
                .into_iter()
                .filter(|s| asking || s.name != "ask_user")
                .map(|s| {
                    json!({"name": s.name, "description": s.description, "inputSchema": s.schema})
                })
                .collect();
            ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error_reply(id, -32602, "missing tool name"));
            };
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            let output = host.call(ctx, name, &args).await;
            ok(content_json(&output))
        }
        _ => Some(error_reply(id, -32601, "method not found")),
    }
}
