//! Relay's orchestrator: runs one task at a time through the local model or the cloud AI CLI,
//! wiring in the desktop tools, confirmations and the stop switch, and reporting progress as
//! chat events.

pub mod agent;
pub mod bridge;
pub mod browser;
pub mod context;
pub mod hybrid;
pub mod local;
pub mod prompt;

pub use agent::{Agent, AgentConfig, AgentParts, SubmitError};
pub use bridge::{ConfirmBridge, EventSink};
pub use context::{ConvContext, ConvStore, MemoryStore};
pub use hybrid::{
    Hybrid, HybridParts, HybridSettings, ProviderOpts, WorkflowRecord, WorkflowStore,
};
pub use local::{LocalConfig, LocalProvider, RouteMode};
