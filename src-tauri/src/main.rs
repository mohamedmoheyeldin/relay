// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod hybrid_commands;
mod installer;
mod settings;
mod setup;
mod store_adapter;

fn main() {
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
