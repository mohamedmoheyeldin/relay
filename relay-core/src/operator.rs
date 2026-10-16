//! The operator service: one long-running `relay-screen operator` process that keeps
//! the local model, the app catalog and the window list warm and does every screen
//! task. The app starts it once, hidden, and talks JSON lines over stdin/stdout:
//!
//! - commands: `task`, `say` (a follow-up into a running task), `stop`, `route` (does
//!   this message go to the operator or to Claude), `state`, `quit`;
//! - events, each with the id of the task or request they belong to: `accepted`,
//!   `step`, `note`, `done` / `failed` / `stopped`, `unavailable`, `route`, `state`,
//!   and `timing` (diagnostics, ignored).
//!
//! Steps of `ui_task` calls Claude makes are forwarded by the service with ids it
//! makes up (`"source":"claude"`); they go to the catch-all handler.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::oneshot;

use crate::chat::{ChatEvent, Turn};
use crate::settings::Settings;
use crate::{local_agent, local_ai, mcp, paths, proc, system};

type Sink = Box<dyn Fn(ChatEvent) + Send + Sync>;

struct Task {
    on_event: Sink,
    done: Option<oneshot::Sender<()>>,
}

/// Where the operator says an owner message belongs.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    Operator,
    Claude,
}

pub struct Operator {
    stdin: tokio::sync::Mutex<ChildStdin>,
    child: tokio::sync::Mutex<Child>,
    alive: AtomicBool,
    next: AtomicU64,
    tasks: Mutex<HashMap<String, Task>>,
    replies: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    other: Sink,
}

/// An event ends its task.
fn is_final(event: &str) -> bool {
    matches!(event, "done" | "failed" | "stopped" | "unavailable")
}

/// One event line as chat events for the chat and the popup.
pub fn to_chat(v: &Value) -> Vec<ChatEvent> {
    let s = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match v.get("event").and_then(Value::as_str) {
        Some("step") => {
            // `say` is the step in plain words; older services only had `thought`.
            let say = s("say");
            vec![ChatEvent::LocalStep {
                step: v.get("step").and_then(Value::as_u64).unwrap_or(0),
                thought: if say.is_empty() { s("thought") } else { say },
                action: s("action"),
                result: s("result"),
            }]
        }
        Some("note") => vec![ChatEvent::LocalNote { text: s("text") }],
        Some("accepted" | "timing" | "route" | "state") | None => vec![],
        // done, failed, stopped and unavailable read like the one-shot agent's.
        Some(_) => local_agent::parse_line(&v.to_string()),
    }
}

