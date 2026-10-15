//! The Tauri side of Relay Setup: what the page can ask for.

use crate::state::{self, Action, Plan, Scope};
use crate::win;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{Emitter, Manager};

/// The NSIS installer `scripts/build_installer.py` built, embedded by build.rs.
static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.exe"));

/// The Relay inside this setup: the workspace version, same as the payload's.
const PAYLOAD_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct Detection {
    payload_version: &'static str,
    has_payload: bool,
    #[serde(flatten)]
    plan: Plan,
    default_location: String,
    /// Relay (or its relay-screen) is running from an install folder now.
    running: bool,
}

#[derive(Serialize)]
struct Outcome {
    ok: bool,
    code: u32,
    message: String,
    /// The version installed afterwards, if any.
    version: Option<String>,
}

#[derive(Clone, Serialize)]
struct Step {
    id: &'static str,
    state: &'static str,
}

fn install_dirs() -> Vec<String> {
    let (machine, user) = win::installs();
    [machine, user]
        .into_iter()
        .flatten()
        .filter_map(|i| i.location)
        .chain(Some(win::default_location()))
        .collect()
}

fn detection() -> Detection {
    let (machine, user) = win::installs();
    let plan = state::plan(machine, user, PAYLOAD_VERSION);
    Detection {
        payload_version: PAYLOAD_VERSION,
        has_payload: !PAYLOAD.is_empty(),
        plan,
        default_location: win::default_location(),
        running: !win::running_from(&install_dirs()).is_empty(),
    }
}

#[tauri::command]
fn detect() -> Detection {
    detection()
}

#[tauri::command]
async fn apply(window: tauri::Window, action: String) -> Result<Outcome, String> {
    let action = Action::parse(&action).ok_or_else(|| format!("Unknown action {action:?}"))?;
    let owner = window.hwnd().map(|h| h.0 as isize).unwrap_or(0);
    tauri::async_runtime::spawn_blocking(move || {
        let step = |id: &'static str, state: &'static str| {
            let _ = window.emit("setup-step", Step { id, state });
        };
        perform(action, owner, &step)
    })
    .await
    .map_err(|e| e.to_string())
}

fn failed(code: u32, message: impl Into<String>) -> Outcome {
    Outcome {
        ok: false,
        code,
        message: message.into(),
        version: None,
    }
}

fn perform(action: Action, owner: isize, step: &dyn Fn(&'static str, &'static str)) -> Outcome {
    step("close", "active");
    win::close_running(&install_dirs());
    step("close", "done");

    let result = if action.runs_payload() {
        run_payload(owner, step)
    } else {
        run_uninstall(owner, step)
    };
    let code = match result {
        Ok(code) => code,
        Err(outcome) => return outcome,
    };
    if code != 0 {
        return failed(code, state::explain_exit(code));
    }

    step("finish", "active");
    let after = detection();
    step("finish", "done");
    let version = after.plan.installed.map(|i| i.version);
    match action {
        Action::Uninstall if version.is_some() => failed(
            code,
            "The uninstaller finished, but Relay is still registered.",
        ),
        Action::Uninstall => Outcome {
            ok: true,
            code,
            message: "Relay was removed. Your settings and models were kept.".into(),
            version: None,
        },
        _ if version.as_deref() != Some(PAYLOAD_VERSION) => failed(
            code,
            "The installer finished, but Windows doesn't list the new Relay.",
        ),
        _ => Outcome {
            ok: true,
            code,
            message: format!("Relay {PAYLOAD_VERSION} is installed."),
            version,
        },
    }
}

fn temp_payload() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "Relay-Setup-{PAYLOAD_VERSION}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("Relay-Setup-nsis-{PAYLOAD_VERSION}.exe"));
    std::fs::write(&path, PAYLOAD)?;
    Ok(path)
}

fn run_payload(owner: isize, step: &dyn Fn(&'static str, &'static str)) -> Result<u32, Outcome> {
    if PAYLOAD.is_empty() {
        return Err(failed(
            0,
            "This copy of Relay Setup has no installer inside. Download Relay-Setup.exe again.",
        ));
    }
    step("prepare", "active");
    let payload = temp_payload().map_err(|e| failed(0, format!("Couldn't unpack: {e}")))?;
    step("prepare", "done");
    step("permission", "active");
    let started = || {
        step("permission", "done");
        step("work", "active");
    };
    let result = win::run_elevated(&payload, "/S", owner, false, started);
    if let Some(dir) = payload.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    match result {
        Ok(code) => {
            step("work", "done");
            Ok(code)
        }
        Err(code) => Err(failed(code, state::explain_exit(code))),
    }
}

fn run_uninstall(owner: isize, step: &dyn Fn(&'static str, &'static str)) -> Result<u32, Outcome> {
    let (machine, user) = win::installs();
    // Both, if both are there: the per-user 0.1.0 leftover goes too, under the same prompt.
    let mut params = vec![String::from("--uninstall-elevated")];
    for installed in [machine, user].into_iter().flatten() {
        let uninstaller = installed
            .uninstall_string
            .as_deref()
            .and_then(state::uninstaller_path)
            .or_else(|| {
                installed
                    .location
                    .as_ref()
                    .map(|l| format!(r"{l}\uninstall.exe"))
            });
        let Some(uninstaller) = uninstaller.filter(|u| Path::new(u).exists()) else {
            if installed.scope == Scope::Machine {
                return Err(failed(
                    0,
                    "Relay's uninstaller is missing. Install Relay again, then uninstall it.",
                ));
            }
            continue;
        };
        let Some(dir) = state::parent_dir(&uninstaller) else {
            continue;
        };
        params.push(state::quote_arg(&uninstaller));
        params.push(state::quote_arg(&dir));
    }
    if params.len() == 1 {
        return Err(failed(0, "Relay isn't installed."));
    }
    let me = std::env::current_exe().map_err(|e| failed(0, e.to_string()))?;
    step("permission", "active");
    let started = || {
        step("permission", "done");
        step("work", "active");
    };
    match win::run_elevated(&me, &params.join(" "), owner, true, started) {
        Ok(code) => {
            step("work", "done");
            Ok(code)
        }
        Err(code) => Err(failed(code, state::explain_exit(code))),
    }
}

#[tauri::command]
fn open_relay(app: tauri::AppHandle) -> Result<(), String> {
    let (machine, user) = win::installs();
    let location = machine
        .or(user)
        .and_then(|i| i.location)
        .unwrap_or_else(win::default_location);
    let exe = Path::new(&location).join("relay.exe");
    std::process::Command::new(&exe)
        .current_dir(&location)
        .spawn()
        .map_err(|e| format!("Couldn't start {}: {e}", exe.display()))?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn quit(app: tauri::AppHandle) {
    app.exit(0);
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![detect, apply, open_relay, quit])
        .setup(|app| {
            // The page shows the window once it has painted in the right theme; this
            // is the fallback if it never does.
            if let Some(window) = app.get_webview_window("main") {
                if let Ok(hwnd) = window.hwnd() {
                    win::hide_border(hwnd.0 as isize);
                }
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let _ = window.show();
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Relay Setup");
}
