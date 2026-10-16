//! Full test runs from the app (D-096): `relay-screen diagnostics --json`, which drives
//! the local operator through real tasks and records everything in
//! Downloads\Relay-diagnostics\<time>\. The app shows its progress and the newest
//! summary; Claude reads the same files.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::download::Cancel;
use crate::settings::Settings;
use crate::{local_ai, mcp, paths, proc, system};

/// Where every run's folder goes: everything Relay makes on the PC is in Downloads.
pub fn root() -> PathBuf {
    paths::downloads_dir().join("Relay-diagnostics")
}

/// The newest run's summary.json, if there is one.
pub fn latest() -> Option<Value> {
    latest_in(&root())
}

pub fn latest_in(root: &Path) -> Option<Value> {
    let pointer: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("latest.json")).ok()?).ok()?;
    let summary = pointer.get("summary")?.as_str()?;
    serde_json::from_str(&std::fs::read_to_string(summary).ok()?).ok()
}

/// Run the test. Each JSON event line goes to `on_event`; `cancel` ends the run.
/// Returns the summary event, or an error when the run could not finish.
pub async fn run(
    python: &Path,
    settings: &Settings,
    only: &[String],
    cancel: &Cancel,
    on_event: impl Fn(Value) + Send + Sync + 'static,
) -> Result<Value> {
    let mut cmd = proc::command(local_ai::cli_python(python));
    if let mcp::ServerSpec::Stdio { env, .. } = system::relay_screen_spec(python, settings) {
        cmd.envs(env);
    }
    let workdir = paths::work_dir();
    std::fs::create_dir_all(&workdir)?;
    cmd.current_dir(&workdir)
        .args(["-m", "relay_screen.server", "diagnostics", "--json"])
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if !only.is_empty() {
        cmd.arg("--only").args(only);
    }
    let mut child = cmd.spawn().context("could not start the test run")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let mut lines = BufReader::new(stdout).lines();
    let mut summary: Option<Value> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            anyhow::bail!("Stopped.");
        }
        match tokio::time::timeout(Duration::from_millis(300), lines.next_line()).await {
            Err(_) => continue,
            Ok(Ok(Some(line))) => {
                let Ok(event) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if event.get("event").and_then(Value::as_str) == Some("summary") {
                    summary = Some(event.clone());
                }
                on_event(event);
            }
            Ok(_) => break,
        }
    }
    let _ = child.wait().await;
    summary.context("the test run ended without a summary")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_follows_the_pointer() {
        let dir = std::env::temp_dir().join(format!("relay-diag-{}", std::process::id()));
        let run = dir.join("2026-10-16_02-00-00");
        std::fs::create_dir_all(&run).unwrap();
        assert!(latest_in(&dir).is_none());
        std::fs::write(run.join("summary.json"), r#"{"passed": 3, "failed": 1}"#).unwrap();
        let pointer = serde_json::json!({"summary": run.join("summary.json")});
        std::fs::write(dir.join("latest.json"), pointer.to_string()).unwrap();
        assert_eq!(latest_in(&dir).unwrap()["passed"], 3);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
