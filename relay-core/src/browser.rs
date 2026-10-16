//! Relay used to give Claude a browser through Playwright MCP and the Playwright
//! Extension (relay-browser, D-074). The local operator now reads and drives the
//! owner's browser through UI Automation with nothing to install (D-089), so all that
//! is left here is removing what that lane set up: the relay-browser registration in
//! Claude Code and the extension's token in Credential Manager. The installer removes
//! the extension policy it added in 0.1.9.

use crate::mcp::RELAY_BROWSER;

/// The name the extension's token was kept under in Credential Manager.
pub const TOKEN_SECRET: &str = "PLAYWRIGHT_MCP_EXTENSION_TOKEN";

/// Forget the extension's token. Nothing saved is fine too.
pub fn forget_token() {
    let _ = crate::secrets::delete(RELAY_BROWSER, TOKEN_SECRET);
}
