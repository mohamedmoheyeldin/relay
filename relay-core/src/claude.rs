//! Claude Code set up by Relay, not bundled (D-045).
//!
//! Relay only runs Claude Code's own commands: `claude --version`, `claude auth status`
//! (which reports whether a login exists, never the token), `claude update`, and
//! Anthropic's official Windows installer, unmodified, from Anthropic's own URL. The
//! sign-in itself runs `claude auth login` in the app's terminal pane.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::proc;

/// Anthropic's official installer for Windows (PowerShell).
pub const INSTALL_SCRIPT_URL: &str = "https://claude.ai/install.ps1";

/// Where the official native installer puts `claude.exe`.
pub fn default_install_path() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    dirs::home_dir().map(|home| home.join(".local").join("bin").join(exe))
}

/// Find `claude`: PATH first, then the native installer's location (the app's PATH
/// does not change after an install until it restarts).
pub fn find() -> Option<PathBuf> {
    proc::which("claude").or_else(|| default_install_path().filter(|p| p.is_file()))
}

fn require() -> Result<PathBuf> {
    find().context("Claude Code is not installed")
}

/// "2.1.296" from "2.1.296 (Claude Code)".
pub fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace()
        .next()
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

pub async fn version() -> Option<String> {
    let claude = find()?;
    let out = proc::run(&claude, &["--version"]).await.ok()?;
    parse_version(&out.stdout)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AuthStatus {
    #[serde(rename = "loggedIn", default)]
    pub logged_in: bool,
    #[serde(rename = "authMethod", default)]
    pub auth_method: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(rename = "subscriptionType", default)]
    pub subscription_type: Option<String>,
}

pub fn parse_auth_status(json: &str) -> Result<AuthStatus> {
    Ok(serde_json::from_str(json.trim())?)
}

pub async fn auth_status() -> Result<AuthStatus> {
    let claude = require()?;
    let out = proc::run(&claude, &["auth", "status", "--json"]).await?;
    // Logged-out status may come with a non-zero exit code; the JSON still says so.
    parse_auth_status(&out.stdout).or_else(|_| {
        if out.ok {
            bail!("unexpected output from claude auth status")
        } else {
            Ok(AuthStatus::default())
        }
    })
}

/// Summary for the Settings > Claude page and the wizard.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ClaudeState {
    pub installed: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub auth: AuthStatus,
}

pub async fn state() -> ClaudeState {
    let path = find();
    if path.is_none() {
        return ClaudeState::default();
    }
    ClaudeState {
        installed: true,
        version: version().await,
        auth: auth_status().await.unwrap_or_default(),
        path,
    }
}

/// Run Anthropic's official installer and stream its output.
pub async fn install(on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
    if !cfg!(windows) {
        bail!("the Claude Code installer step is for Windows");
    }
    let mut cmd = proc::command("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &format!("irm {INSTALL_SCRIPT_URL} | iex"),
    ]);
    let out = proc::run_streaming(cmd, on_line).await?;
    if !out.ok || find().is_none() {
        bail!(
            "the Claude Code installer did not finish (exit code {:?})",
            out.code
        );
    }
    Ok(())
}

/// `claude update`, streamed.
pub async fn update(on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
    let mut cmd = proc::command(require()?);
    cmd.arg("update");
    let out = proc::run_streaming(cmd, on_line).await?;
    if !out.ok {
        bail!("claude update failed (exit code {:?})", out.code);
    }
    Ok(())
}

/// Check that a headless run works: a trivial `claude -p` (D-045).
pub async fn verify() -> Result<String> {
    let claude = require()?;
    let workdir = crate::paths::work_dir();
    std::fs::create_dir_all(&workdir)?;
    let out = proc::command(&claude)
        .current_dir(&workdir)
        .args([
            "-p",
            "Reply with the single word OK.",
            "--output-format",
            "json",
            "--strict-mcp-config",
        ])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).with_context(|| {
        format!(
            "claude -p gave no JSON: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })?;
    if value
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
    {
        bail!(
            "claude -p reported an error: {}",
            value
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
        );
    }
    Ok(value
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        assert_eq!(
            parse_version("2.1.296 (Claude Code)\n").as_deref(),
            Some("2.1.296")
        );
        assert_eq!(parse_version("command not found"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn auth_status_parsing() {
        let json = r#"{
          "loggedIn": true, "authMethod": "claude.ai", "apiProvider": "firstParty",
          "email": "someone@example.com", "subscriptionType": "max"
        }"#;
        let s = parse_auth_status(json).unwrap();
        assert!(s.logged_in);
        assert_eq!(s.subscription_type.as_deref(), Some("max"));
        let out = parse_auth_status(r#"{"loggedIn": false}"#).unwrap();
        assert!(!out.logged_in);
    }
}
