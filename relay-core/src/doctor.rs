//! Settings > Diagnostics: the doctor checks and the "copy report" text.

use std::path::Path;

use serde::Serialize;

use crate::settings::Settings;
use crate::{claude, gpu, local_ai, mcp, paths};

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
    let listing = match python.filter(|p| p.is_file()) {
        Some(python) => local_ai::list(python).await.map_err(|e| format!("{e:#}")),
        None => Err("relay-screen is not bundled with this build".into()),
    };
    match &listing {
        Ok(l) => {
            checks.push(match &l.runtime {
                Some(rt) => check(
                    "Local AI runtime",
                    true,
                    format!("llama.cpp {} (CUDA {})", rt.tag, rt.cuda),
                ),
                None => check(
                    "Local AI runtime",
                    false,
                    "not downloaded yet (Settings > Local AI)",
                ),
            });
            let ready = l.installed.iter().filter(|m| m.ready).count();
            checks.push(check(
                "Local models",
                ready > 0,
                format!(
                    "{ready} downloaded in {} ({:.1} GB)",
                    l.models_folder.display(),
                    paths::dir_size(&l.models_folder) as f64 / 1_073_741_824.0
                ),
            ));
        }
        Err(e) => checks.push(check("Local AI runtime", false, e.clone())),
    }
    let lane = crate::browser::state(settings);
    checks.push(match (&lane.browser, &lane.problem) {
        _ if !lane.enabled => check("Browser lane", true, "turned off in Settings > Browser"),
        (Some(b), None) => check("Browser lane", true, b.name.clone()),
        (_, Some(problem)) => check("Browser lane", false, problem.clone()),
        (None, None) => check("Browser lane", false, "no browser found"),
    });
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
