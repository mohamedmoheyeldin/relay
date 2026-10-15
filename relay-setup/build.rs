//! Embeds the NSIS installer (the payload that really installs Relay) and, for
//! Windows targets, runs tauri-build (icon, manifest, frontend, permissions).
//!
//! `RELAY_SETUP_PAYLOAD` is set by `scripts/build_installer.py` to the NSIS
//! installer it just built. Without it (lint, tests, a plain `cargo build`) an
//! empty payload is embedded and the app says it has nothing to install.

use std::path::{Path, PathBuf};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    embed("RELAY_SETUP_PAYLOAD", &out.join("payload.exe"));
    // MinGW builds only: the WebView2 loader, so the exe needs no DLL beside it
    // (see src/webview2_loader.rs).
    embed(
        "RELAY_SETUP_WEBVIEW2_LOADER",
        &out.join("WebView2Loader.dll"),
    );

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let windows =
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app-manifest.xml"));
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("tauri-build");
    }
}

/// Copy the file the env var names to `target`, or write an empty one.
fn embed(var: &str, target: &Path) {
    println!("cargo:rerun-if-env-changed={var}");
    match std::env::var(var) {
        Ok(path) if !path.is_empty() => {
            println!("cargo:rerun-if-changed={path}");
            std::fs::copy(&path, target).unwrap_or_else(|e| panic!("{var} {path}: {e}"));
        }
        _ => std::fs::write(target, b"").expect("write an empty placeholder"),
    }
}
