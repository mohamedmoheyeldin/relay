//! Relay's orchestrator: runs one task at a time through an AI provider, wiring in the desktop
//! tools, confirmations and the stop switch, and reporting progress as chat events.

pub mod agent;
pub mod bridge;
pub mod prompt;

pub use agent::{Agent, AgentConfig, SubmitError};
pub use bridge::{ConfirmBridge, EventSink};
