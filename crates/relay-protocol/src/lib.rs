//! Types shared by the Relay backend and the Leptos UI.
//!
//! Everything here is plain serde data so it compiles for both native and wasm.

use serde::{Deserialize, Serialize};

/// Identifies one user task from submission to completion.
pub type RunId = u64;

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

/// Broad category of a user-visible error, so the UI can offer the right hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    CliNotInstalled,
    NotLoggedIn,
    Timeout,
    Cancelled,
    Cli,
    Internal,
}

/// How risky an action is, shown on the confirmation card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    /// Changes something but can be undone.
    Reversible,
    /// Deletes, sends, submits or otherwise cannot be taken back.
    Destructive,
}

/// A request for the user to approve an action before it happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmRequest {
    pub id: u64,
    pub run_id: RunId,
    pub title: String,
    pub detail: String,
    pub risk: Risk,
}

/// The user's answer to a [`ConfirmRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmAnswer {
    pub id: u64,
    pub approved: bool,
}

/// Snapshot of whether Relay can currently do work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusInfo {
    pub provider: String,
    pub ready: bool,
    pub detail: String,
    pub busy: bool,
    pub kill_hotkey: String,
    /// The local model that would run now, if any.
    #[serde(default)]
    pub local_model: Option<String>,
    /// `auto`, `local_only` or `cloud_only`.
    #[serde(default)]
    pub route_mode: String,
}

/// One step of a proposed plan, as shown in the task timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStepInfo {
    pub id: String,
    pub title: String,
    /// Tool name, e.g. `open_app`.
    pub tool: String,
    /// `safe`, `sensitive` or `destructive`.
    pub risk: String,
    /// How the step runs: `native` (Relay's own tools) or `mcp` (an MCP server such as the browser).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// The MCP server's name when `via` is `mcp`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// True only when the step touches real sensitive information (passwords, ID or card numbers, key files).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive_data: bool,
}

/// A category of local information that was included in a cloud request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedInfo {
    pub category: String,
    pub items: usize,
}

/// Events pushed from the backend to the chat window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    RunStarted {
        run_id: RunId,
        /// The conversation the task belongs to.
        conv_id: String,
    },
    /// Streaming fragment of the assistant's reply.
    AssistantDelta {
        run_id: RunId,
        text: String,
    },
    /// A complete assistant message (replaces accumulated deltas for that turn).
    AssistantMessage {
        run_id: RunId,
        text: String,
    },
    ToolStarted {
        run_id: RunId,
        tool_id: String,
        name: String,
        summary: String,
    },
    ToolFinished {
        run_id: RunId,
        tool_id: String,
        ok: bool,
        summary: String,
    },
    Confirm(ConfirmRequest),
    /// The confirmation was answered, timed out or abandoned; remove the card.
    ConfirmResolved {
        id: u64,
        approved: bool,
    },
    RunFinished {
        run_id: RunId,
        outcome: RunOutcome,
    },
    Error {
        run_id: Option<RunId>,
        kind: ErrorKind,
        message: String,
    },
    Status(StatusInfo),
    /// How the task will be carried out and why (`native`, `local_plan`, `cloud_plan`,
    /// `cloud_agent` or `unavailable`).
    Route {
        run_id: RunId,
        strategy: String,
        provider: Option<String>,
        reason: String,
        /// `chat` (a question: answered in words), `action` (do something on this PC) or `web` (work in a browser).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intent: Option<String>,
    },
    /// A plan was proposed by an AI (or built natively). It is a proposal, not a result.
    PlanProposed {
        run_id: RunId,
        /// `native`, `local`, `claude` or `codex`.
        source: String,
        objective: String,
        steps: Vec<PlanStepInfo>,
        notes: Option<String>,
    },
    StepStarted {
        run_id: RunId,
        step_id: String,
    },
    /// `status` is `verified`, `done`, `failed` or `skipped`. Only `verified` means the effect was checked.
    StepFinished {
        run_id: RunId,
        step_id: String,
        status: String,
        summary: String,
    },
    /// What local information went into a cloud request (the privacy ledger).
    ContextShared {
        run_id: RunId,
        provider: String,
        categories: Vec<SharedInfo>,
        bytes: usize,
    },
    /// A component did part of this task: `local_ai`, `cloud_ai`, `native` or `mcp`. Sent once per label per run.
    Used {
        run_id: RunId,
        kind: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// A step was added to a task that grows while it runs (browser tasks).
    StepAdded {
        run_id: RunId,
        step: PlanStepInfo,
    },
    /// Which engine is working on the task now (`local` or `cloud`).
    Engine {
        run_id: RunId,
        kind: String,
        name: String,
    },
    /// A short note for the transcript (for example, a hand-over to the cloud engine).
    Notice {
        text: String,
    },
    /// Progress of a long job such as a model download.
    Progress {
        id: String,
        label: String,
        percent: Option<u8>,
        done: bool,
        error: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_json() {
        let events = vec![
            UiEvent::RunStarted {
                run_id: 3,
                conv_id: "c1".into(),
            },
            UiEvent::AssistantDelta {
                run_id: 3,
                text: "hi".into(),
            },
            UiEvent::Confirm(ConfirmRequest {
                id: 9,
                run_id: 3,
                title: "Delete file".into(),
                detail: "C:\\a.txt".into(),
                risk: Risk::Destructive,
            }),
            UiEvent::RunFinished {
                run_id: 3,
                outcome: RunOutcome::Cancelled,
            },
            UiEvent::Error {
                run_id: None,
                kind: ErrorKind::NotLoggedIn,
                message: "log in".into(),
            },
        ];
        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let back: UiEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(event, back);
        }
    }

    #[test]
    fn event_tag_is_snake_case() {
        let json = serde_json::to_value(UiEvent::ConfirmResolved {
            id: 1,
            approved: true,
        })
        .unwrap();
        assert_eq!(json["type"], "confirm_resolved");
    }

    #[test]
    fn confirm_answer_parses_from_ui_payload() {
        let answer: ConfirmAnswer = serde_json::from_str(r#"{"id":4,"approved":false}"#).unwrap();
        assert_eq!(answer.id, 4);
        assert!(!answer.approved);
    }
}
