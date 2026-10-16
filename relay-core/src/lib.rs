//! relay-core: the parts of Relay that are not UI.
//!
//! For now this is a library the app links in-process (see the PR notes); the
//! always-on daemon of D-032 wraps the same modules in M2. Every module works on any
//! OS so tests run in the cloud, and Windows-only calls are guarded.

pub mod browser;
pub mod catalog;
pub mod chat;
pub mod claude;
pub mod doctor;
pub mod download;
pub mod gpu;
pub mod launcher;
pub mod local_agent;
pub mod local_ai;
pub mod mcp;
pub mod operator;
pub mod paths;
pub mod proc;
pub mod secrets;
pub mod settings;
pub mod system;
pub mod updates;

/// Progress of a long job (download, install), reported to the UI.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Progress {
    pub job: String,
    pub stage: String,
    pub done: u64,
    pub total: u64,
}
