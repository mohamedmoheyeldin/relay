//! Application wiring: tray, window, global stop hotkey, and the agent stack.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use relay_agent::hybrid::HybridParts;
use relay_agent::local::engine::{EngineManager, MANAGED_PORT};
use relay_agent::{
    Agent, AgentConfig, AgentParts, ConfirmBridge, EventSink, Hybrid, LocalConfig, LocalProvider,
};
use relay_ai::claude::ClaudeProvider;
use relay_ai::codex::CodexProvider;
use relay_ai::{AiProvider, Timeouts};
use relay_mcp::audit::AuditLog;
use relay_mcp::{HostOptions, KillSwitch, McpServer, ToolHost};
use relay_os::driver::Driver;
use relay_protocol::UiEvent;
use relay_store::Store;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::{info, warn};

use crate::settings::{Paths, Settings};
use crate::setup::LocalSetup;
use crate::store_adapter::StoreAdapter;
use crate::{commands, hybrid_commands};

/// Event name the chat window listens on.
pub const EVENT_NAME: &str = "relay-event";
/// Shown to the owner; matches [`kill_shortcut`].
pub const KILL_HOTKEY_LABEL: &str = "Ctrl+Alt+Shift+Esc";
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// What this launch is for (kept in Tauri state so commands can read it).
pub struct ModeState(pub crate::installer::Mode);

/// Relay's data folder.
pub fn data_dir_of(app: &AppHandle) -> Result<std::path::PathBuf> {
    app.path()
        .app_local_data_dir()
        .context("finding the data folder")
}

/// Show the setup or removal window instead of the app.
fn setup_installer(app: &mut tauri::App, mode: crate::installer::Mode) -> Result<()> {
    app.manage(ModeState(mode));
    if let Some(window) = app.get_webview_window("main") {
        let title = match mode {
            crate::installer::Mode::Uninstall => "Remove Relay",
            _ => "Relay Setup",
        };
        let _ = window.set_title(title);
        let _ = window.set_resizable(false);
        let _ = window.set_maximizable(false);
        let _ = window.set_size(tauri::LogicalSize::new(560.0, 620.0));
        let _ = window.center();
        let _ = window.show();
    }
    Ok(())
}

pub struct AppState {
    /// Browser control for website tasks (switchable from Settings).
    pub browser: Arc<relay_agent::browser::SwitchableBrowser>,
    /// The hybrid assistant: routes, plans (local or cloud), validates, executes and verifies.
    pub hybrid: Arc<Hybrid>,
    /// The older tool-using agent, kept for the optional agent mode.
    pub agent: Arc<Agent>,
    pub host: Arc<ToolHost>,
    pub paths: Paths,
    pub settings: Arc<Mutex<Settings>>,
    pub store: Arc<Store>,
    pub setup: Arc<LocalSetup>,
    /// Where the cloud CLI was found, shown in Settings.
    pub cloud_path: String,
    // Keeps the MCP server alive for the life of the app.
    pub _mcp: Arc<McpServer>,
}

impl AppState {
    /// Stop whatever is running, in either engine.
    pub fn stop_all(&self) {
        self.hybrid.stop();
        self.agent.stop();
    }

    pub fn is_busy(&self) -> bool {
        self.hybrid.is_busy() || self.agent.is_busy()
    }

