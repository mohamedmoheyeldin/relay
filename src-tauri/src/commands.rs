//! Commands the chat window can call.

use relay_protocol::ConfirmAnswer;
use serde_json::json;
use tauri::State;

use crate::app::AppState;
use crate::settings::{mode_from_str, mode_to_str};

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

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<String, String> {
    let mode = state
        .settings
        .lock()
        .map(|s| s.mode())
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "approval_mode": mode_to_str(mode),
        "mcp_config": state.paths.mcp_config.display().to_string(),
        "data_dir": state.paths.data_dir.display().to_string(),
    })
    .to_string())
}

#[tauri::command]
pub async fn set_approval_mode(mode: String, state: State<'_, AppState>) -> Result<(), String> {
    let parsed = mode_from_str(&mode).ok_or_else(|| format!("unknown approval mode {mode:?}"))?;
    state.host.set_approval_mode(parsed);
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.approval_mode = mode_to_str(parsed).to_string();
    settings
        .save(&state.paths.settings_file)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_data_folder(state: State<'_, AppState>) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(&state.paths.data_dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
