//! Commands the chat window can call.

use relay_agent::local::{OllamaClient, RECOMMENDED_MODEL};
use relay_agent::RouteMode;
use relay_protocol::{ConfirmAnswer, UiEvent};
use serde_json::json;
use tauri::{AppHandle, State};

use crate::app::{emit_event, AppState};
use crate::settings::{mode_from_str, mode_to_str};

fn save(
    state: &AppState,
    change: impl FnOnce(&mut crate::settings::Settings),
) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    change(&mut settings);
    settings
        .save(&state.paths.settings_file)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_message(text: String, state: State<'_, AppState>) -> Result<u64, String> {
    state.agent.submit(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop_task(state: State<'_, AppState>) -> Result<(), String> {
    state.agent.stop();
    Ok(())
}

#[tauri::command]
pub async fn answer_confirm(
    id: u64,
    approved: bool,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.agent.answer(ConfirmAnswer { id, approved }))
}

#[tauri::command]
pub async fn new_conversation(state: State<'_, AppState>) -> Result<(), String> {
    state.agent.new_conversation();
    Ok(())
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<String, String> {
    let status = state.agent.status().await;
    serde_json::to_string(&status).map_err(|e| e.to_string())
}

/// Everything the Settings page shows.
#[tauri::command]
pub async fn get_engines(state: State<'_, AppState>) -> Result<String, String> {
    let local = state.agent.local().overview().await;
    let cloud = state.agent.cloud().status().await;
    let settings = state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())?;
    let servers = relay_agent::agent::load_user_servers(&state.paths.mcp_config)
        .await
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .map_err(|e| e.to_string());
    let (mcp_servers, mcp_error) = match servers {
        Ok(names) => (names, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    Ok(json!({
        "route_mode": state.agent.route_mode().as_str(),
        "approval_mode": mode_to_str(settings.mode()),
        "local": local,
        "cloud": {
            "name": state.agent.cloud().name(),
            "installed": cloud.installed,
            "logged_in": cloud.logged_in,
            "detail": cloud.detail,
            "path": state.cloud_path,
        },
        "mcp_config": state.paths.mcp_config.display().to_string(),
        "mcp_servers": mcp_servers,
        "mcp_error": mcp_error,
        "data_dir": state.paths.data_dir.display().to_string(),
        "audit_dir": state.paths.runs_dir.display().to_string(),
        "log_dir": state.paths.logs_dir.display().to_string(),
        "hotkey": crate::app::KILL_HOTKEY_LABEL,
        "version": env!("CARGO_PKG_VERSION"),
        "recommended_model": RECOMMENDED_MODEL,
    })
    .to_string())
}

#[tauri::command]
pub async fn set_approval_mode(mode: String, state: State<'_, AppState>) -> Result<(), String> {
    let parsed = mode_from_str(&mode).ok_or_else(|| format!("unknown approval mode {mode:?}"))?;
    state.host.set_approval_mode(parsed);
    save(&state, |s| {
        s.approval_mode = mode_to_str(parsed).to_string()
    })
}

#[tauri::command]
pub async fn set_route_mode(mode: String, state: State<'_, AppState>) -> Result<(), String> {
    let parsed = RouteMode::parse(&mode).ok_or_else(|| format!("unknown route mode {mode:?}"))?;
    state.agent.set_route_mode(parsed);
    save(&state, |s| s.route_mode = parsed.as_str().to_string())
}

/// An empty name means "pick the best installed model automatically".
#[tauri::command]
pub async fn set_local_model(model: String, state: State<'_, AppState>) -> Result<(), String> {
    let chosen = (!model.trim().is_empty()).then(|| model.trim().to_string());
    state.agent.local().set_model(chosen.clone());
    save(&state, |s| s.local_model = chosen)
}

#[tauri::command]
pub async fn set_local_url(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let url = url.trim().trim_end_matches('/').to_string();
    if !(url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost")) {
        return Err(
            "the local AI server must be on this PC (http://127.0.0.1 or http://localhost)".into(),
        );
    }
    state.agent.local().set_base_url(url.clone());
    save(&state, |s| s.local_url = url)
}

/// Download a model through the local AI server, reporting progress to the window.
#[tauri::command]
pub async fn pull_model(
    model: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let model = model.trim().to_string();
    if model.is_empty() {
        return Err("enter a model name".into());
    }
    let client: OllamaClient = state.agent.local().client();
    tauri::async_runtime::spawn(async move {
        let progress_app = app.clone();
        let label = model.clone();
        let result = client
            .pull(&model, |p| {
                emit_event(
                    &progress_app,
                    &UiEvent::Progress {
                        id: "pull".into(),
                        label: format!("{label}: {}", p.status),
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
                id: "pull".into(),
                label: model.clone(),
                percent: Some(100),
                done: true,
                error: result.err().map(|e| e.to_string()),
            },
        );
    });
    Ok(())
}

#[tauri::command]
pub async fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    open_path(&state.paths.data_dir.display().to_string())
}

/// Open a fixed set of folders and links in Explorer or the default browser.
#[tauri::command]
pub async fn open_link(kind: String, state: State<'_, AppState>) -> Result<(), String> {
    let target = match kind.as_str() {
        "data" => state.paths.data_dir.display().to_string(),
        "audit" => state.paths.runs_dir.display().to_string(),
        "logs" => state.paths.logs_dir.display().to_string(),
        "mcp" => state
            .paths
            .mcp_config
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        "ollama" => "https://ollama.com/download".to_string(),
        other => return Err(format!("unknown link {other:?}")),
    };
    open_path(&target)
}

fn open_path(target: &str) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
