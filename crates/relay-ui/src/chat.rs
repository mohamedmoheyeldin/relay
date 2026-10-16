//! Chat state and the reducer that turns backend events into a conversation. Pure, so it is
//! tested natively.

use relay_protocol::{ConfirmRequest, ErrorKind, RunOutcome, StatusInfo, UiEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolState {
    Running,
    Done,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmState {
    Waiting,
    Approved,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    User(String),
    Assistant {
        text: String,
        open: bool,
    },
    Tool {
        tool_id: String,
        name: String,
        summary: String,
        state: ToolState,
    },
    Error {
        kind: ErrorKind,
        message: String,
    },
    Confirm {
        request: ConfirmRequest,
        state: ConfirmState,
    },
    Notice(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatState {
    pub items: Vec<Item>,
    pub running: bool,
    pub status: Option<StatusInfo>,
}

impl ChatState {
    pub fn push_user(&mut self, text: &str) {
        self.items.push(Item::User(text.to_string()));
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Whether any confirmation card is still waiting for the owner.
    pub fn has_open_confirm(&self) -> bool {
        self.items.iter().any(|i| {
            matches!(
                i,
                Item::Confirm {
                    state: ConfirmState::Waiting,
                    ..
                }
            )
        })
    }

    fn open_assistant(&mut self) -> Option<&mut Item> {
        self.items
            .iter_mut()
            .rev()
            .find(|i| matches!(i, Item::Assistant { open: true, .. }))
    }

    fn close_assistants(&mut self) {
        for item in &mut self.items {
            if let Item::Assistant { open, .. } = item {
                *open = false;
            }
        }
    }

    pub fn apply(&mut self, event: &UiEvent) {
        match event {
            UiEvent::RunStarted { .. } => self.running = true,
            UiEvent::AssistantDelta { text, .. } => match self.open_assistant() {
                Some(Item::Assistant { text: existing, .. }) => existing.push_str(text),
                _ => self.items.push(Item::Assistant {
                    text: text.clone(),
                    open: true,
                }),
            },
            UiEvent::AssistantMessage { text, .. } => match self.open_assistant() {
                Some(Item::Assistant {
                    text: existing,
                    open,
                }) => {
                    *existing = text.clone();
                    *open = false;
                }
                _ => self.items.push(Item::Assistant {
                    text: text.clone(),
                    open: false,
                }),
            },
            UiEvent::ToolStarted {
                tool_id,
                name,
                summary,
                ..
            } => {
                self.close_assistants();
                self.items.push(Item::Tool {
                    tool_id: tool_id.clone(),
                    name: name.clone(),
                    summary: summary.clone(),
                    state: ToolState::Running,
                });
            }
            UiEvent::ToolFinished {
                tool_id,
                ok,
                summary,
                ..
            } => {
                for item in self.items.iter_mut().rev() {
                    if let Item::Tool {
                        tool_id: id, state, ..
                    } = item
                    {
                        if id == tool_id {
                            *state = if *ok {
                                ToolState::Done
                            } else {
                                ToolState::Failed(summary.clone())
                            };
                            break;
                        }
                    }
                }
            }
            UiEvent::Confirm(request) => self.items.push(Item::Confirm {
                request: request.clone(),
                state: ConfirmState::Waiting,
            }),
            UiEvent::ConfirmResolved { id, approved } => {
                for item in &mut self.items {
                    if let Item::Confirm { request, state } = item {
                        if request.id == *id {
                            *state = if *approved {
                                ConfirmState::Approved
                            } else {
                                ConfirmState::Declined
                            };
                        }
                    }
                }
            }
            UiEvent::RunFinished { outcome, .. } => {
                self.running = false;
                self.close_assistants();
                // Any card still waiting can no longer be answered.
                for item in &mut self.items {
                    if let Item::Confirm { state, .. } = item {
                        if *state == ConfirmState::Waiting {
                            *state = ConfirmState::Declined;
                        }
                    }
                }
                if *outcome == RunOutcome::Cancelled {
                    self.items.push(Item::Notice("Stopped.".into()));
                }
            }
            UiEvent::Error { kind, message, .. } => self.items.push(Item::Error {
                kind: *kind,
                message: message.clone(),
            }),
            UiEvent::Status(status) => {
                self.running = status.busy;
                self.status = Some(status.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_protocol::Risk;

    fn delta(t: &str) -> UiEvent {
        UiEvent::AssistantDelta {
            run_id: 1,
            text: t.into(),
        }
    }

    fn confirm(id: u64) -> ConfirmRequest {
        ConfirmRequest {
            id,
            run_id: 1,
            title: "Delete".into(),
            detail: "x".into(),
            risk: Risk::Destructive,
        }
    }

    #[test]
    fn deltas_stream_into_one_bubble_and_the_final_message_replaces_them() {
        let mut s = ChatState::default();
        s.apply(&UiEvent::RunStarted { run_id: 1 });
        s.apply(&delta("Hel"));
        s.apply(&delta("lo"));
        assert_eq!(
            s.items,
            vec![Item::Assistant {
                text: "Hello".into(),
                open: true
            }]
        );
        s.apply(&UiEvent::AssistantMessage {
            run_id: 1,
            text: "Hello there".into(),
        });
        assert_eq!(
            s.items,
            vec![Item::Assistant {
                text: "Hello there".into(),
                open: false
            }]
        );
        // A later message starts a new bubble.
        s.apply(&delta("Next"));
        assert_eq!(s.items.len(), 2);
    }

    #[test]
    fn message_without_deltas_creates_a_closed_bubble() {
        let mut s = ChatState::default();
        s.apply(&UiEvent::AssistantMessage {
            run_id: 1,
            text: "Hi".into(),
        });
        assert_eq!(
            s.items,
            vec![Item::Assistant {
                text: "Hi".into(),
                open: false
            }]
        );
    }

    #[test]
    fn tools_close_the_bubble_and_track_their_result() {
        let mut s = ChatState::default();
        s.apply(&delta("Looking"));
        s.apply(&UiEvent::ToolStarted {
            run_id: 1,
            tool_id: "t1".into(),
            name: "screenshot".into(),
            summary: "Take a screenshot".into(),
        });
        assert!(matches!(s.items[0], Item::Assistant { open: false, .. }));
        assert!(matches!(
            &s.items[1],
            Item::Tool {
                state: ToolState::Running,
                ..
            }
        ));
        s.apply(&UiEvent::ToolFinished {
            run_id: 1,
            tool_id: "t1".into(),
            ok: false,
            summary: "declined".into(),
        });
        assert!(matches!(
            &s.items[1],
            Item::Tool { state: ToolState::Failed(m), .. } if m == "declined"
        ));
        s.apply(&delta("After"));
        assert_eq!(s.items.len(), 3);
    }

    #[test]
    fn confirmation_cards_resolve() {
        let mut s = ChatState::default();
        s.apply(&UiEvent::Confirm(confirm(4)));
        assert!(s.has_open_confirm());
        s.apply(&UiEvent::ConfirmResolved {
            id: 4,
            approved: true,
        });
        assert!(!s.has_open_confirm());
        assert!(matches!(
            &s.items[0],
            Item::Confirm {
                state: ConfirmState::Approved,
                ..
            }
        ));
    }

    #[test]
    fn finishing_a_run_closes_everything_and_notes_a_stop() {
        let mut s = ChatState::default();
        s.apply(&UiEvent::RunStarted { run_id: 1 });
        s.apply(&delta("working"));
        s.apply(&UiEvent::Confirm(confirm(1)));
        assert!(s.running);
        s.apply(&UiEvent::RunFinished {
            run_id: 1,
            outcome: RunOutcome::Cancelled,
        });
        assert!(!s.running);
        assert!(!s.has_open_confirm());
        assert!(matches!(s.items[0], Item::Assistant { open: false, .. }));
        assert_eq!(s.items.last(), Some(&Item::Notice("Stopped.".into())));
    }

    #[test]
    fn errors_and_status() {
        let mut s = ChatState::default();
        s.apply(&UiEvent::Error {
            run_id: None,
            kind: ErrorKind::NotLoggedIn,
            message: "sign in".into(),
        });
        assert!(matches!(
            &s.items[0],
            Item::Error {
                kind: ErrorKind::NotLoggedIn,
                ..
            }
        ));
        s.apply(&UiEvent::Status(StatusInfo {
            provider: "Claude Code".into(),
            ready: true,
            detail: "Ready".into(),
            busy: true,
            kill_hotkey: "Ctrl+Alt+Shift+Esc".into(),
        }));
        assert!(s.running);
        assert_eq!(s.status.as_ref().map(|x| x.ready), Some(true));
    }

    #[test]
    fn user_messages_and_clear() {
        let mut s = ChatState::default();
        s.push_user("hello");
        assert_eq!(s.items, vec![Item::User("hello".into())]);
        s.clear();
        assert!(s.items.is_empty());
    }
}
