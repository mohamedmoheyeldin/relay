//! Relay's settings (D-048), stored in `%APPDATA%\Relay\settings.json`.
//!
//! relay-core owns the file: the app reads and changes settings only through this
//! module. Every field has a default, so an old or partial file still loads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::mcp::PausedServer;
use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub general: General,
    pub claude: ClaudeSettings,
    pub local_ai: LocalAi,
    pub connections: Connections,
    pub browser: Browser,
    pub safety: Safety,
    pub privacy: Privacy,
    pub updates: Updates,
    pub setup: Setup,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloseAction {
    /// Ask the first time the window is closed.
    #[default]
    Ask,
    Tray,
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct General {
    pub start_at_login: bool,
    pub close_action: CloseAction,
    pub language: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            start_at_login: false,
            close_action: CloseAction::Ask,
            language: "en".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClaudeSettings {
    /// Passed to `claude -p --model`; empty means Claude Code's own default.
    pub model: String,
    /// Passed to `claude -p --effort`; empty means Claude Code's own default.
    pub effort: String,
}

/// A downloaded model chosen for one role. relay-screen finds its files by id in
/// `installed.json` (D-052).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveModel {
    pub model_id: String,
    pub quant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentModel {
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LocalAi {
    /// "Where is it" model (D-036: UI-Venus-1.5-8B by default once downloaded).
    pub grounding: Option<ActiveModel>,
    /// "Check / describe" model (D-036: Qwen3-VL-8B-Instruct by default).
    pub describe: Option<ActiveModel>,
    /// The local operator that runs tasks when Claude can't (D-067). None picks
    /// the describe model, then the grounding model, then the first downloaded planner.
    pub agent: Option<AgentModel>,
    /// Models unload this many seconds after a task ends (D-042).
    pub idle_unload_secs: u32,
    /// None means the default `%LOCALAPPDATA%\Relay\models`. relay-screen reads it too.
    pub models_dir: Option<PathBuf>,
}

impl Default for LocalAi {
    fn default() -> Self {
        Self {
            grounding: None,
            describe: None,
            agent: None,
            idle_unload_secs: 60,
            models_dir: None,
        }
    }
}

impl LocalAi {
    pub fn models_dir(&self) -> PathBuf {
        self.models_dir
            .clone()
            .unwrap_or_else(paths::default_models_dir)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Connections {
    /// Servers paused from the hub: removed from Claude Code's config and kept here
    /// so they can be put back exactly as they were (D-047).
    pub paused: Vec<PausedServer>,
    /// Names of the secret environment variables per server. The values live in
    /// Windows Credential Manager, never here.
    pub secret_names: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Browser {
    /// "auto" picks the first Chromium-based browser found; otherwise a browser
    /// name from discovery.
    pub preferred: String,
    /// Give Claude the relay-browser server (Playwright over the chosen browser).
    pub lane_enabled: bool,
    /// Drive the browser the owner already has open, through the Playwright
    /// Extension, instead of a separate Relay profile.
    pub use_own: bool,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            preferred: "auto".into(),
            lane_enabled: true,
            use_own: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Safety {
    /// "ask" (Relay asks before acting) or "off". Passed to relay-screen as RELAY_APPROVAL.
    pub approval_mode: String,
    /// Tools that always ask, even when approvals are off (names, e.g. "type").
    pub always_ask: Vec<String>,
    pub phone_approvals: bool,
    /// ntfy.sh topic for phone approvals.
    pub ntfy_topic: String,
}

impl Default for Safety {
    fn default() -> Self {
        Self {
            approval_mode: "ask".into(),
            always_ask: Vec::new(),
            phone_approvals: false,
            ntfy_topic: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Privacy {
    /// None means the default `%LOCALAPPDATA%\Relay\runs`.
    pub runs_dir: Option<PathBuf>,
    /// Run folders (audit log and screenshots) older than this are deleted. 0 keeps them.
    pub retention_days: u32,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            runs_dir: None,
            retention_days: 30,
        }
    }
}

impl Privacy {
    pub fn runs_dir(&self) -> PathBuf {
        self.runs_dir
            .clone()
            .unwrap_or_else(paths::default_runs_dir)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Updates {
    pub check_on_start: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            check_on_start: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Setup {
    /// The first-run wizard finished (it can be run again from Settings).
    pub completed: bool,
}

impl Settings {
    pub fn load() -> Result<Self> {
        Self::load_from(&paths::settings_file())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("{} is not valid settings JSON", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("could not read {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&paths::settings_file())
    }

    /// Write atomically: a temp file next to the target, then rename.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// Delete run folders older than the retention period. Returns how many went.
pub fn prune_runs(privacy: &Privacy) -> usize {
    if privacy.retention_days == 0 {
        return 0;
    }
    let max_age = std::time::Duration::from_secs(u64::from(privacy.retention_days) * 86_400);
    let Ok(entries) = std::fs::read_dir(privacy.runs_dir()) else {
        return 0;
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age > max_age);
        if meta.is_dir() && old && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::load_from(&dir.path().join("nope.json")).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(s.local_ai.idle_unload_secs, 60);
        assert_eq!(s.safety.approval_mode, "ask");
        assert_eq!(s.general.close_action, CloseAction::Ask);
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"general":{"close_action":"tray"}}"#).unwrap();
        let s = Settings::load_from(&path).unwrap();
        assert_eq!(s.general.close_action, CloseAction::Tray);
        assert_eq!(s.general.language, "en");
        assert_eq!(s.privacy.retention_days, 30);
    }

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("settings.json");
        let mut s = Settings::default();
        s.claude.model = "opus".into();
        s.connections
            .secret_names
            .insert("notion".into(), vec!["NOTION_TOKEN".into()]);
        s.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path).unwrap(), s);
    }

    #[test]
    fn prune_keeps_recent_runs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("run-1")).unwrap();
        let privacy = Privacy {
            runs_dir: Some(dir.path().to_path_buf()),
            retention_days: 30,
        };
        assert_eq!(prune_runs(&privacy), 0);
        assert!(dir.path().join("run-1").exists());
    }
}
