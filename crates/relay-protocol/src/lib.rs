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
}

/// Events pushed from the backend to the chat window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    RunStarted {
        run_id: RunId,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_round_trip_through_json() {
        let events = vec![
            UiEvent::RunStarted { run_id: 3 },
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
