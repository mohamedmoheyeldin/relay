//! Relay's desktop app: a tray app with a main window (chat, first-run setup, settings).
//!
//! A small floating popup (`popup.rs`) shows what Relay is doing while it works.
//!
//! The UI talks to relay-core through the commands in `commands.rs`. Long jobs
//! (installs, downloads) report progress as `job` events; chat turns stream as `chat`
//! events; the sign-in terminal streams as `pty-*` events.

mod approvals;
mod commands;
mod jobs;
mod popup;
mod pty;
mod stage;
mod state;
mod tray;

use tauri::{Emitter, Manager, WindowEvent};

use relay_core::settings::CloseAction;
use state::AppState;

/// Passed by the "start at login" entry so Relay starts in the tray.
pub const MINIMIZED_ARG: &str = "--minimized";

pub fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .manage(AppState::load())
        .manage(approvals::Approvals::default())
        .setup(|app| {
            tray::create(app.handle())?;
            popup::create(app.handle())?;
            {
                use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
                let handle = app.handle().clone();
                // Another program may own the shortcut already; Relay works without it.
                let _ = app
                    .global_shortcut()
                    .on_shortcut(popup::SHORTCUT, move |_, _, event| {
                        if event.state == ShortcutState::Pressed {
                            popup::toggle(&handle);
                            let _ = handle.emit_to(popup::LABEL, "popup-focus", ());
                        }
                    });
            }
            let minimized = std::env::args().any(|a| a == MINIMIZED_ARG);
            let setup_done = app.state::<AppState>().settings().setup.completed;
            if !minimized || !setup_done {
                show_main(app.handle());
            }
            state::on_start(app.handle().clone());
            approvals::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                api.prevent_close();
                let app = window.app_handle();
                match app.state::<AppState>().settings().general.close_action {
                    CloseAction::Tray => {
                        let _ = window.hide();
                    }
                    CloseAction::Quit => app.exit(0),
                    // The first close asks: keep running in the tray, or quit.
                    CloseAction::Ask => {
                        let _ = window.emit("close-requested", ());
                    }
                }
            }
        })
        .invoke_handler(commands::handler())
        .build(tauri::generate_context!())
        .expect("error while running Relay")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                approvals::stop();
                // Unload the local model with Relay rather than leave it in VRAM.
                let op = tauri::async_runtime::block_on(async {
                    app.state::<AppState>().operator.lock().await.take()
                });
                if let Some(op) = op {
                    tauri::async_runtime::block_on(op.quit());
                }
            }
        });
}
