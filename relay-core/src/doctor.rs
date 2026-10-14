//! Settings > Diagnostics: the doctor checks and the "copy report" text.

use std::path::Path;

use serde::Serialize;

use crate::settings::Settings;
use crate::{claude, gpu, mcp, models, paths, runtime};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Check {
    Check {
        name: name.into(),
        ok,
        detail: detail.into(),
    }
}

pub async fn run(settings: &Settings, python: Option<&Path>, app_version: &str) -> Vec<Check> {
    let mut checks = vec![check("Relay", true, format!("version {app_version}"))];
    let state = claude::state().await;
    checks.push(match &state.version {
        Some(v) => check("Claude Code installed", true, format!("version {v}")),
        None => check(
            "Claude Code installed",
            false,
            "Not found. Run the setup again.",
        ),
    });
    if state.installed {
        checks.push(if state.auth.logged_in {
            check(
                "Claude Code signed in",
                true,
                state
                    .auth
                    .subscription_type
                    .clone()
                    .unwrap_or_else(|| "signed in".into()),
            )
        } else {
            check(
                "Claude Code signed in",
                false,
                "Not signed in. Use Settings > Claude.",
            )
        });
        let registered = mcp::exists(mcp::RELAY_SCREEN).await;
        let paused = settings
            .connections
            .paused
            .iter()
            .any(|s| s.name == mcp::RELAY_SCREEN);
        checks.push(check(
            "relay-screen connected to Claude",
            registered || paused,
            if registered {
                "registered at user scope"
            } else if paused {
                "paused in Settings > Connections"
            } else {
                "not registered; restart Relay to register it"
            },
        ));
    }
    checks.push(match python {
        Some(p) if p.is_file() => check("relay-screen bundle", true, p.display().to_string()),
        Some(p) => check(
            "relay-screen bundle",
            false,
            format!("missing: {}", p.display()),
        ),
        None => check("relay-screen bundle", false, "not found next to the app"),
    });
    let g = gpu::detect().await;
    checks.push(match &g {
        Some(g) => check(
            "GPU",
            true,
            format!(
                "{}, {:.1} GB VRAM ({:.1} GB free), driver {}",
                g.name,
                g.vram_total_mb as f64 / 1024.0,
                g.vram_free_mb as f64 / 1024.0,
                g.driver
            ),
        ),
        None => check("GPU", false, "No NVIDIA GPU found; local models are off"),
    });
    let rt = runtime::state();
    checks.push(check(
        "Local AI runtime",
        rt.installed,
        if rt.installed {
            format!(
                "llama.cpp {} ({:?})",
                rt.build,
                rt.flavor.unwrap_or(runtime::Flavor::Cuda)
            )
        } else {
            "not downloaded yet (Settings > Local AI)".into()
        },
    ));
    let dir = settings.local_ai.models_dir();
    let installed = models::installed(&dir);
    checks.push(check(
        "Local models",
        !installed.is_empty(),
        format!(
            "{} installed in {} ({:.1} GB)",
            installed.len(),
            dir.display(),
            paths::dir_size(&dir) as f64 / 1_073_741_824.0
        ),
    ));
    checks.push(check(
        "Settings file",
        true,
        paths::settings_file().display().to_string(),
    ));
    checks
}

pub fn report(checks: &[Check]) -> String {
    let mut out = String::from("Relay diagnostics\n\n");
    for c in checks {
        out.push_str(&format!(
            "[{}] {}: {}\n",
            if c.ok { "ok" } else { "!!" },
            c.name,
            c.detail
        ));
    }
    out.push_str(&format!(
        "\nOS: {} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_lists_checks() {
        let text = report(&[check("A", true, "fine"), check("B", false, "broken")]);
        assert!(text.contains("[ok] A: fine"));
        assert!(text.contains("[!!] B: broken"));
    }
}
