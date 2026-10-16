//! App-wide state and the work done at start-up.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use relay_core::download::Cancel;
use relay_core::settings::Settings;
use relay_core::{browser, chat, claude, mcp, settings, system, updates};

pub struct AppState {
    settings: Mutex<Settings>,
    pub jobs: Mutex<HashMap<String, Cancel>>,
    pub turn: tokio::sync::Mutex<Option<chat::Turn>>,
    pub session: Mutex<Option<String>>,
    /// The owner pressed Stop, so a failed Claude turn must not fall back to the local AI.
    pub stopped: std::sync::atomic::AtomicBool,
    pub pty: Mutex<Option<crate::pty::Session>>,
    /// The operator service (`commands::operator`), once started.
    pub operator: tokio::sync::Mutex<Option<std::sync::Arc<relay_core::operator::Operator>>>,
    /// When the operator service last failed to start, so it isn't retried on every message.
    pub operator_failed: Mutex<Option<std::time::Instant>>,
    /// Whether Claude Code was signed in, and when that was checked (`commands::claude_signed_in`).
    pub claude_signed_in: Mutex<Option<(bool, std::time::Instant)>>,
    /// A Claude turn of this app is running, so its `ui_task` steps belong in the chat.
    pub claude_turn: std::sync::atomic::AtomicBool,
}

impl AppState {
    pub fn load() -> Self {
        Self {
            settings: Mutex::new(Settings::load().unwrap_or_default()),
            jobs: Mutex::default(),
            turn: tokio::sync::Mutex::default(),
            session: Mutex::default(),
            stopped: Default::default(),
            pty: Mutex::default(),
            operator: tokio::sync::Mutex::default(),
            operator_failed: Mutex::default(),
            claude_signed_in: Mutex::default(),
            claude_turn: Default::default(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().expect("settings lock").clone()
    }

    /// Change settings and save them; returns the saved copy.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> anyhow::Result<Settings> {
        let mut guard = self.settings.lock().expect("settings lock");
        let mut next = guard.clone();
        change(&mut next);
        next.save()?;
        *guard = next.clone();
        Ok(next)
    }
}

/// The bundled Python that runs relay-screen (`<install dir>\python\pythonw.exe`).
pub fn python_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = relay_core::paths::plain(app.path().resource_dir().ok()?).join("python");
    let exe = if cfg!(windows) {
        "pythonw.exe"
    } else {
        "bin/python3"
    };
    Some(dir.join(exe))
}

/// This executable, which doubles as the MCP secrets launcher.
pub fn launcher_path() -> PathBuf {
    std::env::current_exe()
        .map(relay_core::paths::plain)
        .unwrap_or_else(|_| PathBuf::from("relay"))
}

/// Make sure Claude Code starts relay-screen the way the current settings say.
/// Skipped when Claude Code or the bundle is missing, or the owner paused it.
pub async fn ensure_relay_screen(app: &AppHandle) -> anyhow::Result<bool> {
    let settings = app.state::<AppState>().settings();
    if settings
        .connections
        .paused
        .iter()
        .any(|p| p.name == mcp::RELAY_SCREEN)
    {
        return Ok(false);
    }
    let Some(python) = python_path(app).filter(|p| p.is_file()) else {
        return Ok(false);
    };
    if claude::find().is_none() {
        return Ok(false);
    }
    let want = system::relay_screen_spec(&python, &settings).config_json(None);
    match mcp::user_config(mcp::RELAY_SCREEN) {
        Ok(have) if have == want => return Ok(true),
        Ok(_) => mcp::remove(mcp::RELAY_SCREEN, &[], true).await?,
        Err(_) => {}
    }
    mcp::add_json(mcp::RELAY_SCREEN, &want).await?;
    Ok(true)
}

/// Take out what the old browser lane left behind (D-089): relay-browser in Claude
/// Code's config, its paused entry and the extension's token.
pub async fn retire_relay_browser(app: &AppHandle) -> anyhow::Result<()> {
    let st = app.state::<AppState>();
    let settings = st.settings();
    let paused = settings
        .connections
        .paused
        .iter()
        .any(|p| p.name == mcp::RELAY_BROWSER);
    if paused
        || settings
            .connections
            .secret_names
            .contains_key(mcp::RELAY_BROWSER)
    {
        browser::forget_token();
        st.update(|s| {
            s.connections
                .paused
                .retain(|p| p.name != mcp::RELAY_BROWSER);
            s.connections.secret_names.remove(mcp::RELAY_BROWSER);
        })?;
    }
    if claude::find().is_some() && mcp::user_config(mcp::RELAY_BROWSER).is_ok() {
        mcp::remove(mcp::RELAY_BROWSER, &[], true).await?;
    }
    Ok(())
}

pub fn on_start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let s = app.state::<AppState>().settings();
        let _ = std::fs::create_dir_all(relay_core::paths::work_dir());
        settings::prune_runs(&s.privacy);
        if let Err(err) = ensure_relay_screen(&app).await {
            eprintln!("relay: could not register relay-screen: {err:#}");
        }
        if let Err(err) = retire_relay_browser(&app).await {
            eprintln!("relay: could not remove relay-browser: {err:#}");
        }
        // Warm the local model and the screen state before the first message.
        crate::commands::operator(&app).await;
        if s.updates.check_on_start {
            let version = app.package_info().version.to_string();
            if let Ok(info) = updates::check(&version).await {
                if info.available {
                    let _ = app.emit("update-available", info);
                }
            }
        }
    });
}
