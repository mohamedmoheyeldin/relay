//! Entry point used by Trunk. On non-wasm targets this is a no-op so the workspace builds everywhere.

#[cfg(target_arch = "wasm32")]
fn main() {
    relay_ui::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {}
