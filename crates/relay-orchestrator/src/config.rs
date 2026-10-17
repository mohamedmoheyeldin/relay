//! Hybrid-assistant configuration shared by the app and the stand-alone MCP server, read from the
//! `hybrid` object of `settings.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::permissions::{ConfirmLevel, PrivacyMode, SharePolicy};
use crate::router::ProviderKind;

/// The owner's model and reasoning choice for one cloud provider (`None` = the CLI's default).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderChoice {
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Folders the assistant may work in until the owner changes it.
pub fn default_allowed_roots() -> Vec<String> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    [
        "Desktop",
        "Documents",
        "Downloads",
        "Pictures",
        "Videos",
        "Music",
    ]
    .iter()
    .map(|d| home.join(d))
    .filter(|p| p.is_dir())
    .map(|p| p.display().to_string())
    .collect()
}

/// Folders that stay off limits even inside an allowed folder.
pub fn default_protected_roots() -> Vec<String> {
    let mut out: Vec<String> = if cfg!(windows) {
        vec![
            r"C:\Windows".into(),
            r"C:\Program Files".into(),
            r"C:\Program Files (x86)".into(),
            r"C:\ProgramData".into(),
        ]
    } else {
        vec![
            "/System".into(),
            "/Library".into(),
            "/usr".into(),
            "/etc".into(),
        ]
    };
    if let Some(home) = home_dir() {
        for d in ["AppData", ".ssh", ".aws", ".gnupg", "Library/Keychains"] {
            out.push(home.join(d).display().to_string());
        }
    }
    out
}

/// Hybrid-assistant choices: what may leave the PC, which cloud CLI helps, what needs a yes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HybridConfig {
    pub privacy_mode: PrivacyMode,
    /// `claude` or `codex`.
    pub default_provider: String,
    pub confirm_level: ConfirmLevel,
    pub share: SharePolicy,
    pub allowed_roots: Vec<String>,
    pub protected_roots: Vec<String>,
    pub disabled_tools: Vec<String>,
    pub providers: BTreeMap<String, ProviderChoice>,
    /// Let a cloud CLI drive Relay's tools directly instead of returning a plan (not the default).
    pub agent_mode: bool,
    /// Let Relay work in a browser (Playwright) for website tasks.
    #[serde(default = "default_true")]
    pub browser_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            privacy_mode: PrivacyMode::default(),
            default_provider: "claude".into(),
            confirm_level: ConfirmLevel::default(),
            share: SharePolicy::default(),
            allowed_roots: default_allowed_roots(),
            protected_roots: default_protected_roots(),
            disabled_tools: Vec::new(),
            providers: BTreeMap::new(),
            agent_mode: false,
            browser_enabled: true,
        }
    }
}

impl HybridConfig {
    pub fn provider(&self) -> ProviderKind {
        ProviderKind::parse(&self.default_provider).unwrap_or(ProviderKind::Claude)
    }
}

impl HybridConfig {
    /// Read the `hybrid` object from a `settings.json`; a missing or unreadable file gives the defaults.
    pub fn load_from_settings(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("hybrid").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }

    /// The permission policy this configuration describes.
    pub fn policy(&self) -> crate::permissions::Policy {
        let mut p = crate::permissions::Policy::new(
            self.allowed_roots.iter().map(PathBuf::from).collect(),
            self.protected_roots.iter().map(PathBuf::from).collect(),
        );
        p.privacy = self.privacy_mode;
        p.confirm = self.confirm_level;
        p.share = self.share;
        p.disabled_tools = self.disabled_tools.iter().cloned().collect();
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_and_missing_hybrid_object_give_safe_defaults() {
        let dir = std::env::temp_dir().join(format!("relay-cfg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let none = HybridConfig::load_from_settings(&dir.join("nope.json"));
        assert_eq!(none.confirm_level, ConfirmLevel::Destructive);
        let f = dir.join("s.json");
        std::fs::write(&f, r#"{"approval_mode":"off"}"#).unwrap();
        assert_eq!(
            HybridConfig::load_from_settings(&f).confirm_level,
            ConfirmLevel::Destructive
        );
        std::fs::write(
            &f,
            r#"{"hybrid":{"confirm_level":"always","disabled_tools":["type_text"]}}"#,
        )
        .unwrap();
        let c = HybridConfig::load_from_settings(&f);
        assert_eq!(c.confirm_level, ConfirmLevel::Always);
        assert!(c.policy().disabled_tools.contains("type_text"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
