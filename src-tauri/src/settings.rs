//! On-disk locations and persisted settings.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use relay_mcp::ApprovalMode;
use serde::{Deserialize, Serialize};

/// Folders and files Relay uses under its data directory.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    /// Working directory handed to the AI CLI (kept empty so no project files leak in).
    pub work_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// The owner's MCP servers (`{"mcpServers": {...}}`).
    pub mcp_config: PathBuf,
    pub settings_file: PathBuf,
}

impl Paths {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            work_dir: data_dir.join("workspace"),
            runs_dir: data_dir.join("runs"),
            logs_dir: data_dir.join("logs"),
            mcp_config: data_dir.join("mcp.json"),
            settings_file: data_dir.join("settings.json"),
            data_dir,
        }
    }

    /// Create the folders and a starter MCP file so the owner can see where to add servers.
    pub fn ensure(&self) -> Result<()> {
        for dir in [&self.work_dir, &self.runs_dir, &self.logs_dir] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        if !self.mcp_config.exists() {
            std::fs::write(&self.mcp_config, "{\n  \"mcpServers\": {}\n}\n")
                .with_context(|| format!("creating {}", self.mcp_config.display()))?;
        }
        Ok(())
    }
}

pub fn mode_to_str(mode: ApprovalMode) -> &'static str {
    match mode {
        ApprovalMode::Destructive => "destructive",
        ApprovalMode::Always => "always",
        ApprovalMode::Off => "off",
    }
}

pub fn mode_from_str(text: &str) -> Option<ApprovalMode> {
    match text {
        "destructive" => Some(ApprovalMode::Destructive),
        "always" => Some(ApprovalMode::Always),
        "off" => Some(ApprovalMode::Off),
        _ => None,
    }
}

/// Settings stored in `settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub approval_mode: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            approval_mode: mode_to_str(ApprovalMode::Destructive).to_string(),
        }
    }
}

impl Settings {
    pub fn mode(&self) -> ApprovalMode {
        mode_from_str(&self.approval_mode).unwrap_or_default()
    }

    /// Load settings; a missing or unreadable file yields the safe defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("relay-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in [
            ApprovalMode::Destructive,
            ApprovalMode::Always,
            ApprovalMode::Off,
        ] {
            assert_eq!(mode_from_str(mode_to_str(mode)), Some(mode));
        }
        assert_eq!(mode_from_str("bogus"), None);
    }

    #[test]
    fn missing_or_corrupt_settings_fall_back_to_safe_defaults() {
        let dir = temp("settings");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        assert_eq!(Settings::load(&file).mode(), ApprovalMode::Destructive);
        std::fs::write(&file, "{nope").unwrap();
        assert_eq!(Settings::load(&file).mode(), ApprovalMode::Destructive);
        std::fs::write(&file, r#"{"approval_mode":"surprise"}"#).unwrap();
        assert_eq!(Settings::load(&file).mode(), ApprovalMode::Destructive);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_persist() {
        let dir = temp("persist");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        let s = Settings {
            approval_mode: "always".into(),
        };
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file).mode(), ApprovalMode::Always);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_creates_folders_and_a_starter_mcp_file_once() {
        let dir = temp("paths");
        let paths = Paths::new(&dir);
        paths.ensure().unwrap();
        assert!(paths.work_dir.is_dir() && paths.runs_dir.is_dir() && paths.logs_dir.is_dir());
        let text = std::fs::read_to_string(&paths.mcp_config).unwrap();
        assert!(text.contains("mcpServers"));
        std::fs::write(&paths.mcp_config, "{\"mcpServers\":{\"x\":{}}}").unwrap();
        paths.ensure().unwrap();
        assert!(std::fs::read_to_string(&paths.mcp_config)
            .unwrap()
            .contains("\"x\""));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