    /// Change and persist the settings.
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) -> Result<(), String> {
        let mut settings = self.settings.lock().map_err(|e| e.to_string())?;
        change(&mut settings);
        settings
            .save(&self.paths.settings_file)
            .map_err(|e| e.to_string())
    }
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
    if let Err(e) = app.emit(EVENT_NAME, event) {
        warn!("could not deliver event to the window: {e}");
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
                    state.stop_all();
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.stop_all();
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
    // Match the window's own background to the OS theme so there is no light flash before the page paints.
    if let Some(window) = app.get_webview_window("main") {
        let dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
        let (r, g, b) = if dark { (28, 26, 23) } else { (250, 247, 242) };
        let _ = window.set_background_color(Some(tauri::window::Color(r, g, b, 255)));
    }
    let args: Vec<String> = std::env::args().collect();
    let mode = crate::installer::detect_mode(
        &args,
        &std::env::current_exe().unwrap_or_default(),
        &crate::installer::install_dir(),
        cfg!(debug_assertions),
    );
    if mode != crate::installer::Mode::App {
        return setup_installer(app, mode);
    }
    app.manage(ModeState(mode));
    let handle = app.handle().clone();
    let data_dir = app
        .path()
        .app_local_data_dir()
        .context("finding the data folder")?;
    let paths = Paths::new(data_dir);
    paths.ensure()?;
    init_logging(&paths);
    info!("Relay starting; data in {}", paths.data_dir.display());

    let settings = Arc::new(Mutex::new(Settings::load(&paths.settings_file)));
    let start = settings.lock().map(|s| s.clone()).unwrap_or_default();
    let store = Arc::new(Store::open(&paths.database).context("opening the chat database")?);
    let sink: EventSink = {
        let handle = handle.clone();
        Arc::new(move |event| emit_event(&handle, &event))
    };

    let desktop = relay_os::system_desktop().context("opening the desktop")?;
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::new(desktop.clone(), kill.flag()));
    let native_driver = driver.clone();
    let native_desktop = desktop.clone();
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
            approval_mode: start.mode(),
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
            base_url: start.local_url.clone(),
            model: start.local_model.clone(),
        },
        host.clone(),
    );
    let engine = Arc::new(EngineManager::new(
        paths.engine_dir.clone(),
        paths.logs_dir.join("local-ai.log"),
        MANAGED_PORT,
    ));
    let setup = LocalSetup::new(handle.clone(), engine, local.clone());
    let conv_store = Arc::new(StoreAdapter::new(store.clone(), settings.clone()));
    let browser = Arc::new(relay_agent::browser::SwitchableBrowser::new(
        Arc::new(relay_agent::browser::PlaywrightBrowser::new(
            paths.data_dir.join("browser-profile"),
        )),
        start.hybrid.browser_enabled,
    ));
    let hybrid = Hybrid::new(HybridParts {
        services: Arc::new(relay_orchestrator::system::SystemServices::new(
            native_desktop,
            native_driver,
            start.hybrid.allowed_roots.iter().map(Into::into).collect(),
        )),
        local: local.clone(),
        claude: Arc::new(ClaudeProvider::discover()),
        codex: Arc::new(CodexProvider::discover()),
        bridge: bridge.clone(),
        kill: kill.clone(),
        conv_store: conv_store.clone(),
        workflows: conv_store.clone(),
        work_dir: paths.work_dir.clone(),
        settings: crate::settings::engine_settings(&start.hybrid),
        sink: sink.clone(),
        browser: browser.clone(),
    });
    let agent = Agent::new(
        AgentParts {
            cloud,
            local,
            route: start.route(),
            mcp: mcp.clone(),
            kill,
            bridge,
            store: conv_store,
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
        browser,
        hybrid,
        agent,
        host,
        paths,
        settings,
        store,
        setup: setup.clone(),
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

    // Ask both cloud CLIs for their sign-in state now, so the first task does not wait for it.
    if let Some(state) = handle.try_state::<AppState>() {
        let hybrid = state.hybrid.clone();
        tauri::async_runtime::spawn(async move {
            let _ = tokio::join!(
                hybrid.provider_status(relay_orchestrator::router::ProviderKind::Claude, true),
                hybrid.provider_status(relay_orchestrator::router::ProviderKind::Codex, true)
            );
        });
    }

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
    // Bring the built-in local AI up in the background; install it first if it is missing.
    if start.auto_setup {
        let setup = setup.clone();
        tauri::async_runtime::spawn(async move {
            if setup.engine().is_installed() {
                if let Err(e) = setup.engine().start().await {
                    warn!("local AI did not start: {e}");
                }
                let has_model = setup
                    .local_client()
                    .models()
                    .await
                    .map(|m| m.iter().any(|m| m.tools))
                    .unwrap_or(false);
                if !has_model {
                    setup.start();
                }
            } else {
                setup.start();
            }
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
    let args: Vec<String> = std::env::args().collect();
    let mode = crate::installer::detect_mode(
        &args,
        &std::env::current_exe().unwrap_or_default(),
        &crate::installer::install_dir(),
        cfg!(debug_assertions),
    );
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // Only the running app is a singleton: setup must still open while an older copy runs.
    if mode == crate::installer::Mode::App {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }));
    }
    builder
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, pressed, event| {
                    if *pressed == shortcut && event.state() == ShortcutState::Pressed {
                        if let Some(state) = app.try_state::<AppState>() {
                            state.stop_all();
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
            commands::get_status,
            hybrid_commands::get_platform,
            hybrid_commands::get_hybrid,
            hybrid_commands::set_hybrid,
            hybrid_commands::refresh_providers,
            hybrid_commands::workflow_list,
            hybrid_commands::workflow_get,
            hybrid_commands::get_appearance,
            hybrid_commands::set_appearance,
            commands::conv_list,
            commands::conv_get,
            commands::conv_create,
            commands::conv_save,
            commands::conv_update,
            commands::conv_move,
            commands::conv_delete,
            commands::search,
            commands::project_list,
            commands::project_create,
            commands::project_update,
            commands::project_delete,
            commands::get_engines,
            commands::set_route_mode,
            commands::set_local_model,
            commands::local_ai_setup,
            commands::pull_model,
            commands::get_settings,
            commands::set_settings,
            commands::mcp_get,
            commands::mcp_set,
            commands::pick_files,
            commands::open_link,
            commands::open_url,
            commands::installer_info,
            commands::installer_install,
            commands::installer_launch,
            commands::uninstall_run,
            commands::app_quit,
        ])
        .build(tauri::generate_context!())
        .context("building the application")?
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.stop_all();
                    // Close the browser Relay was driving.
                    tauri::async_runtime::block_on(relay_agent::browser::BrowserDriver::shutdown(
                        state.browser.as_ref(),
                    ));
                    // Stop the built-in local AI server with the app.
                    tauri::async_runtime::block_on(state.setup.engine().stop());
                }
            }
        });
    Ok(())
}
