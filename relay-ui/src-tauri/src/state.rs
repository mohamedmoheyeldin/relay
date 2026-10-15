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

/// Register, update or remove relay-browser to match Settings > Browser.
/// Left alone while the owner has it paused in Connections.
pub async fn ensure_relay_browser(app: &AppHandle) -> anyhow::Result<bool> {
    let settings = app.state::<AppState>().settings();
    if claude::find().is_none()
        || settings
            .connections
            .paused
            .iter()
            .any(|p| p.name == mcp::RELAY_BROWSER)
    {
        return Ok(false);
    }
    let lane = browser::state(&settings);
    let have = mcp::user_config(mcp::RELAY_BROWSER).ok();
    let want = match (&lane.browser, browser::npx()) {
        (Some(b), Some(npx)) if lane.enabled => {
            Some(browser::spec(&npx, std::path::Path::new(&b.path), &settings).config_json(None))
        }
        _ => None,
    };
    if have == want {
        return Ok(want.is_some());
    }
    if have.is_some() {
        mcp::remove(mcp::RELAY_BROWSER, &[], true).await?;
    }
    if let Some(want) = &want {
        mcp::add_json(mcp::RELAY_BROWSER, want).await?;
    }
    Ok(want.is_some())
}

pub fn on_start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let s = app.state::<AppState>().settings();
        let _ = std::fs::create_dir_all(relay_core::paths::work_dir());
        settings::prune_runs(&s.privacy);
        if let Err(err) = ensure_relay_screen(&app).await {
            eprintln!("relay: could not register relay-screen: {err:#}");
        }
        if let Err(err) = ensure_relay_browser(&app).await {
            eprintln!("relay: could not register relay-browser: {err:#}");
        }
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
