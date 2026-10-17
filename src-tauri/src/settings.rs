//! On-disk locations and persisted settings.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use relay_agent::local::engine::MANAGED_PORT;
use relay_agent::RouteMode;
use relay_mcp::ApprovalMode;
use relay_orchestrator::config::HybridConfig;
use relay_orchestrator::permissions::ConfirmLevel;
use relay_orchestrator::router::ProviderKind;
use serde::{Deserialize, Serialize};

/// Folders and files Relay uses under its data directory.
#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    /// Working directory handed to the AI CLI (kept empty so no project files leak in).
    pub work_dir: PathBuf,
    pub runs_dir: PathBuf,
    pub logs_dir: PathBuf,
    /// The built-in local AI engine and its models.
    pub engine_dir: PathBuf,
    pub database: PathBuf,
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
            engine_dir: data_dir.join("engine"),
            database: data_dir.join("relay.db"),
            mcp_config: data_dir.join("mcp.json"),
            settings_file: data_dir.join("settings.json"),
            data_dir,
        }
    }

    /// Create the folders and a starter MCP file so the owner can see where to add servers.
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.work_dir,
            &self.runs_dir,
            &self.logs_dir,
            &self.engine_dir,
        ] {
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

fn default_user() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "You".to_string())
}

/// Translate the stored configuration into the engine's runtime settings.
pub fn engine_settings(cfg: &HybridConfig) -> relay_agent::HybridSettings {
    let provider_opts = cfg
        .providers
        .iter()
        .filter_map(|(id, c)| {
            Some((
                ProviderKind::parse(id)?,
                relay_agent::hybrid::ProviderOpts {
                    model: c.model.clone(),
                    reasoning: c.reasoning.clone(),
                },
            ))
        })
        .collect();
    relay_agent::HybridSettings {
        privacy: cfg.privacy_mode,
        default_provider: cfg.provider(),
        confirm: cfg.confirm_level,
        share: cfg.share,
        allowed_roots: cfg.allowed_roots.iter().map(PathBuf::from).collect(),
        protected_roots: cfg.protected_roots.iter().map(PathBuf::from).collect(),
        disabled_tools: cfg.disabled_tools.iter().cloned().collect(),
        provider_opts,
    }
}

/// Settings stored in `settings.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub approval_mode: String,
    /// `auto`, `local_only` or `cloud_only`.
    pub route_mode: String,
    /// Local model name; `None` picks the best installed one.
    pub local_model: Option<String>,
    pub local_url: String,
    pub user_name: String,
    pub custom_instructions: String,
    /// Download and run the built-in local AI automatically.
    pub auto_setup: bool,
    pub hybrid: HybridConfig,
    /// How the app looks (themes, fonts, sizes, views). Owned by the UI; stored as given.
    pub appearance: serde_json::Value,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            approval_mode: mode_to_str(ApprovalMode::Destructive).to_string(),
            route_mode: RouteMode::Auto.as_str().to_string(),
            local_model: None,
            local_url: format!("http://127.0.0.1:{MANAGED_PORT}"),
            user_name: default_user(),
            custom_instructions: String::new(),
            auto_setup: true,
            hybrid: HybridConfig::default(),
            appearance: serde_json::json!({}),
        }
    }
}

/// A partial update from the UI.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsPatch {
    pub approval_mode: Option<String>,
    pub user_name: Option<String>,
    pub custom_instructions: Option<String>,
    pub auto_setup: Option<bool>,
}

impl Settings {
    pub fn route(&self) -> RouteMode {
        RouteMode::parse(&self.route_mode).unwrap_or_default()
    }

    pub fn mode(&self) -> ApprovalMode {
        mode_from_str(&self.approval_mode).unwrap_or(ApprovalMode::Destructive)
    }

