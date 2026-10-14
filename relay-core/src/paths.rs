//! Where Relay keeps its files (D-046, D-048).
//!
//! - `%APPDATA%\Relay`: settings.json and playbooks (roams with the profile).
//! - `%LOCALAPPDATA%\Relay`: models, the llama.cpp runtime, logs and run traces.
//!
//! `RELAY_HOME` overrides both roots, which tests use.

use std::path::PathBuf;

fn home_override() -> Option<PathBuf> {
    std::env::var_os("RELAY_HOME").map(PathBuf::from)
}

/// Settings and playbooks.
pub fn config_dir() -> PathBuf {
    match home_override() {
        Some(home) => home.join("config"),
        None => dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Relay"),
    }
}

/// Large local data: models, runtime, logs, runs.
pub fn data_dir() -> PathBuf {
    match home_override() {
        Some(home) => home.join("data"),
        None => dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("Relay"),
    }
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn default_models_dir() -> PathBuf {
    data_dir().join("models")
}

pub fn runtime_dir() -> PathBuf {
    data_dir().join("runtime")
}

pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

pub fn default_runs_dir() -> PathBuf {
    data_dir().join("runs")
}

pub fn playbooks_dir() -> PathBuf {
    config_dir().join("playbooks")
}

/// The folder `claude -p` runs in for chat tasks, so Claude Code never sees a project.
pub fn work_dir() -> PathBuf {
    data_dir().join("work")
}

/// The owner's Downloads folder: where an app update installer is saved.
pub fn downloads_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| data_dir().join("downloads"))
}

/// Total size in bytes of every file under `path` (0 when it does not exist).
pub fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}
