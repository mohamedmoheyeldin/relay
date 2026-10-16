//! Cloud AI set up by Relay, not bundled (D-045).
//!
//! Relay only runs Cloud AI's own commands: `cloud-cli --version`, `cloud-cli auth status`
//! (which reports whether a login exists, never the token), `cloud-cli update`, and
//! the official Windows installer, unmodified, from its own URL. The
//! sign-in itself runs `cloud-cli auth login` in the app's terminal pane.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::{cli_names, proc};

pub use crate::cli_names::INSTALL_SCRIPT_URL;

/// Where the official native installer puts `cloud-cli.exe`.
pub fn default_install_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".local").join("bin").join(cli_names::EXE))
}

/// Find `cloud-cli`: PATH first, then the native installer's location (the app's PATH
/// does not change after an install until it restarts).
pub fn find() -> Option<PathBuf> {
    proc::which(cli_names::COMMAND).or_else(|| default_install_path().filter(|p| p.is_file()))
}

fn require() -> Result<PathBuf> {
    find().context("Cloud AI is not installed")
}

/// "2.1.296" from "2.1.296 (Cloud AI)".
pub fn parse_version(out: &str) -> Option<String> {
    out.split_whitespace()
        .next()
        .filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

pub async fn version() -> Option<String> {
    let cloud = find()?;
    let out = proc::run(&cloud, &["--version"]).await.ok()?;
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
    let cloud = require()?;
    let out = proc::run(&cloud, &["auth", "status", "--json"]).await?;
    // Logged-out status may come with a non-zero exit code; the JSON still says so.
    parse_auth_status(&out.stdout).or_else(|_| {
        if out.ok {
            bail!("unexpected output from the Cloud AI CLI auth status")
        } else {
            Ok(AuthStatus::default())
        }
    })
}

/// Summary for the Settings > Cloud AI page and the wizard.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CloudState {
    pub installed: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub auth: AuthStatus,
}

pub async fn state() -> CloudState {
    let path = find();
    if path.is_none() {
        return CloudState::default();
    }
    CloudState {
        installed: true,
        version: version().await,
        auth: auth_status().await.unwrap_or_default(),
        path,
    }
}

/// Run the official installer and stream its output.
pub async fn install(on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
    if !cfg!(windows) {
        bail!("the Cloud AI installer step is for Windows");
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
            "the Cloud AI installer did not finish (exit code {:?})",
            out.code
        );
    }
    Ok(())
}

/// `cloud-cli update`, streamed.
pub async fn update(on_line: impl Fn(String) + Send + Sync + 'static) -> Result<()> {
    let mut cmd = proc::command(require()?);
    cmd.arg("update");
    let out = proc::run_streaming(cmd, on_line).await?;
    if !out.ok {
        bail!("Cloud AI CLI update failed (exit code {:?})", out.code);
    }
    Ok(())
}

/// Check that a headless run works: a trivial `cloud-cli -p` (D-045).
/// How long the one-word test may take before Relay gives up on it.
const VERIFY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

pub async fn verify() -> Result<String> {
    let cloud = require()?;
    let workdir = crate::paths::work_dir();
    std::fs::create_dir_all(&workdir)?;
    let run = proc::command(&cloud)
        .current_dir(&workdir)
        .args([
            "-p",
            "Reply with the single word OK.",
            "--output-format",
            "json",
            "--strict-mcp-config",
        ])
        .output();
    let out = match tokio::time::timeout(VERIFY_TIMEOUT, run).await {
        Ok(out) => out?,
        Err(_) => bail!(
            "Cloud AI did not answer within {} seconds. Check your internet connection and try again.",
            VERIFY_TIMEOUT.as_secs()
        ),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).with_context(|| {
        format!(
            "Cloud AI CLI -p gave no JSON: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })?;
    if value
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
    {
        bail!(
            "Cloud AI CLI -p reported an error: {}",
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
            parse_version("2.1.296 (Cloud AI)\n").as_deref(),
            Some("2.1.296")
        );
        assert_eq!(parse_version("command not found"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn auth_status_parsing() {
        let json = r#"{
          "loggedIn": true, "authMethod": "account", "apiProvider": "firstParty",
          "email": "someone@example.com", "subscriptionType": "max"
        }"#;
        let s = parse_auth_status(json).unwrap();
        assert!(s.logged_in);
        assert_eq!(s.subscription_type.as_deref(), Some("max"));
        let out = parse_auth_status(r#"{"loggedIn": false}"#).unwrap();
        assert!(!out.logged_in);
    }
}
