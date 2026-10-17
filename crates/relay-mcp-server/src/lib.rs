//! Relay as a stand-alone MCP server, so Claude Code, Codex, Cursor and any other MCP client can use
//! Relay's verified desktop actions and screen control.

pub mod approver;
pub mod cli;
pub mod hotkey;
pub mod protocol;
pub mod toolbox;

use std::path::PathBuf;
use std::sync::Arc;

use relay_mcp::audit::AuditLog;
use relay_mcp::{ApprovalMode, Approver, HostOptions, KillSwitch, ToolHost};
use relay_orchestrator::config::HybridConfig;
use relay_orchestrator::permissions::ConfirmLevel;
use relay_orchestrator::SystemServices;
use relay_os::driver::Driver;

pub use toolbox::{Surface, Toolbox};

/// The folder Relay's app keeps its data in (shared so both read the same settings).
pub fn default_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("dev.relay.operator")
    }
    #[cfg(target_os = "macos")]
    {
        relay_orchestrator::config::home_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Application Support/dev.relay.operator")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        relay_orchestrator::config::home_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(".local/share/dev.relay.operator")
    }
}

/// Build the toolbox for the real desktop.
pub fn build_toolbox(
    config: &HybridConfig,
    surface: Surface,
    approver: Arc<dyn Approver>,
    audit_dir: Option<PathBuf>,
) -> Result<(Arc<Toolbox>, KillSwitch), relay_os::OsError> {
    let desktop = relay_os::system_desktop()?;
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::new(desktop.clone(), kill.flag()));
    let services = Arc::new(SystemServices::new(
        desktop.clone(),
        driver.clone(),
        config.allowed_roots.iter().map(PathBuf::from).collect(),
    ));
    let mode = match config.confirm_level {
        ConfirmLevel::Off => ApprovalMode::Off,
        ConfirmLevel::Destructive => ApprovalMode::Destructive,
        ConfirmLevel::Always => ApprovalMode::Always,
    };
    let host = (surface == Surface::All).then(|| {
        Arc::new(ToolHost::new(
            desktop,
            driver,
            kill.clone(),
            approver.clone(),
            audit_dir.map(|d| Arc::new(AuditLog::new(d))),
            HostOptions {
                approval_mode: mode,
                ..HostOptions::default()
            },
        ))
    });
    let toolbox = Arc::new(Toolbox::new(
        host,
        services,
        config.policy(),
        approver,
        kill.clone(),
    ));
    Ok((toolbox, kill))
}
