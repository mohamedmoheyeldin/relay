//! Relay's chat window (Leptos, compiled to WebAssembly by Trunk).

pub mod chat;

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod bridge;

/// Mount the UI into the page.
#[cfg(target_arch = "wasm32")]
pub fn start() {
    app::start();
}
