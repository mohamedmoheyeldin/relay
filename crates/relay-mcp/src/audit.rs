//! Append-only activity log: one JSON line per tool call. Local only, never committed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use relay_protocol::RunId;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tracing::warn;

use crate::safety::truncate;

pub struct AuditLog {
    path: PathBuf,
    seq: AtomicU64,
}

/// Keep logged inputs short; typed text may be long or sensitive.
fn shorten_input(args: &Value) -> Value {
    match args {
        Value::String(s) => Value::String(truncate(s, 200)),
        Value::Array(a) => Value::Array(a.iter().map(shorten_input).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), shorten_input(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

impl AuditLog {
    /// Log to `<dir>/actions.jsonl`, creating the directory on first write.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            path: dir.into().join("actions.jsonl"),
            seq: AtomicU64::new(0),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Record one call. Failures are logged and never block the action.
    pub async fn record(
        &self,
        run_id: RunId,
        tool: &str,
        args: &Value,
        result: &str,
        is_error: bool,
        took: Duration,
    ) {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default();
        let line = json!({
            "seq": self.seq.fetch_add(1, Ordering::SeqCst) + 1,
            "unix_ms": unix_ms,
            "run": run_id,
            "tool": tool,
            "input": shorten_input(args),
            "result": result,
            "error": is_error,
            "ms": took.as_millis() as u64,
        });
        if let Some(parent) = self.path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                warn!("audit: cannot create {}: {e}", parent.display());
                return;
            }
        }
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await;
        match file {
            Ok(mut f) => {
                if let Err(e) = f.write_all(format!("{line}\n").as_bytes()).await {
                    warn!("audit: write failed: {e}");
                }
            }
            Err(e) => warn!("audit: cannot open {}: {e}", self.path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_one_line_per_call_in_order() {
        let dir = std::env::temp_dir().join(format!("relay-audit-{}", std::process::id()));
        let log = AuditLog::new(&dir);
        log.record(
            7,
            "type",
            &json!({"text": "x".repeat(500)}),
            "OK",
            false,
            Duration::from_millis(5),
        )
        .await;
        log.record(
            7,
            "left_click",
            &json!({"coordinate":[1,2]}),
            "denied",
            true,
            Duration::ZERO,
        )
        .await;
        let content = tokio::fs::read_to_string(log.path()).await.unwrap();
        let lines: Vec<Value> = content
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["seq"], 1);
        assert_eq!(lines[1]["seq"], 2);
        assert_eq!(lines[1]["error"], true);
        assert_eq!(lines[0]["run"], 7);
        let typed = lines[0]["input"]["text"].as_str().unwrap();
        assert!(typed.chars().count() <= 201);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
