//! Per-conversation AI context (session id, recent history, standing instructions).

use std::collections::HashMap;
use std::sync::Mutex;

/// What the agent knows about one conversation before a task starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConvContext {
    /// The cloud CLI's session id, to resume it.
    pub session_id: Option<String>,
    /// Recent (request, answer) pairs.
    pub history: Vec<(String, String)>,
    /// Standing instructions to follow (owner's custom instructions plus the project's).
    pub instructions: String,
}

/// Where conversation contexts live. The app backs this with the database.
pub trait ConvStore: Send + Sync {
    fn load(&self, conv_id: &str) -> ConvContext;
    fn save(&self, conv_id: &str, session_id: Option<&str>, history: &[(String, String)]);
}

/// In-memory store, used in tests and as a fallback.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<HashMap<String, ConvContext>>,
    instructions: Mutex<String>,
}

impl MemoryStore {
    pub fn with_instructions(text: &str) -> Self {
        Self {
            inner: Mutex::default(),
            instructions: Mutex::new(text.to_string()),
        }
    }
}

impl ConvStore for MemoryStore {
    fn load(&self, conv_id: &str) -> ConvContext {
        let mut ctx = self
            .inner
            .lock()
            .map(|m| m.get(conv_id).cloned().unwrap_or_default())
            .unwrap_or_default();
        ctx.instructions = self
            .instructions
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        ctx
    }

    fn save(&self, conv_id: &str, session_id: Option<&str>, history: &[(String, String)]) {
        if let Ok(mut m) = self.inner.lock() {
            let entry = m.entry(conv_id.to_string()).or_default();
            entry.session_id = session_id.map(str::to_string);
            entry.history = history.to_vec();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contexts_are_kept_per_conversation() {
        let s = MemoryStore::with_instructions("be brief");
        assert_eq!(s.load("a").session_id, None);
        s.save("a", Some("sess"), &[("q".into(), "a".into())]);
        let a = s.load("a");
        assert_eq!(a.session_id.as_deref(), Some("sess"));
        assert_eq!(a.history.len(), 1);
        assert_eq!(a.instructions, "be brief");
        assert_eq!(s.load("b").session_id, None);
    }
}
