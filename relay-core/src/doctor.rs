//! Settings > Diagnostics: the doctor checks and the "copy report" text.

use std::path::Path;

use serde::Serialize;

use crate::settings::Settings;
use crate::{cloud, gpu, local_ai, mcp, paths};

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
    let state = cloud::state().await;
    checks.push(match &state.version {
        Some(v) => check("Cloud AI installed", true, format!("version {v}")),
        None => check(
            "Cloud AI installed",
            false,
            "Not found. Run the setup again.",
        ),
    });
    if state.installed {
        checks.push(if state.auth.logged_in {
            check(
                "Cloud AI signed in",
                true,
                state
                    .auth
                    .subscription_type
                    .clone()
                    .unwrap_or_else(|| "signed in".into()),
            )
        } else {
            check(
                "Cloud AI signed in",
                false,
                "Not signed in. Use Settings > Cloud AI.",
            )
        });
        let registered = mcp::exists(mcp::RELAY_SCREEN).await;
        let paused = settings
            .connections
            .paused
            .iter()
            .any(|s| s.name == mcp::RELAY_SCREEN);
        checks.push(check(
            "relay-screen connected to Cloud AI",
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
    if cfg!(windows) {
        if let Some(python) = python.filter(|p| p.is_file()) {
            checks.push(approval_prompt(python).await);
        }
    }
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
    checks.push(check(
        "Settings file",
        true,
        paths::settings_file().display().to_string(),
    ));
    checks
}

/// Builds Relay's approval prompt off screen (`prompt --check`), so a prompt that can't
/// open shows up here instead of as every action being refused.
async fn approval_prompt(python: &Path) -> Check {
    let python = crate::local_ai::cli_python(python);
    let run = crate::proc::run(&python, &["-m", "relay_screen.safety.prompt", "--check"]);
    let out = match tokio::time::timeout(std::time::Duration::from_secs(30), run).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return check("Approval prompt", false, format!("{e:#}")),
        Err(_) => return check("Approval prompt", false, "the check did not finish in 30 s"),
    };
    let answer: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap_or_default();
    if answer["ok"] == true {
        return check(
            "Approval prompt",
            true,
            "ready to ask you before Cloud AI acts",
        );
    }
    let reason = answer["error"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            out.stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "it stopped without saying why".into());
    check(
        "Approval prompt",
        false,
        format!("can't open, so Cloud AI's actions would be refused: {reason}"),
    )
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
