//! Commands the UI calls. Errors come back as plain sentences for the UI to show.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use relay_core::settings::{ActiveModel, Settings};
use relay_core::{
    catalog, chat, claude, doctor, gpu, mcp, models, paths, runtime, system, updates,
};

use crate::jobs;
use crate::state::{self, AppState};

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn anyhow_err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        get_settings,
        save_settings,
        close_choice,
        quit_app,
        open_path,
        job_cancel,
        jobs_running,
        claude_state,
        claude_install,
        claude_update,
        claude_verify,
        pty_start,
        pty_write,
        pty_resize,
        pty_kill,
        local_ai_state,
        runtime_install,
        model_install,
        model_activate,
        model_delete,
        custom_add_disk,
        custom_add_hf,
        custom_remove,
        mcp_list,
        mcp_get,
        mcp_catalog,
        mcp_add,
        mcp_remove,
        mcp_pause,
        mcp_resume,
        chat_send,
        chat_stop,
        chat_new,
        browsers,
        playbooks_list,
        playbook_delete,
        doctor_run,
        update_check,
        update_install,
        setup_complete,
    ]
}

// ---------------------------------------------------------------- app and settings

#[derive(Serialize)]
struct AppInfo {
    version: String,
    os: String,
    settings_file: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    logs_dir: PathBuf,
    playbooks_dir: PathBuf,
    default_models_dir: PathBuf,
    default_runs_dir: PathBuf,
    python: Option<PathBuf>,
    python_present: bool,
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    let python = state::python_path(&app);
    AppInfo {
        version: app.package_info().version.to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        settings_file: paths::settings_file(),
        config_dir: paths::config_dir(),
        data_dir: paths::data_dir(),
        logs_dir: paths::logs_dir(),
        playbooks_dir: paths::playbooks_dir(),
        default_models_dir: paths::default_models_dir(),
        default_runs_dir: paths::default_runs_dir(),
        python_present: python.as_ref().is_some_and(|p| p.is_file()),
        python,
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
async fn save_settings(app: AppHandle, settings: Settings) -> Res<Settings> {
    let st = app.state::<AppState>();
    let before = st.settings();
    let saved = st.update(|s| *s = settings).map_err(anyhow_err)?;
    if before.general.start_at_login != saved.general.start_at_login {
        let autolaunch = app.autolaunch();
        let r = if saved.general.start_at_login {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        r.map_err(err)?;
    }
    if before.safety != saved.safety || before.privacy != saved.privacy {
        state::ensure_relay_screen(&app).await.map_err(anyhow_err)?;
    }
    Ok(saved)
}

/// Answer to the first-close question: "tray" or "quit", remembered when asked.
#[tauri::command]
fn close_choice(app: AppHandle, action: String, remember: bool) -> Res<()> {
    use relay_core::settings::CloseAction;
    let choice = if action == "quit" {
        CloseAction::Quit
    } else {
        CloseAction::Tray
    };
    if remember {
        app.state::<AppState>()
            .update(|s| s.general.close_action = choice)
            .map_err(anyhow_err)?;
    }
    match choice {
        CloseAction::Quit => app.exit(0),
        _ => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Res<()> {
    let p = PathBuf::from(&path);
    if p.extension().is_none() && !p.exists() {
        std::fs::create_dir_all(&p).map_err(err)?;
    }
    if path.starts_with("https://") {
        app.opener().open_url(path, None::<&str>).map_err(err)
    } else {
        app.opener().open_path(path, None::<&str>).map_err(err)
    }
}

#[tauri::command]
fn job_cancel(app: AppHandle, job: String) {
    jobs::cancel(&app, &job);
}

#[tauri::command]
fn jobs_running(app: AppHandle) -> Vec<String> {
    jobs::running(&app)
}

#[tauri::command]
fn setup_complete(state: State<'_, AppState>) -> Res<Settings> {
    state
        .update(|s| s.setup.completed = true)
        .map_err(anyhow_err)
}

// ---------------------------------------------------------------- Claude Code (D-045)

#[tauri::command]
async fn claude_state() -> claude::ClaudeState {
    claude::state().await
}

#[tauri::command]
fn claude_install(app: AppHandle) -> Res<()> {
    let app2 = app.clone();
    jobs::spawn(&app, "claude-install", move |r, _| async move {
        let line_r = r.clone();
        claude::install(move |l| line_r.line(l)).await?;
        state::ensure_relay_screen(&app2).await?;
        Ok(())
    })
}

#[tauri::command]
fn claude_update(app: AppHandle) -> Res<()> {
    jobs::spawn(&app, "claude-update", move |r, _| async move {
        claude::update(move |l| r.line(l)).await
    })
}

#[tauri::command]
async fn claude_verify(app: AppHandle) -> Res<String> {
    // Registration needs Claude Code, so retry it here after a fresh install.
    let _ = state::ensure_relay_screen(&app).await;
    claude::verify().await.map_err(anyhow_err)
}

#[tauri::command]
fn pty_start(app: AppHandle, kind: String, cols: u16, rows: u16) -> Res<()> {
    let args: &[&str] = match kind.as_str() {
        "login" => &["auth", "login"],
        _ => return Err("unknown terminal".into()),
    };
    let st = app.state::<AppState>();
    let mut slot = st.pty.lock().expect("pty lock");
    if let Some(mut old) = slot.take() {
        old.kill();
    }
    *slot = Some(crate::pty::start(&app, args, cols, rows).map_err(anyhow_err)?);
    Ok(())
}

#[tauri::command]
fn pty_write(state: State<'_, AppState>, data: String) -> Res<()> {
    match state.pty.lock().expect("pty lock").as_mut() {
        Some(s) => s.write(data.as_bytes()).map_err(anyhow_err),
        None => Err("the terminal is closed".into()),
    }
}

#[tauri::command]
fn pty_resize(state: State<'_, AppState>, cols: u16, rows: u16) -> Res<()> {
    if let Some(s) = state.pty.lock().expect("pty lock").as_ref() {
        s.resize(cols, rows).map_err(anyhow_err)?;
    }
    Ok(())
}

#[tauri::command]
fn pty_kill(state: State<'_, AppState>) {
    if let Some(mut s) = state.pty.lock().expect("pty lock").take() {
        s.kill();
    }
}

// ---------------------------------------------------------------- Local AI (D-046)

#[derive(Serialize)]
struct LocalAiState {
    gpu: Option<gpu::Gpu>,
    recommendation: catalog::Recommendation,
    models: Vec<models::ModelView>,
    installed: Vec<models::Installed>,
    custom: Vec<relay_core::settings::CustomModel>,
    grounding: Option<ActiveModel>,
    describe: Option<ActiveModel>,
    runtime: runtime::RuntimeState,
    runtime_flavor: runtime::Flavor,
    runtime_download_bytes: u64,
    models_dir: PathBuf,
    models_dir_bytes: u64,
    idle_unload_secs: u32,
}

#[tauri::command]
async fn local_ai_state(state: State<'_, AppState>) -> Res<LocalAiState> {
    let s = state.settings();
    let g = gpu::detect().await;
    let flavor = runtime::flavor_for(g.as_ref());
    let dir = s.local_ai.models_dir();
    Ok(LocalAiState {
        recommendation: catalog::recommend(g.as_ref()),
        models: models::views(&s.local_ai, g.as_ref()),
        installed: models::installed(&dir),
        custom: s.local_ai.custom_models.clone(),
        grounding: s.local_ai.grounding.clone(),
        describe: s.local_ai.describe.clone(),
        runtime: runtime::state(),
        runtime_flavor: flavor,
        runtime_download_bytes: runtime::download_bytes(flavor),
        models_dir_bytes: paths::dir_size(&dir),
        models_dir: dir,
        idle_unload_secs: s.local_ai.idle_unload_secs,
        gpu: g,
    })
}

#[tauri::command]
fn runtime_install(app: AppHandle) -> Res<()> {
    jobs::spawn(&app, "runtime", move |r, cancel| async move {
        let g = gpu::detect().await;
        runtime::install(runtime::flavor_for(g.as_ref()), &cancel, &|stage, d, t| {
            r.progress(&stage, d, t)
        })
        .await?;
        Ok(())
    })
}

fn roles_for(role: &str) -> Vec<&'static str> {
    match role {
        "grounding" => vec!["grounding"],
        "describe" => vec!["describe"],
        _ => vec!["grounding", "describe"],
    }
}

fn set_active(s: &mut Settings, role: &str, active: Option<ActiveModel>) {
    for r in roles_for(role) {
        match r {
            "grounding" => s.local_ai.grounding = active.clone(),
            _ => s.local_ai.describe = active.clone(),
        }
    }
}

#[tauri::command]
fn model_install(app: AppHandle, model_id: String, quant: String) -> Res<()> {
    let model = catalog::find(&model_id).ok_or("unknown model")?;
    let job = format!("model:{model_id}:{quant}");
    let app2 = app.clone();
    jobs::spawn(&app, &job, move |r, cancel| async move {
        let st = app2.state::<AppState>();
        let dir = st.settings().local_ai.models_dir();
        let installed = models::install(&model, &quant, &dir, &cancel, &|stage, d, t| {
            r.progress(&stage, d, t)
        })
        .await?;
        // A freshly downloaded model takes any role that has none yet.
        st.update(|s| {
            for role in roles_for(&model.role) {
                let empty = match role {
                    "grounding" => s.local_ai.grounding.is_none(),
                    _ => s.local_ai.describe.is_none(),
                };
                if empty {
                    set_active(s, role, Some(installed.active()));
                }
            }
        })?;
        Ok(())
    })
}

/// Use an installed catalog model (or a custom one: quant "custom") for a role.
#[tauri::command]
fn model_activate(
    state: State<'_, AppState>,
    role: String,
    model_id: String,
    quant: String,
) -> Res<Settings> {
    let s = state.settings();
    let active = if quant == "custom" {
        let c = s
            .local_ai
            .custom_models
            .iter()
            .find(|c| c.id == model_id)
            .ok_or("unknown custom model")?;
        ActiveModel {
            model_id: c.id.clone(),
            quant: "custom".into(),
            model_path: c.model_path.clone(),
            mmproj_path: c.mmproj_path.clone(),
        }
    } else {
        models::installed(&s.local_ai.models_dir())
            .into_iter()
            .find(|i| i.model_id == model_id && i.quant == quant)
            .ok_or("download this model first")?
            .active()
    };
    state
        .update(|s| set_active(s, &role, Some(active)))
        .map_err(anyhow_err)
}

#[tauri::command]
fn model_delete(state: State<'_, AppState>, model_id: String, quant: String) -> Res<Settings> {
    let dir = state.settings().local_ai.models_dir();
    models::delete(&dir, &model_id, &quant).map_err(anyhow_err)?;
    state
        .update(|s| {
            let gone = |a: &Option<ActiveModel>| {
                a.as_ref()
                    .is_some_and(|a| a.model_id == model_id && a.quant == quant)
            };
            if gone(&s.local_ai.grounding) {
                s.local_ai.grounding = None;
            }
            if gone(&s.local_ai.describe) {
                s.local_ai.describe = None;
            }
        })
        .map_err(anyhow_err)
}

#[tauri::command]
fn custom_add_disk(
    state: State<'_, AppState>,
    name: String,
    role: String,
    model_path: String,
    mmproj_path: String,
) -> Res<Settings> {
    let c = models::custom_from_disk(&name, &role, model_path.as_ref(), mmproj_path.as_ref())
        .map_err(anyhow_err)?;
    state
        .update(|s| {
            s.local_ai.custom_models.retain(|x| x.id != c.id);
            s.local_ai.custom_models.push(c);
        })
        .map_err(anyhow_err)
}

#[tauri::command]
fn custom_add_hf(
    app: AppHandle,
    name: String,
    role: String,
    model_url: String,
    mmproj_url: String,
) -> Res<()> {
    models::parse_hf_url(&model_url).map_err(anyhow_err)?;
    models::parse_hf_url(&mmproj_url).map_err(anyhow_err)?;
    let app2 = app.clone();
    jobs::spawn(&app, "custom", move |r, cancel| async move {
        let st = app2.state::<AppState>();
        let dir = st.settings().local_ai.models_dir();
        let c = models::install_custom_from_hf(
            &name,
            &role,
            &model_url,
            &mmproj_url,
            &dir,
            &cancel,
            &|stage, d, t| r.progress(&stage, d, t),
        )
        .await?;
        st.update(|s| {
            s.local_ai.custom_models.retain(|x| x.id != c.id);
            s.local_ai.custom_models.push(c);
        })?;
        Ok(())
    })
}

/// Forget a custom model; its files are deleted only if Relay downloaded them.
#[tauri::command]
fn custom_remove(state: State<'_, AppState>, id: String) -> Res<Settings> {
    let s = state.settings();
    let dir = s.local_ai.models_dir();
    if let Some(c) = s.local_ai.custom_models.iter().find(|c| c.id == id) {
        if c.model_path.starts_with(&dir) {
            let _ = std::fs::remove_dir_all(dir.join(&c.id));
        }
    }
    state
        .update(|s| {
            s.local_ai.custom_models.retain(|c| c.id != id);
            for slot in [&mut s.local_ai.grounding, &mut s.local_ai.describe] {
                if slot.as_ref().is_some_and(|a| a.model_id == id) {
                    *slot = None;
                }
            }
        })
        .map_err(anyhow_err)
}

// ---------------------------------------------------------------- MCP hub (D-047)

#[tauri::command]
async fn mcp_list(state: State<'_, AppState>) -> Res<Vec<mcp::Entry>> {
    let paused = state.settings().connections.paused;
    mcp::list(&paused).await.map_err(anyhow_err)
}

#[tauri::command]
async fn mcp_get(name: String) -> Res<String> {
    mcp::get(&name).await.map_err(anyhow_err)
}

#[tauri::command]
fn mcp_catalog() -> Vec<mcp::CatalogServer> {
    mcp::catalog()
}

#[derive(Deserialize)]
struct SecretInput {
    key: String,
    value: String,
}

#[tauri::command]
async fn mcp_add(app: AppHandle, spec: mcp::ServerSpec, secrets: Vec<SecretInput>) -> Res<()> {
    if mcp::is_builtin(spec.name()) {
        return Err("that name belongs to one of Relay's own servers".into());
    }
    let secret_env: Vec<(String, String)> = secrets
        .into_iter()
        .filter(|s| !s.key.trim().is_empty())
        .map(|s| (s.key.trim().to_string(), s.value))
        .collect();
    // Record the secret names first: the launcher reads them when Claude starts the server.
    let names: Vec<String> = secret_env.iter().map(|(k, _)| k.clone()).collect();
    let name = spec.name().to_string();
    let st = app.state::<AppState>();
    if !names.is_empty() {
        st.update(|s| {
            s.connections.secret_names.insert(name.clone(), names);
        })
        .map_err(anyhow_err)?;
    }
    mcp::add(&spec, &secret_env, &state::launcher_path())
        .await
        .map_err(anyhow_err)
}

#[tauri::command]
async fn mcp_remove(app: AppHandle, name: String) -> Res<()> {
    if mcp::is_builtin(&name) {
        return Err("Relay's own servers can be paused but not removed".into());
    }
    let st = app.state::<AppState>();
    let s = st.settings();
    let secret_names = s
        .connections
        .secret_names
        .get(&name)
        .cloned()
        .unwrap_or_default();
    if s.connections.paused.iter().any(|p| p.name == name) {
        for key in &secret_names {
            let _ = relay_core::secrets::delete(&name, key);
        }
    } else {
        mcp::remove(&name, &secret_names, false)
            .await
            .map_err(anyhow_err)?;
    }
    st.update(|s| {
        s.connections.paused.retain(|p| p.name != name);
        s.connections.secret_names.remove(&name);
    })
    .map_err(anyhow_err)?;
    Ok(())
}

#[tauri::command]
async fn mcp_pause(app: AppHandle, name: String) -> Res<()> {
    let config = mcp::user_config(&name).map_err(anyhow_err)?;
    mcp::remove(&name, &[], true).await.map_err(anyhow_err)?;
    app.state::<AppState>()
        .update(|s| {
            s.connections.paused.retain(|p| p.name != name);
            s.connections.paused.push(mcp::PausedServer {
                name: name.clone(),
                config,
            });
        })
        .map_err(anyhow_err)?;
    Ok(())
}

#[tauri::command]
async fn mcp_resume(app: AppHandle, name: String) -> Res<()> {
    let st = app.state::<AppState>();
    let paused = st
        .settings()
        .connections
        .paused
        .into_iter()
        .find(|p| p.name == name)
        .ok_or("that server is not paused")?;
    st.update(|s| s.connections.paused.retain(|p| p.name != name))
        .map_err(anyhow_err)?;
    if name == mcp::RELAY_SCREEN {
        state::ensure_relay_screen(&app).await.map_err(anyhow_err)?;
    } else {
        mcp::add_json(&name, &paused.config)
            .await
            .map_err(anyhow_err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- chat

#[derive(Clone, Serialize)]
struct ChatPayload {
    event: Option<chat::ChatEvent>,
    finished: bool,
}

#[tauri::command]
async fn chat_send(app: AppHandle, prompt: String) -> Res<()> {
    let st = app.state::<AppState>();
    let mut turn = st.turn.lock().await;
    if turn.is_some() {
        return Err("Relay is still working on the last message.".into());
    }
    let s = st.settings();
    let opts = chat::TurnOptions {
        resume: st.session.lock().expect("session lock").clone(),
        model: s.claude.model.clone(),
        effort: s.claude.effort.clone(),
    };
    let events_app = app.clone();
    let (handle, task) = chat::start(&prompt, &opts, move |event| {
        if let chat::ChatEvent::Started { session_id, .. } = &event {
            if !session_id.is_empty() {
                *events_app
                    .state::<AppState>()
                    .session
                    .lock()
                    .expect("session lock") = Some(session_id.clone());
            }
        }
        let _ = events_app.emit(
            "chat",
            ChatPayload {
                event: Some(event),
                finished: false,
            },
        );
    })
    .await
    .map_err(anyhow_err)?;
    *turn = Some(handle);
    drop(turn);
    let done_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = task.await;
        *done_app.state::<AppState>().turn.lock().await = None;
        let _ = done_app.emit(
            "chat",
            ChatPayload {
                event: None,
                finished: true,
            },
        );
    });
    Ok(())
}

#[tauri::command]
async fn chat_stop(state: State<'_, AppState>) -> Res<()> {
    if let Some(turn) = state.turn.lock().await.as_mut() {
        turn.stop().await;
    }
    Ok(())
}

#[tauri::command]
fn chat_new(state: State<'_, AppState>) {
    *state.session.lock().expect("session lock") = None;
}

// ---------------------------------------------------------------- browser, playbooks

#[tauri::command]
fn browsers() -> Vec<system::BrowserInfo> {
    system::browsers()
}

#[derive(Serialize)]
struct Playbook {
    name: String,
    path: PathBuf,
    bytes: u64,
    modified: u64,
}

#[tauri::command]
fn playbooks_list() -> Vec<Playbook> {
    let dir = paths::playbooks_dir();
    let mut list: Vec<Playbook> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_file())
                .map(|e| {
                    let meta = e.metadata().ok();
                    Playbook {
                        name: e.file_name().to_string_lossy().into_owned(),
                        path: e.path(),
                        bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                        modified: meta
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

#[tauri::command]
fn playbook_delete(name: String) -> Res<()> {
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("not a playbook name".into());
    }
    std::fs::remove_file(paths::playbooks_dir().join(name)).map_err(err)
}

// ---------------------------------------------------------------- diagnostics, updates

#[derive(Serialize)]
struct DoctorResult {
    checks: Vec<doctor::Check>,
    report: String,
}

#[tauri::command]
async fn doctor_run(app: AppHandle) -> DoctorResult {
    let s = app.state::<AppState>().settings();
    let python = state::python_path(&app);
    let version = app.package_info().version.to_string();
    let checks = doctor::run(&s, python.as_deref(), &version).await;
    DoctorResult {
        report: doctor::report(&checks),
        checks,
    }
}

#[tauri::command]
async fn update_check(app: AppHandle) -> Res<updates::UpdateInfo> {
    let version = app.package_info().version.to_string();
    updates::check(&version).await.map_err(anyhow_err)
}

#[tauri::command]
fn update_install(app: AppHandle, info: updates::UpdateInfo) -> Res<()> {
    let app2 = app.clone();
    jobs::spawn(&app, "update", move |r, cancel| async move {
        let path =
            updates::download_installer(&info, &cancel, &|d, t| r.progress("Relay update", d, t))
                .await?;
        std::process::Command::new(&path).spawn()?;
        app2.exit(0);
        Ok(())
    })
}