impl Operator {
    /// Start the service. Events of tasks this app didn't start (Claude's `ui_task`
    /// calls) go to `other`.
    pub async fn start(
        python: &Path,
        settings: &Settings,
        other: impl Fn(ChatEvent) + Send + Sync + 'static,
    ) -> Result<Arc<Self>> {
        let mut cmd = proc::command(local_ai::cli_python(python));
        if let mcp::ServerSpec::Stdio { env, .. } = system::relay_screen_spec(python, settings) {
            cmd.envs(env);
        }
        let workdir = paths::work_dir();
        std::fs::create_dir_all(&workdir)?;
        cmd.current_dir(&workdir)
            .args(["-m", "relay_screen.server", "operator"])
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn().context("could not start the local AI")?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let op = Arc::new(Self {
            stdin: tokio::sync::Mutex::new(stdin),
            child: tokio::sync::Mutex::new(child),
            alive: AtomicBool::new(true),
            next: AtomicU64::new(1),
            tasks: Mutex::default(),
            replies: Mutex::default(),
            other: Box::new(other),
        });
        let reader = op.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    reader.dispatch(&v);
                }
            }
            reader.ended();
        });
        Ok(op)
    }

    fn dispatch(&self, v: &Value) {
        let id = v.get("id").and_then(Value::as_str).unwrap_or_default();
        let event = v.get("event").and_then(Value::as_str).unwrap_or_default();
        if let Some(reply) = self.replies.lock().expect("replies").remove(id) {
            let _ = reply.send(v.clone());
            return;
        }
        let mut tasks = self.tasks.lock().expect("tasks");
        match tasks.get_mut(id) {
            Some(task) => {
                for e in to_chat(v) {
                    (task.on_event)(e);
                }
                if is_final(event) {
                    if let Some(done) = task.done.take() {
                        let _ = done.send(());
                    }
                    tasks.remove(id);
                }
            }
            None => {
                drop(tasks);
                for e in to_chat(v) {
                    (self.other)(e);
                }
            }
        }
    }

    /// The process exited: every open task ends with that.
    fn ended(&self) {
        self.alive.store(false, Ordering::SeqCst);
        self.replies.lock().expect("replies").clear();
        for (_, mut task) in self.tasks.lock().expect("tasks").drain() {
            (task.on_event)(ChatEvent::Error {
                message: "The local AI stopped before finishing.".into(),
            });
            if let Some(done) = task.done.take() {
                let _ = done.send(());
            }
        }
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.next.fetch_add(1, Ordering::SeqCst))
    }

    async fn send(&self, command: Value) -> Result<()> {
        anyhow::ensure!(self.is_alive(), "the local AI isn't running");
        let mut line = command.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn request(&self, op: &str, text: Option<&str>, wait: Duration) -> Option<Value> {
        let id = self.new_id(&op[..1]);
        let (tx, rx) = oneshot::channel();
        self.replies.lock().expect("replies").insert(id.clone(), tx);
        let mut command = json!({ "op": op, "id": id });
        if let Some(text) = text {
            command["text"] = text.into();
        }
        if self.send(command).await.is_err() {
            self.replies.lock().expect("replies").remove(&id);
            return None;
        }
        let reply = tokio::time::timeout(wait, rx).await.ok()?.ok();
        self.replies.lock().expect("replies").remove(&id);
        reply
    }

    /// Whether a message is a screen task for the operator or work for Claude.
    /// No answer in time means Claude, as before the operator existed.
    pub async fn route(&self, text: &str) -> Route {
        let reply = self
            .request("route", Some(text), Duration::from_secs(3))
            .await;
        match reply
            .as_ref()
            .and_then(|v| v.get("to"))
            .and_then(Value::as_str)
        {
            Some("operator") => Route::Operator,
            _ => Route::Claude,
        }
    }

    /// What is on the screen right now (foreground window, browser page, windows).
    pub async fn state(&self) -> Option<Value> {
        self.request("state", None, Duration::from_secs(3)).await
    }

    /// Start a task. Its events go to `on_event`; the handle ends when it finishes.
    pub async fn task(
        self: &Arc<Self>,
        text: &str,
        on_event: impl Fn(ChatEvent) + Send + Sync + 'static,
    ) -> Result<(Turn, tokio::task::JoinHandle<()>)> {
        let id = self.new_id("t");
        let (tx, rx) = oneshot::channel();
        self.tasks.lock().expect("tasks").insert(
            id.clone(),
            Task {
                on_event: Box::new(on_event),
                done: Some(tx),
            },
        );
        if let Err(e) = self
            .send(json!({ "op": "task", "id": id, "text": text }))
            .await
        {
            self.tasks.lock().expect("tasks").remove(&id);
            return Err(e);
        }
        let wait = tokio::spawn(async move {
            let _ = rx.await;
        });
        Ok((Turn::operator(self.clone(), id), wait))
    }

    /// A follow-up or correction for a running task, seen on its next step.
    pub async fn say(&self, id: &str, text: &str) -> Result<()> {
        self.send(json!({ "op": "say", "id": id, "text": text }))
            .await
    }

    pub async fn stop(&self, id: &str) {
        let _ = self.send(json!({ "op": "stop", "id": id })).await;
    }

    /// Ask the service to unload the model and exit; kill it if it doesn't.
    pub async fn quit(&self) {
        let _ = self.send(json!({ "op": "quit" })).await;
        let mut child = self.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .is_err()
        {
            let _ = child.kill().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_show_the_plain_words() {
        let step = to_chat(&json!({
            "id": "t1", "event": "step", "step": 3,
            "say": "Typed \"python\" into Search jobs",
            "action": "type", "result": "ok", "thought": "the box is empty", "ms": 140
        }));
        assert_eq!(
            step,
            vec![ChatEvent::LocalStep {
                step: 3,
                thought: "Typed \"python\" into Search jobs".into(),
                action: "type".into(),
                result: "ok".into(),
            }]
        );
        let old = to_chat(&json!({"event": "step", "step": 1, "thought": "Opening Brave"}));
        assert!(
            matches!(&old[0], ChatEvent::LocalStep { thought, .. } if thought == "Opening Brave")
        );
    }

    #[test]
    fn notes_ends_and_diagnostics() {
        let note =
            to_chat(&json!({"id": "t1", "event": "note", "text": "Loading the local model…"}));
        assert!(matches!(&note[0], ChatEvent::LocalNote { text } if text.starts_with("Loading")));
        let done =
            to_chat(&json!({"id": "t1", "event": "done", "message": "Searched.", "steps": 4}));
        assert!(
            matches!(&done[0], ChatEvent::Done { is_error: false, result, .. } if result == "Searched. (4 steps)")
        );
        let failed =
            to_chat(&json!({"id": "t1", "event": "failed", "message": "No results.", "steps": 2}));
        assert!(matches!(&failed[0], ChatEvent::Done { is_error: true, .. }));
        assert!(to_chat(&json!({"id": "t1", "event": "timing", "load_ms": 900})).is_empty());
        assert!(to_chat(&json!({"id": "t1", "event": "accepted", "text": "x"})).is_empty());
        assert!(is_final("stopped") && is_final("unavailable") && !is_final("step"));
    }
}
