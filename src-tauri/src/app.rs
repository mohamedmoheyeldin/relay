//! Application wiring: tray, window, global stop hotkey, and the agent stack.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use relay_agent::{
    Agent, AgentConfig, AgentParts, ConfirmBridge, EventSink, LocalConfig, LocalProvider,
};
use relay_ai::claude::ClaudeProvider;
use relay_ai::{AiProvider, Timeouts};
use relay_mcp::audit::AuditLog;
use relay_mcp::{HostOptions, KillSwitch, McpServer, ToolHost};
use relay_os::driver::Driver;
use relay_protocol::UiEvent;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::{error, info, warn};

use crate::commands;
use crate::settings::{Paths, Settings};

/// Event name the chat window listens on.
pub const EVENT_NAME: &str = "relay-event";
/// Shown to the owner; matches [`kill_shortcut`].
pub const KILL_HOTKEY_LABEL: &str = "Ctrl+Alt+Shift+Esc";
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

pub struct AppState {
    pub agent: Arc<Agent>,
    pub host: Arc<ToolHost>,
    pub paths: Paths,
    pub settings: Mutex<Settings>,
    /// Where the cloud CLI was found, shown in Settings.
    pub cloud_path: String,
    // Keeps the MCP server alive for the life of the app.
    pub _mcp: Arc<McpServer>,
}

fn kill_shortcut() -> Shortcut {
    Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
        Code::Escape,
    )
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
            let _ = window.hide();
        } else {
            show_main(app);
        }
    }
}

pub fn emit_event(app: &AppHandle, event: &UiEvent) {
    match serde_json::to_string(event) {
        Ok(json) => {
            if let Err(e) = app.emit(EVENT_NAME, json) {
                warn!("could not deliver event to the window: {e}");
            }
        }
        Err(e) => error!("could not serialise event: {e}"),
    }
}

fn init_logging(paths: &Paths) {
    use tracing_subscriber::prelude::*;
    let file = tracing_appender::rolling::daily(&paths.logs_dir, "relay.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    // The guard must live as long as the process so buffered log lines are flushed.
    Box::leak(Box::new(guard));
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,relay=debug"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        )
        .try_init();
}

fn build_tray(app: &AppHandle) -> Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Relay", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop the agent", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Relay", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&open, &stop, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| anyhow!("the app has no icon"))?;
    TrayIconBuilder::with_id("relay")
        .icon(icon)
        .tooltip("Relay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "stop" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.agent.stop();
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.agent.stop();
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn setup(app: &mut tauri::App) -> Result<()> {
    let handle = app.handle().clone();
    let data_dir = app
        .path()
        .app_local_data_dir()
        .context("finding the data folder")?;
    let paths = Paths::new(data_dir);
    paths.ensure()?;
    init_logging(&paths);
    info!("Relay starting; data in {}", paths.data_dir.display());

    let settings = Settings::load(&paths.settings_file);
    let sink: EventSink = {
        let handle = handle.clone();
        Arc::new(move |event| emit_event(&handle, &event))
    };

    let desktop = relay_os::system_desktop().context("opening the desktop")?;
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::new(desktop.clone(), kill.flag()));
    let bridge = ConfirmBridge::new(sink.clone(), CONFIRM_TIMEOUT);
    let launch_id = format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default(),
        std::process::id()
    );
    let audit = Arc::new(AuditLog::new(paths.runs_dir.join(launch_id)));
    let host = Arc::new(ToolHost::new(
        desktop,
        driver,
        kill.clone(),
        bridge.clone(),
        Some(audit),
        HostOptions {
            approval_mode: settings.mode(),
            ..HostOptions::default()
        },
    ));
    let mcp = Arc::new(
        tauri::async_runtime::block_on(McpServer::start(host.clone()))
            .context("starting the tool server")?,
    );
    let claude = ClaudeProvider::discover();
    let cloud_path = claude.program().display().to_string();
    let cloud: Arc<dyn AiProvider> = Arc::new(claude);
    let local = LocalProvider::new(
        LocalConfig {
            base_url: settings.local_url.clone(),
            model: settings.local_model.clone(),
        },
        host.clone(),
    );
    let agent = Agent::new(
        AgentParts {
            cloud,
            local,
            route: settings.route(),
            mcp: mcp.clone(),
            kill,
            bridge,
        },
        AgentConfig {
            work_dir: paths.work_dir.clone(),
            user_mcp_config: Some(paths.mcp_config.clone()),
            timeouts: Timeouts::default(),
            model: None,
            kill_hotkey: KILL_HOTKEY_LABEL.to_string(),
        },
        sink,
    );
    app.manage(AppState {
        agent,
        host,
        paths,
        settings: Mutex::new(settings),
        cloud_path,
        _mcp: mcp,
    });

    if let Err(e) = app.global_shortcut().register(kill_shortcut()) {
        warn!("could not register the stop hotkey: {e}");
        emit_event(
            &handle,
            &UiEvent::Error {
                run_id: None,
                kind: relay_protocol::ErrorKind::Internal,
                message: format!(
                    "The emergency-stop hotkey {KILL_HOTKEY_LABEL} is in use by another program. The Stop button and tray menu still work."
                ),
            },
        );
    }

    build_tray(&handle)?;

    if let Some(window) = app.get_webview_window("main") {
        let hider = window.clone();
        let status_handle = handle.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = hider.hide();
            }
            WindowEvent::Focused(true) => {
                // Refresh readiness: the owner may have just signed in.
                let handle = status_handle.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = handle.try_state::<AppState>() {
                        let status = state.agent.status().await;
                        emit_event(&handle, &UiEvent::Status(status));
                    }
                });
            }
            _ => {}
        });
    }
    let start_hidden = std::env::args().any(|a| a == "--minimized");
    if !start_hidden {
        show_main(&handle);
    }
    Ok(())
}

pub fn run() -> Result<()> {
    let shortcut = kill_shortcut();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, pressed, event| {
                    if *pressed == shortcut && event.state() == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<AppState>() {
                            state.agent.stop();
                        }
                    }
                })
                .build(),
        )
        .setup(|app| setup(app).map_err(Into::into))
        .invoke_handler(tauri::generate_handler![
            commands::send_message,
            commands::stop_task,
            commands::answer_confirm,
            commands::new_conversation,
            commands::get_status,
            commands::get_engines,
            commands::set_approval_mode,
            commands::set_route_mode,
            commands::set_local_model,
            commands::set_local_url,
            commands::pull_model,
            commands::open_data_folder,
            commands::open_link,
        ])
        .run(tauri::generate_context!())
        .context("running the application")
}
