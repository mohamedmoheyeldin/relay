//! The native execution layer, as the orchestrator sees it. Implemented over the OS APIs
//! (UI Automation / Accessibility, filesystem, process launch) by the application.

use async_trait::async_trait;

use crate::context::DesktopFacts;
use crate::plan::{UiTarget, Verification};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    /// Worth retrying (a window that has not appeared yet, a busy file).
    #[error("{0}")]
    Transient(String),
    /// Will not work however often it is tried.
    #[error("{0}")]
    Failed(String),
    #[error("this PC cannot do that: {0}")]
    Unsupported(String),
}

impl ServiceError {
    pub fn is_transient(&self) -> bool {
        matches!(self, ServiceError::Transient(_))
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;

/// Executes validated actions. Every method is one native operation; none runs arbitrary commands.
#[async_trait]
pub trait Services: Send + Sync {
    async fn open_app(&self, name: &str) -> ServiceResult<String>;
    async fn open_url(&self, url: &str) -> ServiceResult<String>;
    async fn focus_window(&self, title: &str) -> ServiceResult<String>;
    async fn find_files(
        &self,
        query: &str,
        root: Option<&str>,
        max: usize,
    ) -> ServiceResult<Vec<String>>;
    async fn list_dir(&self, path: &str) -> ServiceResult<Vec<String>>;
    async fn read_text(&self, path: &str, max_bytes: usize) -> ServiceResult<String>;
    async fn create_dir(&self, path: &str) -> ServiceResult<String>;
    async fn move_path(&self, from: &str, to: &str) -> ServiceResult<String>;
    async fn write_text(&self, path: &str, content: &str, overwrite: bool)
        -> ServiceResult<String>;
    async fn move_to_trash(&self, paths: &[String]) -> ServiceResult<String>;
    async fn ui_click(&self, target: &UiTarget) -> ServiceResult<String>;
    async fn ui_set_text(&self, target: &UiTarget, text: &str) -> ServiceResult<String>;
    async fn type_text(&self, text: &str) -> ServiceResult<String>;
    async fn press_keys(&self, keys: &str) -> ServiceResult<String>;
    /// Inspect the live state to confirm a step's effect.
    async fn check(&self, condition: &Verification) -> ServiceResult<bool>;
    /// Gather facts about the desktop for planners (titles, outline); never contents.
    async fn snapshot(&self) -> ServiceResult<DesktopFacts>;
}
