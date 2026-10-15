//! Facts about this PC that Settings shows: installed browsers, and the built-in
//! relay-screen server's launch line.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::mcp::{ServerSpec, RELAY_SCREEN};
use crate::settings::Settings;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BrowserInfo {
    pub name: String,
    pub path: String,
    /// Chromium-based browsers (Brave, Chrome, Edge, ...) can run Relay's browser lane.
    pub chromium: bool,
}

/// Whether a browser, by its registered name or path, is built on Chromium.
pub fn is_chromium(name: &str, path: &str) -> bool {
    let text = format!("{name} {path}").to_lowercase();
    [
        "brave", "chrome", "chromium", "edge", "msedge", "vivaldi", "opera", "arc",
    ]
    .iter()
    .any(|k| text.contains(k))
}

/// Browsers registered with Windows (`StartMenuInternet`), found, never hardcoded.
#[cfg(windows)]
pub fn browsers() -> Vec<BrowserInfo> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let mut found: Vec<BrowserInfo> = Vec::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let Ok(root) = RegKey::predef(hive).open_subkey(r"SOFTWARE\Clients\StartMenuInternet")
        else {
            continue;
        };
        for key in root.enum_keys().flatten() {
            let Ok(sub) = root.open_subkey(&key) else {
                continue;
            };
            let name: String = sub.get_value("").unwrap_or_else(|_| key.clone());
            let path: String = sub
                .open_subkey(r"shell\open\command")
                .and_then(|c| c.get_value(""))
                .unwrap_or_default();
            let path = path.trim_matches('"').to_string();
            // Internet Explorer is still registered on Windows 11 but can't be used.
            if path.to_lowercase().ends_with("iexplore.exe") {
                continue;
            }
            if !found.iter().any(|b| b.name == name) {
                let chromium = is_chromium(&name, &path);
                found.push(BrowserInfo {
                    name,
                    path,
                    chromium,
                });
            }
        }
    }
    found
}

#[cfg(not(windows))]
pub fn browsers() -> Vec<BrowserInfo> {
    Vec::new()
}

/// How Claude Code starts relay-screen: the bundled Python, with the settings that
/// relay-screen reads from its environment.
pub fn relay_screen_spec(python: &Path, settings: &Settings) -> ServerSpec {
    let mut env = BTreeMap::new();
    env.insert(
        "RELAY_APPROVAL".to_string(),
        if settings.safety.approval_mode == "off" {
            "off"
        } else {
            "ask"
        }
        .to_string(),
    );
    env.insert(
        "RELAY_RUNS_DIR".to_string(),
        settings.privacy.runs_dir().to_string_lossy().into_owned(),
    );
    env.insert(
        "RELAY_SETTINGS".to_string(),
        crate::paths::settings_file().to_string_lossy().into_owned(),
    );
    ServerSpec::Stdio {
        name: RELAY_SCREEN.into(),
        command: python.to_string_lossy().into_owned(),
        args: vec!["-m".into(), "relay_screen.server".into()],
        env,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_browsers_are_recognised() {
        assert!(is_chromium(
            "Brave Origin",
            r"C:\Program Files\BraveSoftware\Brave-Origin\Application\brave.exe"
        ));
        assert!(is_chromium("Microsoft Edge", "msedge.exe"));
        assert!(!is_chromium(
            "Firefox",
            r"C:\Program Files\Mozilla Firefox\firefox.exe"
        ));
    }

    #[test]
    fn relay_screen_spec_carries_settings() {
        let mut s = Settings::default();
        s.safety.approval_mode = "off".into();
        let spec = relay_screen_spec(Path::new("C:/Relay/python/pythonw.exe"), &s);
        match spec {
            ServerSpec::Stdio {
                name, args, env, ..
            } => {
                assert_eq!(name, "relay-screen");
                assert_eq!(args, ["-m", "relay_screen.server"]);
                assert_eq!(env["RELAY_APPROVAL"], "off");
                assert!(env.contains_key("RELAY_RUNS_DIR"));
            }
            _ => panic!("expected stdio"),
        }
    }
}
