//! Commands the chat window can call. See `docs/ui-contract.md`.

use relay_agent::RouteMode;
use relay_protocol::{ConfirmAnswer, StatusInfo, UiEvent};
use relay_store::{ConvPatch, ConvSummary, Conversation, Project, ProjectPatch, SearchResults};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::app::{emit_event, AppState, KILL_HOTKEY_LABEL};
use crate::settings::{mode_to_str, SettingsPatch};
use crate::setup::{LocalSetup, PROGRESS_ID};

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---- running tasks ----

#[tauri::command]
pub async fn send_message(
    conv_id: String,
    text: String,
    attachments: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> CmdResult<Value> {
    let attachments = attachments.unwrap_or_default();
    let run_id = state
        .agent
        .submit_in(&conv_id, &text, &attachments)
        .map_err(err)?;
    Ok(json!({ "run_id": run_id }))
}

#[tauri::command]
pub async fn stop_task(state: State<'_, AppState>) -> CmdResult<()> {
    state.agent.stop();
    Ok(())
}

#[tauri::command]
pub async fn answer_confirm(
    id: u64,
    approved: bool,
    state: State<'_, AppState>,
) -> CmdResult<bool> {
    Ok(state.agent.answer(ConfirmAnswer { id, approved }))
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> CmdResult<StatusInfo> {
    Ok(state.agent.status().await)
}

// ---- conversations ----

#[tauri::command]
pub async fn conv_list(
    archived: Option<bool>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<ConvSummary>> {
    state
        .store
        .conv_list(archived.unwrap_or(false))
        .map_err(err)
}

#[tauri::command]
pub async fn conv_get(id: String, state: State<'_, AppState>) -> CmdResult<Conversation> {
    state.store.conv_get(&id).map_err(err)
}

#[tauri::command]
pub async fn conv_create(
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<ConvSummary> {
    state.store.conv_create(project_id.as_deref()).map_err(err)
}

#[tauri::command]
pub async fn conv_save(
    id: String,
    title: Option<String>,
    items: Value,
    search_text: String,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    state
        .store
        .conv_save(&id, title.as_deref(), &items, &search_text)
        .map_err(err)
}

#[tauri::command]
pub async fn conv_update(
    id: String,
    title: Option<String>,
    pinned: Option<bool>,
    archived: Option<bool>,
    state: State<'_, AppState>,
) -> CmdResult<ConvSummary> {
    let patch = ConvPatch {
        title,
        pinned,
        archived,
        project_id: None,
    };
    state.store.conv_update(&id, &patch).map_err(err)
}

/// Move a chat into a project, or out of one when `project_id` is null.
#[tauri::command]
pub async fn conv_move(
    id: String,
    project_id: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<ConvSummary> {
    let patch = ConvPatch {
        project_id: Some(project_id),
        ..ConvPatch::default()
    };
    state.store.conv_update(&id, &patch).map_err(err)
}

#[tauri::command]
pub async fn conv_delete(id: String, state: State<'_, AppState>) -> CmdResult<()> {
    state.store.conv_delete(&id).map_err(err)
}

#[tauri::command]
pub async fn search(query: String, state: State<'_, AppState>) -> CmdResult<SearchResults> {
    state.store.search(&query).map_err(err)
}

// ---- projects ----

#[tauri::command]
pub async fn project_list(state: State<'_, AppState>) -> CmdResult<Vec<Project>> {
    state.store.project_list().map_err(err)
}

#[tauri::command]
pub async fn project_create(
    name: String,
    color: String,
    instructions: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<Project> {
    state
        .store
        .project_create(&name, &color, instructions.as_deref().unwrap_or_default())
        .map_err(err)
}

#[tauri::command]
pub async fn project_update(
    id: String,
    name: Option<String>,
    color: Option<String>,
    instructions: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<Project> {
    state
        .store
        .project_update(
            &id,
            &ProjectPatch {
                name,
                color,
                instructions,
            },
        )
        .map_err(err)
}

#[tauri::command]
pub async fn project_delete(id: String, state: State<'_, AppState>) -> CmdResult<()> {
    state.store.project_delete(&id).map_err(err)
}

// ---- engines ----

/// Everything the AI engines page shows.
#[tauri::command]
pub async fn get_engines(state: State<'_, AppState>) -> CmdResult<Value> {
    let mut local = serde_json::to_value(state.agent.local().overview().await).map_err(err)?;
    let gpu = state.setup.gpu_or_detect().await;
    let base_url = local["base_url"].as_str().unwrap_or_default().to_string();
    local["managed"] = json!(base_url == state.setup.engine().base_url());
    local["recommended"] = json!(relay_agent::local::engine::recommended_model(gpu.as_ref()));
    local["gpu"] = serde_json::to_value(&gpu).map_err(err)?;
    let cloud = state.agent.cloud_status().await;
    let setup = state.setup.engine().state();
    Ok(json!({
        "route_mode": state.agent.route_mode().as_str(),
        "local": local,
        "cloud": {
            "name": state.agent.cloud().name(),
            "installed": cloud.installed,
            "logged_in": cloud.logged_in,
            "detail": cloud.detail,
            "path": state.cloud_path,
        },
        "setup": setup,
    }))
}

#[tauri::command]
pub async fn set_route_mode(mode: String, state: State<'_, AppState>) -> CmdResult<()> {
    let parsed = RouteMode::parse(&mode).ok_or_else(|| format!("unknown route mode {mode:?}"))?;
    state.agent.set_route_mode(parsed);
    state.update_settings(|s| s.route_mode = parsed.as_str().to_string())
}

/// An empty name means "pick the best installed model automatically".
#[tauri::command]
pub async fn set_local_model(model: String, state: State<'_, AppState>) -> CmdResult<()> {
    let chosen = (!model.trim().is_empty()).then(|| model.trim().to_string());
    state.agent.local().set_model(chosen.clone());
    state.update_settings(|s| s.local_model = chosen)
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupAction {
    Start,
    Cancel,
}

#[tauri::command]
pub async fn local_ai_setup(action: SetupAction, state: State<'_, AppState>) -> CmdResult<()> {
    match action {
        SetupAction::Start => state.setup.start(),
        SetupAction::Cancel => state.setup.cancel(),
    }
    Ok(())
}

/// Download any model through the built-in engine, reporting progress as `local-ai`.
#[tauri::command]
pub async fn pull_model(
    model: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("enter a model name".into());
    }
    let setup: std::sync::Arc<LocalSetup> = state.setup.clone();
    tauri::async_runtime::spawn(async move {
        // Make sure the engine is there and running first.
        if !setup.engine().is_installed() || !setup.engine().is_running().await {
            setup.start();
            return;
        }
        let client = setup.local_client();
        let progress_app = app.clone();
        let label = model.clone();
        let result = client
            .pull(&model, |p| {
                emit_event(
                    &progress_app,
                    &UiEvent::Progress {
                        id: PROGRESS_ID.into(),
                        label: format!("Downloading {label}"),
                        percent: p.percent(),
                        done: false,
                        error: None,
                    },
                );
            })
            .await;
        emit_event(
            &app,
            &UiEvent::Progress {
                id: PROGRESS_ID.into(),
                label: model.clone(),
                percent: Some(100),
                done: true,
                error: result.err().map(|e| e.to_string()),
            },
        );
    });
    Ok(())
}

// ---- settings ----

fn settings_json(state: &AppState) -> CmdResult<Value> {
    let s = state.settings.lock().map(|s| s.clone()).map_err(err)?;
    Ok(json!({
        "approval_mode": mode_to_str(s.mode()),
        "user_name": s.user_name,
        "custom_instructions": s.custom_instructions,
        "auto_setup": s.auto_setup,
        "hotkey": KILL_HOTKEY_LABEL,
        "version": env!("CARGO_PKG_VERSION"),
        "paths": {
            "data": state.paths.data_dir.display().to_string(),
            "audit": state.paths.runs_dir.display().to_string(),
            "logs": state.paths.logs_dir.display().to_string(),
            "mcp": state.paths.mcp_config.display().to_string(),
        },
    }))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Value> {
    settings_json(&state)
}

#[tauri::command]
pub async fn set_settings(
    approval_mode: Option<String>,
    user_name: Option<String>,
    custom_instructions: Option<String>,
    auto_setup: Option<bool>,
    state: State<'_, AppState>,
) -> CmdResult<Value> {
    let patch = SettingsPatch {
        approval_mode,
        user_name,
        custom_instructions,
        auto_setup,
    };
    let mode = {
        let mut settings = state.settings.lock().map_err(err)?;
        settings.apply(patch).map_err(err)?;
        settings.save(&state.paths.settings_file).map_err(err)?;
        settings.mode()
    };
    state.host.set_approval_mode(mode);
    settings_json(&state)
}

// ---- connections (MCP) ----

#[tauri::command]
pub async fn mcp_get(state: State<'_, AppState>) -> CmdResult<Value> {
    let path = state.paths.mcp_config.clone();
    let (servers, error) = match tokio::fs::read_to_string(&path).await {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(v) => (
                v.get("mcpServers").cloned().unwrap_or_else(|| json!({})),
                None,
            ),
            Err(e) => (
                json!({}),
                Some(format!("{} is not valid JSON: {e}", path.display())),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (json!({}), None),
        Err(e) => (json!({}), Some(e.to_string())),
    };
    Ok(json!({ "path": path.display().to_string(), "servers": servers, "error": error }))
}

#[tauri::command]
pub async fn mcp_set(servers: Value, state: State<'_, AppState>) -> CmdResult<()> {
    if !servers.is_object() {
        return Err("servers must be an object of name -> definition".into());
    }
    if servers.get("relay").is_some() {
        return Err("the name \"relay\" is reserved for Relay's own tools".into());
    }
    let text = serde_json::to_string_pretty(&json!({ "mcpServers": servers })).map_err(err)?;
    tokio::fs::write(&state.paths.mcp_config, text)
        .await
        .map_err(err)
}

// ---- files and links ----

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> CmdResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .map(|p| p.display().to_string())
            .collect()
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub async fn open_link(kind: String, state: State<'_, AppState>) -> CmdResult<()> {
    let target = match kind.as_str() {
        "data" => state.paths.data_dir.clone(),
        "audit" => state.paths.runs_dir.clone(),
        "logs" => state.paths.logs_dir.clone(),
        "mcp" => state
            .paths
            .mcp_config
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default(),
        other => return Err(format!("unknown link {other:?}")),
    };
    std::process::Command::new("explorer")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(err)
}

// ---- installer (the downloaded setup program is the app itself) ----

use crate::installer::{self, InstallInfo, InstallOptions, Mode};

/// Which screen to show: the app, the installer, or the uninstaller.
#[tauri::command]
pub async fn installer_info(app: AppHandle) -> CmdResult<InstallInfo> {
    let mode = app
        .try_state::<crate::app::ModeState>()
        .map(|m| m.0)
        .unwrap_or(Mode::App);
    let data_dir = crate::app::data_dir_of(&app).map_err(err)?;
    Ok(InstallInfo {
        mode,
        version: env!("CARGO_PKG_VERSION").to_string(),
        install_dir: installer::install_dir().display().to_string(),
        installed_version: installer::installed_version(),
        data_bytes: installer::dir_size(&data_dir),
        data_dir: data_dir.display().to_string(),
    })
}

fn progress(app: &AppHandle, label: &str, percent: u8, done: bool, error: Option<String>) {
    emit_event(
        app,
        &UiEvent::Progress {
            id: "installer".into(),
            label: label.to_string(),
            percent: Some(percent),
            done,
            error,
        },
    );
}

#[tauri::command]
pub async fn installer_install(
    desktop_shortcut: bool,
    start_with_windows: bool,
    app: AppHandle,
) -> CmdResult<()> {
    let opts = InstallOptions {
        desktop_shortcut,
        start_with_windows,
    };
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        installer::install(opts, env!("CARGO_PKG_VERSION"), |label, pct| {
            progress(&progress_app, label, pct, false, None)
        })
    })
    .await
    .map_err(err)?;
    match result {
        Ok(_) => {
            progress(&app, "Relay is installed", 100, true, None);
            Ok(())
        }
        Err(e) => {
            progress(&app, "Setup could not finish", 0, true, Some(e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Start the installed copy and close the setup window.
#[tauri::command]
pub async fn installer_launch(app: AppHandle) -> CmdResult<()> {
    let exe = installer::install_dir().join(installer::INSTALLED_EXE);
    std::process::Command::new(exe)
        .arg("--run")
        .spawn()
        .map_err(err)?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn uninstall_run(delete_data: bool, app: AppHandle) -> CmdResult<()> {
    let data_dir = crate::app::data_dir_of(&app).map_err(err)?;
    let progress_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        installer::uninstall(delete_data, &data_dir, |label, pct| {
            progress(&progress_app, label, pct, false, None)
        })
    })
    .await
    .map_err(err)?;
    match result {
        Ok(()) => {
            progress(&app, "Relay was removed", 100, true, None);
            Ok(())
        }
        Err(e) => {
            progress(
                &app,
                "Removal could not finish",
                0,
                true,
                Some(e.to_string()),
            );
            Err(e.to_string())
        }
    }
}

/// Quit the program (used by the setup and removal screens).
#[tauri::command]
pub async fn app_quit(app: AppHandle) -> CmdResult<()> {
    app.exit(0);
    Ok(())
}

/// Open a web or mail link in the default program (http, https and mailto only).
#[tauri::command]
pub async fn open_url(url: String) -> CmdResult<()> {
    if !is_safe_url(&url) {
        return Err("only http, https and mailto links can be opened".into());
    }
    std::process::Command::new("explorer")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(err)
}

fn is_safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    !url.contains(['\n', '\r', '"', '|', '&', '<', '>', '^'])
        && (lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:"))
}

#[cfg(test)]
mod tests {
    use super::is_safe_url;

    #[test]
    fn only_web_and_mail_links_are_opened() {
        assert!(is_safe_url("https://example.com/a?b=c"));
        assert!(is_safe_url("HTTP://example.com"));
        assert!(is_safe_url("mailto:me@example.com"));
        for bad in [
            "file:///C:/Windows/System32/cmd.exe",
            "javascript:alert(1)",
            "calc.exe",
            "https://a.com\" & calc",
            "https://x|y",
            "ms-settings:",
        ] {
            assert!(!is_safe_url(bad), "{bad}");
        }
    }
}
