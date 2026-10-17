//! Connects the agent's per-conversation context to the database and the owner's settings.

use std::sync::{Arc, Mutex};

use relay_agent::{ConvContext, ConvStore, WorkflowRecord, WorkflowStore};
use relay_store::{AiContext, Store, WorkflowRow};
use tracing::warn;

use crate::settings::Settings;

pub struct StoreAdapter {
    store: Arc<Store>,
    settings: Arc<Mutex<Settings>>,
}

impl StoreAdapter {
    pub fn new(store: Arc<Store>, settings: Arc<Mutex<Settings>>) -> Self {
        Self { store, settings }
    }
}

/// Join the owner's custom instructions and a project's instructions.
pub fn combine_instructions(custom: &str, project: &str) -> String {
    let custom = custom.trim();
    let project = project.trim();
    match (custom.is_empty(), project.is_empty()) {
        (true, true) => String::new(),
        (false, true) => custom.to_string(),
        (true, false) => format!("Project instructions:\n{project}"),
        (false, false) => format!("{custom}\n\nProject instructions:\n{project}"),
    }
}

impl ConvStore for StoreAdapter {
    fn load(&self, conv_id: &str) -> ConvContext {
        let ctx = self.store.ai_context(conv_id);
        let custom = self
            .settings
            .lock()
            .map(|s| s.custom_instructions.clone())
            .unwrap_or_default();
        ConvContext {
            session_id: ctx.session_id,
            history: ctx.history,
            instructions: combine_instructions(
                &custom,
                &self.store.project_instructions_for(conv_id),
            ),
        }
    }

    fn save(&self, conv_id: &str, session_id: Option<&str>, history: &[(String, String)]) {
        let ctx = AiContext {
            session_id: session_id.map(str::to_string),
            history: history.to_vec(),
        };
        if let Err(e) = self.store.save_ai_context(conv_id, &ctx) {
            warn!("could not save the AI context for {conv_id}: {e}");
        }
    }
}

impl WorkflowStore for StoreAdapter {
    fn save(&self, record: &WorkflowRecord) {
        let row = WorkflowRow {
            id: record.id.clone(),
            conv_id: record.conv_id.clone(),
            objective: record.objective.clone(),
            outcome: record.outcome.clone(),
            created_at: record.created_at,
            updated_at: record.updated_at,
        };
        match serde_json::to_string(record) {
            Ok(body) => {
                if let Err(e) = self.store.workflow_save(&row, &body) {
                    warn!("could not save workflow {}: {e}", record.id);
                }
            }
            Err(e) => warn!("could not encode workflow {}: {e}", record.id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_workflows_are_stored_and_read_back() {
        let store = Arc::new(Store::in_memory().unwrap());
        let adapter = StoreAdapter::new(store.clone(), Arc::default());
        let rec = WorkflowRecord {
            id: "w1".into(),
            conv_id: Some("c1".into()),
            objective: "tidy".into(),
            strategy: "native".into(),
            provider: None,
            outcome: "completed".into(),
            steps_total: 1,
            steps_verified: 1,
            created_at: 1,
            updated_at: 2,
            plan_source: "native".into(),
            steps: vec![],
            shared: vec![],
            shared_bytes: 0,
            approvals: vec![],
            errors: vec![],
            notes: None,
        };
        WorkflowStore::save(&adapter, &rec);
        let back: WorkflowRecord =
            serde_json::from_str(&store.workflow_get("w1").unwrap()).unwrap();
        assert_eq!(back, rec);
    }

    #[test]
    fn instructions_are_combined_cleanly() {
        assert_eq!(combine_instructions("", ""), "");
        assert_eq!(combine_instructions(" be brief ", ""), "be brief");
        assert_eq!(
            combine_instructions("", "use rust"),
            "Project instructions:\nuse rust"
        );
        assert_eq!(
            combine_instructions("be brief", "use rust"),
            "be brief\n\nProject instructions:\nuse rust"
        );
    }

    #[test]
    fn context_comes_from_the_database_and_settings() {
        let store = Arc::new(Store::in_memory().unwrap());
        let project = store
            .project_create("P", "blue", "Always use pnpm")
            .unwrap();
        let conv = store.conv_create(Some(&project.id)).unwrap();
        let settings = Arc::new(Mutex::new(Settings {
            custom_instructions: "Short answers".into(),
            ..Settings::default()
        }));
        let adapter = StoreAdapter::new(store.clone(), settings);
        let loaded = adapter.load(&conv.id);
        assert_eq!(loaded.session_id, None);
        assert!(
            loaded.instructions.contains("Short answers") && loaded.instructions.contains("pnpm")
        );
        ConvStore::save(&adapter, &conv.id, Some("s1"), &[("q".into(), "a".into())]);
        let again = adapter.load(&conv.id);
        assert_eq!(again.session_id.as_deref(), Some("s1"));
        assert_eq!(again.history.len(), 1);
    }
}
