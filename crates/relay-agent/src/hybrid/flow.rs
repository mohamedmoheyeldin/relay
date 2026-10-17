//! Runs that are not "plan once, then execute": written answers and browser tasks.

use relay_ai::AiError;
use relay_orchestrator::router::Strategy;
use relay_orchestrator::PrivacyMode;
use relay_protocol::{ErrorKind, RunId, RunOutcome as UiOutcome, UiEvent};
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::web::WebEngine;
use super::{now_ms, BridgeConfirmer, Hybrid, WorkflowRecord, HISTORY_TURNS};

impl Hybrid {
    pub(super) async fn run_conversational(
        &self,
        run_id: RunId,
        conv_id: &str,
        task: &str,
        history: &[(String, String)],
        strategy: Strategy,
        cancel: &CancellationToken,
    ) {
        let started = now_ms();
        let settings = self.settings();
        let (strategy_name, provider) = match &strategy {
            Strategy::ChatLocal => ("chat_local", None),
            Strategy::ChatCloud(p) => ("chat_cloud", Some(p.as_str().to_string())),
            Strategy::WebLocal => ("web_local", None),
            Strategy::WebCloud(p) => ("web_cloud", Some(p.as_str().to_string())),
            _ => ("other", None),
        };
        let mut record = WorkflowRecord {
            id: uuid::Uuid::new_v4().simple().to_string(),
            conv_id: Some(conv_id.to_string()),
            objective: task.chars().take(300).collect(),
            strategy: strategy_name.into(),
            provider,
            outcome: "failed".into(),
            steps_total: 0,
            steps_verified: 0,
            created_at: started,
            updated_at: started,
            plan_source: String::new(),
            steps: Vec::new(),
            shared: Vec::new(),
            shared_bytes: 0,
            approvals: Vec::new(),
            errors: Vec::new(),
            notes: None,
        };
        let mut outcome = UiOutcome::Completed;
        let text: String = match strategy {
            Strategy::ChatLocal | Strategy::ChatCloud(_) => {
                record.plan_source = if matches!(strategy, Strategy::ChatLocal) {
                    "local".into()
                } else {
                    record.provider.clone().unwrap_or_default()
                };
                let answer = match strategy {
                    Strategy::ChatCloud(kind) => {
                        match self.chat_cloud(kind, run_id, task, history, cancel).await {
                            Ok(t) => Ok(t),
                            Err(AiError::Cancelled) => Err("stopped".to_string()),
                            Err(e) => {
                                warn!("{} could not answer: {e}", kind.label());
                                let (_, message) = crate::agent::error_to_ui(&e);
                                let local_ok = self.local_model().await.is_ok();
                                if local_ok && !cancel.is_cancelled() {
                                    self.emit(UiEvent::Notice {
                                        text: format!("{} could not answer ({message}); the local AI is answering instead.", kind.label()),
                                    });
                                    self.chat_local(run_id, task, history, cancel).await
                                } else {
                                    Err(message)
                                }
                            }
                        }
                    }
                    _ => self.chat_local(run_id, task, history, cancel).await,
                };
                match answer {
                    Ok(t) => {
                        record.outcome = "completed".into();
                        t
                    }
                    Err(e) if cancel.is_cancelled() || e == "stopped" => {
                        outcome = UiOutcome::Cancelled;
                        record.outcome = "cancelled".into();
                        String::new()
                    }
                    Err(e) => {
                        outcome = UiOutcome::Failed;
                        record.errors.push(e.clone());
                        self.emit(UiEvent::Error {
                            run_id: Some(run_id),
                            kind: ErrorKind::Cli,
                            message: e.clone(),
                        });
                        format!("I couldn't answer that: {e}")
                    }
                }
            }
            Strategy::WebLocal | Strategy::WebCloud(_) => {
                let engine = match strategy {
                    Strategy::WebCloud(kind) if settings.privacy != PrivacyMode::LocalOnly => {
                        WebEngine::Cloud(kind)
                    }
                    _ => WebEngine::Local,
                };
                record.plan_source = match engine {
                    WebEngine::Local => "local".into(),
                    WebEngine::Cloud(k) => k.as_str().into(),
                };
                self.emit(UiEvent::PlanProposed {
                    run_id,
                    source: record.plan_source.clone(),
                    objective: task.chars().take(200).collect(),
                    steps: Vec::new(),
                    notes: Some("Relay works in the browser one step at a time and asks before anything irreversible.".into()),
                });
                let confirmer = BridgeConfirmer {
                    run_id,
                    bridge: self.bridge.clone(),
                };
                let r = self
                    .run_web(engine, run_id, task, history, &confirmer, cancel)
                    .await;
                outcome = r.outcome;
                record.outcome = r.record_outcome.into();
                record.steps = r.steps;
                record.shared = r.shared;
                record.shared_bytes = r.bytes;
                r.text
            }
            _ => String::new(),
        };

        self.bridge.deny_all();
        record.steps_total = record.steps.len();
        record.steps_verified = record
            .steps
            .iter()
            .filter(|s| s.status == "verified")
            .count();
        record.updated_at = now_ms();
        self.workflows.save(&record);
        if !text.is_empty() {
            self.emit(UiEvent::AssistantMessage {
                run_id,
                text: text.clone(),
            });
        }
        let mut hist = history.to_vec();
        hist.push((task.to_string(), text));
        let excess = hist.len().saturating_sub(HISTORY_TURNS);
        hist.drain(..excess);
        let session = self.conv_store.load(conv_id).session_id;
        self.conv_store.save(conv_id, session.as_deref(), &hist);
        *Self::lock(&self.active) = None;
        self.emit(UiEvent::RunFinished { run_id, outcome });
    }
}
