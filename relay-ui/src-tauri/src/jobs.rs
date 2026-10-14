//! Long-running jobs (installs, downloads) with progress events and cancel.
//!
//! Every job emits `job` events: `running` with progress or a log line, then one of
//! `done` or `error`. The UI matches them by job id.

use std::future::Future;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use relay_core::download::Cancel;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct JobEvent {
    pub job: String,
    pub state: &'static str,
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub line: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct Reporter {
    app: AppHandle,
    job: String,
}

impl Reporter {
    fn emit(&self, event: JobEvent) {
        let _ = self.app.emit("job", event);
    }

    pub fn progress(&self, stage: &str, done: u64, total: u64) {
        self.emit(JobEvent {
            job: self.job.clone(),
            state: "running",
            stage: stage.into(),
            done,
            total,
            line: None,
            error: None,
        });
    }

    pub fn line(&self, line: String) {
        self.emit(JobEvent {
            job: self.job.clone(),
            state: "running",
            stage: String::new(),
            done: 0,
            total: 0,
            line: Some(line),
            error: None,
        });
    }
}

/// Start `work` as job `id` unless one with that id is already running.
pub fn spawn<F, Fut>(app: &AppHandle, id: &str, work: F) -> Result<(), String>
where
    F: FnOnce(Reporter, Cancel) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let cancel = Cancel::default();
    {
        let state = app.state::<AppState>();
        let mut jobs = state.jobs.lock().expect("jobs lock");
        if jobs.contains_key(id) {
            return Err("This is already running.".into());
        }
        jobs.insert(id.to_string(), cancel.clone());
    }
    let reporter = Reporter {
        app: app.clone(),
        job: id.to_string(),
    };
    let app = app.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let result = work(reporter.clone(), cancel).await;
        app.state::<AppState>()
            .jobs
            .lock()
            .expect("jobs lock")
            .remove(&id);
        reporter.emit(JobEvent {
            job: id,
            state: if result.is_ok() { "done" } else { "error" },
            stage: String::new(),
            done: 0,
            total: 0,
            line: None,
            error: result.err().map(|e| format!("{e:#}")),
        });
    });
    Ok(())
}

pub fn cancel(app: &AppHandle, id: &str) {
    if let Some(flag) = app
        .state::<AppState>()
        .jobs
        .lock()
        .expect("jobs lock")
        .get(id)
    {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn running(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>()
        .jobs
        .lock()
        .expect("jobs lock")
        .keys()
        .cloned()
        .collect()
}
