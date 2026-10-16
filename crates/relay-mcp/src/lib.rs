//! Relay's desktop tools, served to the AI CLI over MCP, behind a safety gate.
//!
//! * [`actions`] parses and validates tool arguments.
//! * [`safety`] decides what needs the owner's approval and owns the kill switch.
//! * [`host`] executes actions against a [`relay_os::Desktop`].
//! * [`server`] exposes it all as an MCP server on loopback.

pub mod actions;
pub mod audit;
pub mod host;
pub mod safety;
pub mod server;
pub mod tools;

pub use host::{CallCtx, Content, HostOptions, ToolHost, ToolOutput};
pub use safety::{ApprovalMode, ApprovalRequest, Approver, Decision, KillSwitch, STOPPED_MESSAGE};
pub use server::{McpServer, RunGrant};
pub use tools::{PERMISSION_TOOL, SERVER_NAME};
