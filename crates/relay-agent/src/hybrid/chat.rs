//! Questions and requests for text or code: answered in words, streamed, and nothing is run.

use std::time::Duration;

use relay_ai::{AiError, AiEvent, Finish, RunRequest, SessionMode};
use relay_orchestrator::ProviderKind;
use relay_protocol::{RunId, UiEvent};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::{Hybrid, Purpose, PLAN_TIMEOUTS};
use crate::local::ChatMessage;

pub const CHAT_SYSTEM: &str = "\
You are Relay, a helpful assistant on the owner's computer. Answer the message directly and well. \
Use Markdown. Put any code in fenced code blocks with the language name (```python, ```typescript, ```powershell, ...), \
complete and ready to copy, with short comments where they help. Prefer concise answers with one good example over long \
lists. Do not claim to have opened, run or changed anything: in this mode you only answer.";

fn chat_prompt(task: &str, history: &[(String, String)]) -> String {
    let mut p = String::new();
    if !history.is_empty() {
        p.push_str("Earlier in this conversation:\n");
        for (a, r) in history.iter().rev().take(4).rev() {
            let clip = |s: &str, n: usize| s.chars().take(n).collect::<String>();
            p.push_str(&format!(
                "- Owner: {}\n  You: {}\n",
                clip(a, 300),
                clip(r, 600)
            ));
        }
        p.push('\n');
    }
    p.push_str(task);
    p
}

impl Hybrid {
    /// Answer with the local model, streaming the text.
    pub(super) async fn chat_local(
        &self,
        run_id: RunId,
        task: &str,
        history: &[(String, String)],
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        let model = self.local_model().await?;
        let messages = vec![
            ChatMessage::new("system", CHAT_SYSTEM),
            ChatMessage::new("user", chat_prompt(task, history)),
        ];
        let sink = self.sink.clone();
        let client = self.local.client();
        tokio::select! {
            _ = cancel.cancelled() => Err("stopped".into()),
            r = client.chat_stream(&model, &messages, Duration::from_secs(300), |piece| {
                sink(UiEvent::AssistantDelta { run_id, text: piece.to_string() });
            }) => r.map_err(|e| e.to_string()),
        }
    }

    /// Answer with a cloud provider, retrying once after a transient failure that streamed nothing.
    pub(super) async fn chat_cloud(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        task: &str,
        history: &[(String, String)],
        cancel: &CancellationToken,
    ) -> Result<String, AiError> {
        match self
            .chat_cloud_once(kind, run_id, task, history, cancel)
            .await
        {
            Err(e) if super::is_transient(&e) && !cancel.is_cancelled() => {
                tracing::info!("{} hit a transient error, retrying: {e}", kind.label());
                tokio::time::sleep(Duration::from_secs(2)).await;
                let r = self
                    .chat_cloud_once(kind, run_id, task, history, cancel)
                    .await;
                if r.is_ok() {
                    self.mark_cloud_used(kind, Purpose::Answer);
                }
                r
            }
            Ok(t) => {
                self.mark_cloud_used(kind, Purpose::Answer);
                Ok(t)
            }
            Err(e) => Err(e),
        }
    }

    /// Answer with a cloud provider (no tools), streaming the text when the provider does.
    async fn chat_cloud_once(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        task: &str,
        history: &[(String, String)],
        cancel: &CancellationToken,
    ) -> Result<String, AiError> {
        let (model, reasoning) = self.provider_choice(kind, Purpose::Answer);
        let request = RunRequest {
            run_id,
            prompt: chat_prompt(task, history),
            session: SessionMode::New(uuid::Uuid::new_v4().to_string()),
            work_dir: self.work_dir.clone(),
            mcp_servers: json!({}),
            permission_prompt_tool: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            system_prompt_append: Some(CHAT_SYSTEM.to_string()),
            model,
            reasoning,
            output_schema: None,
            plan_only: true,
            timeouts: PLAN_TIMEOUTS,
        };
        let mut handle = self.provider(kind).run(request).await?;
        let mut streamed = String::new();
        let mut held = String::new();
        let mut streamed_any = false;
        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    handle.cancel();
                }
                event = handle.events.recv() => match event {
                    None => return Err(AiError::Io("the provider ended unexpectedly".into())),
                    Some(AiEvent::TextDelta(t)) => {
                        streamed_any = true;
                        streamed.push_str(&t);
                        self.emit(UiEvent::AssistantDelta { run_id, text: t });
                    }
                    // Whole blocks (providers without token streaming, and CLI error text) are held
                    // back until the run succeeds, so a failure never shows up as an answer.
                    Some(AiEvent::Text(t)) => held.push_str(&t),
                    Some(AiEvent::Finished(Finish::Completed { result, .. })) => {
                        let text = if !result.trim().is_empty() {
                            result
                        } else if !streamed.is_empty() {
                            streamed
                        } else {
                            held
                        };
                        if !streamed_any {
                            self.emit(UiEvent::AssistantDelta { run_id, text: text.clone() });
                        }
                        return Ok(text);
                    }
                    Some(AiEvent::Finished(Finish::Cancelled)) => return Err(AiError::Cancelled),
                    Some(AiEvent::Finished(Finish::TimedOut(k))) => return Err(AiError::Timeout(k)),
                    Some(AiEvent::Finished(Finish::Failed(e))) => return Err(e),
                    Some(AiEvent::Finished(Finish::Escalate { reason, .. })) => {
                        return Err(AiError::Cli { message: reason, exit_code: None })
                    }
                    Some(_) => {}
                }
            }
        }
    }
}
