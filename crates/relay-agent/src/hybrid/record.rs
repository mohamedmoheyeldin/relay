//! Persisted record of one workflow (task), for the task history view and audit.

use relay_protocol::SharedInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOut {
    pub id: String,
    pub title: String,
    pub tool: String,
    pub risk: String,
    pub status: String,
    pub summary: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub conv_id: Option<String>,
    pub objective: String,
    pub strategy: String,
    pub provider: Option<String>,
    /// completed, failed, cancelled, declined, denied, timed_out or unavailable.
    pub outcome: String,
    pub steps_total: usize,
    pub steps_verified: usize,
    pub created_at: i64,
    pub updated_at: i64,
    pub plan_source: String,
    pub steps: Vec<StepOut>,
    pub shared: Vec<SharedInfo>,
    pub shared_bytes: usize,
    pub approvals: Vec<String>,
    pub errors: Vec<String>,
    pub notes: Option<String>,
    /// `chat`, `action` or `web`.
    #[serde(default)]
    pub intent: Option<String>,
    /// What did the work (local AI, cloud AI, native tools, MCP servers), for the tags in history.
    #[serde(default)]
    pub used: Vec<UsedTag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedTag {
    pub kind: String,
    pub label: String,
}

/// Where finished workflows are kept.
pub trait WorkflowStore: Send + Sync {
    fn save(&self, record: &WorkflowRecord);
}

/// Discards records (tests and callers that do not need history).
pub struct NullWorkflowStore;

impl WorkflowStore for NullWorkflowStore {
    fn save(&self, _record: &WorkflowRecord) {}
}
