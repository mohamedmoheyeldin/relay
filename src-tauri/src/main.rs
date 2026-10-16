// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod commands;
mod settings;

fn main() {
    if let Err(e) = app::run() {
        eprintln!("Relay failed to start: {e:#}");
        std::process::exit(1);
    }
}
