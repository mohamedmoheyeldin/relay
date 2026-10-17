// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod hybrid_commands;
mod installer;
mod settings;
mod setup;
mod store_adapter;

/// The WebView2 remote-debugging switch is the only test hook the desktop E2E layer needs. Builds
/// made without the `e2e` feature drop it, so a shipped Relay cannot be driven from outside.
fn strip_test_hooks() {
    if cfg!(feature = "e2e") {
        return;
    }
    const VAR: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    if std::env::var(VAR).is_ok_and(|v| v.contains("remote-debugging")) {
        // SAFETY: called first thing in `main`, before any thread is started.
        unsafe { std::env::remove_var(VAR) };
    }
}

fn main() {
    strip_test_hooks();
    // Elevated setup or removal workers do their job headlessly and exit.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = installer::run_worker_if_requested(&args, env!("CARGO_PKG_VERSION")) {
        std::process::exit(code);
    }
    if let Err(e) = app::run() {
        eprintln!("Relay failed to start: {e:#}");
        std::process::exit(1);
    }
}
