//! The names Relay can't choose: the cloud AI's command-line tool, its installer and
//! its config file are called what the vendor calls them. They live here and nowhere
//! else, so the rest of Relay only ever says "Cloud AI".

/// The command-line tool (on PATH, or where the official installer puts it).
pub const EXE: &str = if cfg!(windows) {
    "claude.exe"
} else {
    "claude"
};
/// The same tool without the extension, as PATH lookups take it.
pub const COMMAND: &str = "claude";
/// The official installer for Windows (PowerShell), run unmodified from its own URL.
pub const INSTALL_SCRIPT_URL: &str = "https://claude.ai/install.ps1";
/// The tool's user config in the home folder (its MCP server list lives here).
pub const USER_CONFIG: &str = ".claude.json";
/// How the tool's `mcp list` names connectors that come from the owner's account.
pub const ACCOUNT_CONNECTOR_PREFIX: &str = "claude.ai ";
/// `auth login` flag that signs in with the owner's subscription account.
pub const ACCOUNT_LOGIN_FLAG: &str = "--claudeai";