    /// Apply a patch; unknown approval modes are rejected.
    pub fn apply(&mut self, patch: SettingsPatch) -> Result<()> {
        if let Some(mode) = patch.approval_mode {
            mode_from_str(&mode).with_context(|| format!("unknown approval mode {mode:?}"))?;
            if let Some(parsed) = mode_from_str(&mode) {
                self.hybrid.confirm_level = match parsed {
                    ApprovalMode::Off => ConfirmLevel::Off,
                    ApprovalMode::Destructive => ConfirmLevel::Destructive,
                    ApprovalMode::Always => ConfirmLevel::Always,
                };
            }
            self.approval_mode = mode;
        }
        if let Some(name) = patch.user_name {
            self.user_name = name.trim().to_string();
        }
        if let Some(text) = patch.custom_instructions {
            self.custom_instructions = text;
        }
        if let Some(auto) = patch.auto_setup {
            self.auto_setup = auto;
        }
        Ok(())
    }

    /// Load settings; a missing or unreadable file yields the defaults.
    pub fn load(path: &Path) -> Self {
        let mut s: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        s.migrate();
        s
    }

    /// Older builds pointed at a separately installed Ollama on its standard port. Relay now runs its own
    /// engine, so that address would leave "Relay Local AI" looking for a server that is not there.
    fn migrate(&mut self) {
        let legacy = ["http://127.0.0.1:11434", "http://localhost:11434"];
        if legacy.contains(&self.local_url.trim_end_matches('/')) {
            self.local_url = format!("http://127.0.0.1:{MANAGED_PORT}");
        }
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
    fn defaults_confirm_destructive_steps_and_route_automatically() {
        let s = Settings::default();
        assert_eq!(s.mode(), ApprovalMode::Destructive);
        assert_eq!(s.hybrid.confirm_level, ConfirmLevel::Destructive);
        assert_eq!(
            s.hybrid.privacy_mode,
            relay_orchestrator::permissions::PrivacyMode::HybridAuto
        );
        assert!(!s.hybrid.share.file_contents && !s.hybrid.share.screenshots);
        assert!(!s.hybrid.protected_roots.is_empty());
        assert_eq!(s.route(), RouteMode::Auto);
        assert!(s.auto_setup);
        assert!(s.local_url.ends_with(&MANAGED_PORT.to_string()));
    }

    #[test]
    fn missing_or_corrupt_settings_fall_back_to_defaults() {
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
    fn the_old_standard_ollama_address_moves_to_relays_own_engine() {
        let dir = temp("legacy-url");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, r#"{"local_url":"http://127.0.0.1:11434"}"#).unwrap();
        assert!(Settings::load(&file)
            .local_url
            .ends_with(&MANAGED_PORT.to_string()));
        std::fs::write(&file, r#"{"local_url":"http://192.168.1.5:11434"}"#).unwrap();
        assert_eq!(
            Settings::load(&file).local_url,
            "http://192.168.1.5:11434",
            "a custom address is kept"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_settings_files_without_new_fields_still_load() {
        let dir = temp("old");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, r#"{"approval_mode":"always"}"#).unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.mode(), ApprovalMode::Always);
        assert_eq!(s.route(), RouteMode::Auto);
        assert!(s.auto_setup);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_persist_and_patch() {
        let dir = temp("persist");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        let mut s = Settings::default();
        s.apply(SettingsPatch {
            approval_mode: Some("always".into()),
            user_name: Some("  Moe ".into()),
            custom_instructions: Some("Be brief".into()),
            auto_setup: Some(false),
        })
        .unwrap();
        s.route_mode = "local_only".into();
        s.local_model = Some("qwen3:14b".into());
        s.save(&file).unwrap();
        let back = Settings::load(&file);
        assert_eq!(back.mode(), ApprovalMode::Always);
        assert_eq!(back.hybrid.confirm_level, ConfirmLevel::Always);
        assert_eq!(back.route(), RouteMode::LocalOnly);
        assert_eq!(back.user_name, "Moe");
        assert_eq!(back.custom_instructions, "Be brief");
        assert!(!back.auto_setup);
        assert_eq!(back.local_model.as_deref(), Some("qwen3:14b"));
        assert!(s
            .apply(SettingsPatch {
                approval_mode: Some("nope".into()),
                ..SettingsPatch::default()
            })
            .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_creates_folders_and_a_starter_mcp_file_once() {
        let dir = temp("paths");
        let paths = Paths::new(&dir);
        paths.ensure().unwrap();
        assert!(paths.work_dir.is_dir() && paths.runs_dir.is_dir() && paths.engine_dir.is_dir());
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
