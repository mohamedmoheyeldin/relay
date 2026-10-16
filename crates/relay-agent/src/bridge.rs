//! Connects approval requests from the tools to the chat window and back.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use relay_mcp::{ApprovalRequest, Approver, Decision};
use relay_protocol::{ConfirmAnswer, ConfirmRequest, UiEvent};
use tokio::sync::oneshot;

/// Sends events to the chat window.
pub type EventSink = Arc<dyn Fn(UiEvent) + Send + Sync>;

/// Shows confirmation cards and waits for the owner's click (or a timeout).
pub struct ConfirmBridge {
    sink: EventSink,
    timeout: Duration,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
}

impl ConfirmBridge {
    pub fn new(sink: EventSink, timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            sink,
            timeout,
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        })
    }

    fn pending(&self) -> std::sync::MutexGuard<'_, HashMap<u64, oneshot::Sender<bool>>> {
        self.pending.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Deliver the owner's answer. Returns false if the card was already resolved.
    pub fn answer(&self, answer: ConfirmAnswer) -> bool {
        match self.pending().remove(&answer.id) {
            Some(tx) => tx.send(answer.approved).is_ok(),
            None => false,
        }
    }

    /// Deny every open card (used by the stop button and the kill hotkey).
    pub fn deny_all(&self) {
        let drained: Vec<_> = self.pending().drain().collect();
        for (_, tx) in drained {
            let _ = tx.send(false);
        }
    }

    pub fn open_count(&self) -> usize {
        self.pending().len()
    }
}

#[async_trait]
impl Approver for ConfirmBridge {
    async fn ask(&self, request: ApprovalRequest) -> Decision {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending().insert(id, tx);
        (self.sink)(UiEvent::Confirm(ConfirmRequest {
            id,
            run_id: request.run_id,
            title: request.title,
            detail: request.detail,
            risk: request.risk,
        }));
        let decision = match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(true)) => Decision::Approved,
            Ok(Ok(false)) | Ok(Err(_)) => Decision::Denied,
            Err(_) => Decision::TimedOut,
        };
        self.pending().remove(&id);
        (self.sink)(UiEvent::ConfirmResolved {
            id,
            approved: decision.is_approved(),
        });
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relay_protocol::Risk;

    fn collecting() -> (EventSink, Arc<Mutex<Vec<UiEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let e = events.clone();
        let sink: EventSink = Arc::new(move |ev| {
            if let Ok(mut v) = e.lock() {
                v.push(ev);
            }
        });
        (sink, events)
    }

    fn request() -> ApprovalRequest {
        ApprovalRequest {
            run_id: 5,
            title: "Delete".into(),
            detail: "file".into(),
            risk: Risk::Destructive,
        }
    }

    async fn first_confirm_id(events: &Arc<Mutex<Vec<UiEvent>>>) -> u64 {
        for _ in 0..200 {
            if let Some(UiEvent::Confirm(c)) = events.lock().unwrap().first().cloned() {
                return c.id;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no confirm card was shown");
    }

    #[tokio::test]
    async fn approval_round_trip() {
        let (sink, events) = collecting();
        let bridge = ConfirmBridge::new(sink, Duration::from_secs(5));
        let b = bridge.clone();
        let ask = tokio::spawn(async move { b.ask(request()).await });
        let id = first_confirm_id(&events).await;
        assert_eq!(bridge.open_count(), 1);
        assert!(bridge.answer(ConfirmAnswer { id, approved: true }));
        assert_eq!(ask.await.unwrap(), Decision::Approved);
        assert_eq!(bridge.open_count(), 0);
        let evs = events.lock().unwrap().clone();
        assert!(matches!(
            evs.last(),
            Some(UiEvent::ConfirmResolved { approved: true, .. })
        ));
        // A second answer to the same card is ignored.
        assert!(!bridge.answer(ConfirmAnswer {
            id,
            approved: false
        }));
    }

    #[tokio::test]
    async fn denial_and_timeout() {
        let (sink, events) = collecting();
        let bridge = ConfirmBridge::new(sink, Duration::from_secs(5));
        let b = bridge.clone();
        let ask = tokio::spawn(async move { b.ask(request()).await });
        let id = first_confirm_id(&events).await;
        bridge.answer(ConfirmAnswer {
            id,
            approved: false,
        });
        assert_eq!(ask.await.unwrap(), Decision::Denied);

        let (sink, _) = collecting();
        let quick = ConfirmBridge::new(sink, Duration::from_millis(50));
        assert_eq!(quick.ask(request()).await, Decision::TimedOut);
        assert_eq!(quick.open_count(), 0);
    }

    #[tokio::test]
    async fn deny_all_resolves_open_cards() {
        let (sink, events) = collecting();
        let bridge = ConfirmBridge::new(sink, Duration::from_secs(30));
        let b = bridge.clone();
        let ask = tokio::spawn(async move { b.ask(request()).await });
        first_confirm_id(&events).await;
        bridge.deny_all();
        assert_eq!(ask.await.unwrap(), Decision::Denied);
    }
}
