//! Commands the UI calls. Errors come back as plain sentences for the UI to show.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use relay_core::settings::{ActiveModel, Settings};
use relay_core::{
    browser, catalog, chat, claude, doctor, gpu, local_agent, local_ai, mcp, paths, system, updates,
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
        custom_add,
        mcp_list,
        mcp_get,
        mcp_catalog,
        mcp_add,
        mcp_remove,
        mcp_pause,
        mcp_resume,
        mcp_login,
        mcp_logout,
        plugins_list,
        plugin_set_enabled,
        browser_state,
        browser_open,
        browser_set_token,
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

/// Tells Claude when relay-browser can't open pages, so it says so instead of waiting.
fn browser_note(s: &Settings) -> Option<String> {
    let lane = browser::state(s);
    let problem = lane.problem.filter(|_| lane.enabled)?;
    Some(format!(
        "The relay-browser tools can't open pages right now: {problem} If the task needs \
         the browser, tell the owner this instead of trying them."
    ))
}

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
    if before.browser != saved.browser || before.privacy != saved.privacy {
        state::ensure_relay_browser(&app)
            .await
            .map_err(anyhow_err)?;
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
    // One check at a time: the wizard and Settings can both ask, and two runs
    // would race on re-registering relay-screen in Claude Code's config.
    static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = ONE_AT_A_TIME.lock().await;
    // Registration needs Claude Code, so retry it here after a fresh install.
    let _ = state::ensure_relay_screen(&app).await;
    let _ = state::ensure_relay_browser(&app).await;
    claude::verify().await.map_err(anyhow_err)
}

#[tauri::command]
fn pty_start(app: AppHandle, kind: String, cols: u16, rows: u16) -> Res<u64> {
    let args: &[&str] = match kind.as_str() {
        "login" => &["auth", "login", "--claudeai"],
        "login-console" => &["auth", "login", "--console"],
        _ => return Err("unknown terminal".into()),
    };
    let st = app.state::<AppState>();
    let mut slot = st.pty.lock().expect("pty lock");
    if let Some(mut old) = slot.take() {
        old.kill();
    }
    let session = crate::pty::start(&app, args, cols, rows).map_err(anyhow_err)?;
    let id = session.id;
    *slot = Some(session);
    Ok(id)
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

/// Close the terminal `id` (a newer one started since is left alone).
#[tauri::command]
fn pty_kill(state: State<'_, AppState>, id: u64) {
    let mut slot = state.pty.lock().expect("pty lock");
    if slot.as_ref().is_some_and(|s| s.id == id) {
        if let Some(mut s) = slot.take() {
            s.kill();
        }
    }
}

// ---------------------------------------------------------------- Local AI (D-046)

#[derive(Serialize)]
struct LocalAiState {
    gpu: Option<gpu::Gpu>,
    recommendation: catalog::Recommendation,
    models: Vec<local_ai::ModelView>,
    custom: Vec<local_ai::CustomView>,
    grounding: Option<ActiveModel>,
    describe: Option<ActiveModel>,
    runtime: Option<local_ai::RuntimeInfo>,
    /// Why the model store could not be read (no bundled relay-screen, say).
    error: Option<String>,
    models_dir: PathBuf,
    models_dir_bytes: u64,
    idle_unload_secs: u32,
}

/// The bundled Python that runs relay-screen's model runtime.
fn runtime_python(app: &AppHandle) -> Res<PathBuf> {
    state::python_path(app)
        .filter(|p| p.is_file())
        .ok_or_else(|| "Local AI needs the relay-screen that comes with the installer.".into())
}

#[tauri::command]
async fn local_ai_state(app: AppHandle, state: State<'_, AppState>) -> Res<LocalAiState> {
    let s = state.settings();
    let g = gpu::detect().await;
    let listing = match runtime_python(&app) {
        Ok(python) => local_ai::list(&python).await.map_err(anyhow_err),
        Err(e) => Err(e),
    };
    let l = listing.as_ref().ok();
    let (models, custom) = local_ai::views(&s.local_ai, g.as_ref(), l);
    let dir = l
        .map(|l| l.models_folder.clone())
        .unwrap_or_else(|| s.local_ai.models_dir());
    Ok(LocalAiState {
        recommendation: catalog::recommend(g.as_ref()),
        models,
        custom,
        grounding: s.local_ai.grounding.clone(),
        describe: s.local_ai.describe.clone(),
        runtime: l.and_then(|l| l.runtime.clone()),
        error: listing.err(),
        models_dir_bytes: paths::dir_size(&dir),
        models_dir: dir,
        idle_unload_secs: s.local_ai.idle_unload_secs,
        gpu: g,
    })
}

#[tauri::command]
fn runtime_install(app: AppHandle) -> Res<()> {
    let python = runtime_python(&app)?;
    jobs::spawn(&app, "runtime", move |r, cancel| async move {
        local_ai::install_runtime(&python, &cancel, &|stage, d, t| r.progress(&stage, d, t)).await
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

/// Download `model` (a catalog id or a Hugging Face repo) and give it every role in
/// `role` that has no model yet.
fn download_job(
    app: AppHandle,
    job: String,
    model: String,
    id: String,
    quant: String,
    role: String,
) -> Res<()> {
    let python = runtime_python(&app)?;
    let app2 = app.clone();
    jobs::spawn(&app, &job, move |r, cancel| async move {
        local_ai::download(&python, &model, &quant, &cancel, &|stage, d, t| {
            r.progress(&stage, d, t)
        })
        .await?;
        app2.state::<AppState>().update(|s| {
            for slot in roles_for(&role) {
                let empty = match slot {
                    "grounding" => s.local_ai.grounding.is_none(),
                    _ => s.local_ai.describe.is_none(),
                };
                if empty {
                    let active = ActiveModel {
                        model_id: id.clone(),
                        quant: quant.clone(),
                    };
                    set_active(s, slot, Some(active));
                }
            }
        })?;
        Ok(())
    })
}

#[tauri::command]
fn model_install(app: AppHandle, model_id: String, quant: String) -> Res<()> {
    let model = catalog::find(&model_id).ok_or("unknown model")?;
    download_job(
        app,
        format!("model:{model_id}"),
        model_id.clone(),
        model_id,
        quant,
        model.role,
    )
}

/// Download any GGUF repo from Hugging Face as a custom model (D-046).
#[tauri::command]
fn custom_add(app: AppHandle, repo: String, quant: String) -> Res<()> {
    let repo = local_ai::check_repo(&repo).map_err(anyhow_err)?;
    let quant = quant.trim().to_string();
    if quant.is_empty() {
        return Err("Enter a quantization such as Q4_K_M.".into());
    }
    let id = local_ai::custom_id(&repo);
    download_job(app, "custom".into(), repo, id, quant, "both".into())
}

/// Use a downloaded model for a role ("grounding", "describe" or "both").
#[tauri::command]
async fn model_activate(
    app: AppHandle,
    state: State<'_, AppState>,
    role: String,
    model_id: String,
) -> Res<Settings> {
    let python = runtime_python(&app)?;
    let listing = local_ai::list(&python).await.map_err(anyhow_err)?;
    let m = listing
        .installed(&model_id)
        .ok_or("Download this model first.")?;
    let active = ActiveModel {
        model_id: m.id.clone(),
        quant: m.quant.clone(),
    };
    state
        .update(|s| set_active(s, &role, Some(active)))
        .map_err(anyhow_err)
}

#[tauri::command]
async fn model_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Res<Settings> {
    let python = runtime_python(&app)?;
    local_ai::remove(&python, &model_id)
        .await
        .map_err(anyhow_err)?;
    state
        .update(|s| {
            for slot in [&mut s.local_ai.grounding, &mut s.local_ai.describe] {
                if slot.as_ref().is_some_and(|a| a.model_id == model_id) {
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
    } else if name == mcp::RELAY_BROWSER {
        state::ensure_relay_browser(&app)
            .await
            .map_err(anyhow_err)?;
    } else {
        mcp::add_json(&name, &paused.config)
            .await
            .map_err(anyhow_err)?;
    }
    Ok(())
}

/// Sign in to a server that uses OAuth (HTTP servers, claude.ai connectors).
/// Claude Code opens the browser and keeps the token; the job ends when it's done.
#[tauri::command]
fn mcp_login(app: AppHandle, name: String) -> Res<()> {
    jobs::spawn(
        &app,
        &format!("mcp-login:{name}"),
        move |r, cancel| async move {
            let login = mcp::login(&name, move |l| r.line(l));
            relay_core::download::until_cancelled(login, &cancel).await
        },
    )
}

#[tauri::command]
async fn mcp_logout(name: String) -> Res<()> {
    mcp::logout(&name).await.map_err(anyhow_err)
}

#[tauri::command]
async fn plugins_list() -> Res<Vec<mcp::Plugin>> {
    mcp::plugins().await.map_err(anyhow_err)
}

/// Turn a Claude Code plugin (and every server it brings) on or off.
#[tauri::command]
async fn plugin_set_enabled(id: String, enabled: bool) -> Res<()> {
    mcp::set_plugin_enabled(&id, enabled)
        .await
        .map_err(anyhow_err)
}

// ---------------------------------------------------------------- chat

#[derive(Clone, Serialize)]
struct ChatPayload {
    event: Option<chat::ChatEvent>,
    finished: bool,
}

fn emit_chat(app: &AppHandle, event: Option<chat::ChatEvent>) {
    let finished = event.is_none();
    let _ = app.emit("chat", ChatPayload { event, finished });
}

/// What a Claude turn showed about whether Claude could run at all.
#[derive(Default)]
struct ClaudeOutcome {
    tool_ran: bool,
    auth_text: bool,
    unavailable: bool,
}

#[tauri::command]
async fn chat_send(app: AppHandle, prompt: String) -> Res<()> {
    let st = app.state::<AppState>();
    let mut turn = st.turn.lock().await;
    if turn.is_some() {
        return Err("Relay is still working on the last message.".into());
    }
    st.stopped.store(false, Ordering::SeqCst);
    let s = st.settings();
    let opts = chat::TurnOptions {
        resume: st.session.lock().expect("session lock").clone(),
        model: s.claude.model.clone(),
        effort: s.claude.effort.clone(),
        note: browser_note(&s),
    };
    let outcome = Arc::new(std::sync::Mutex::new(ClaudeOutcome::default()));
    let seen = outcome.clone();
    let events_app = app.clone();
    let started = chat::start(&prompt, &opts, move |event| {
        {
            let mut o = seen.lock().expect("outcome lock");
            match &event {
                chat::ChatEvent::ToolUse { .. } => o.tool_ran = true,
                chat::ChatEvent::Text { text } => {
                    o.auth_text |= local_agent::claude_unavailable(text)
                }
                chat::ChatEvent::Done {
                    is_error: true,
                    result,
                    ..
                } => o.unavailable |= o.auth_text || local_agent::claude_unavailable(result),
                chat::ChatEvent::Error { message } => {
                    o.unavailable |= local_agent::claude_unavailable(message)
                }
                _ => {}
            }
        }
        if let chat::ChatEvent::Started { session_id, .. } = &event {
            if !session_id.is_empty() {
                *events_app
                    .state::<AppState>()
                    .session
                    .lock()
                    .expect("session lock") = Some(session_id.clone());
            }
        }
        emit_chat(&events_app, Some(event));
    })
    .await;
    let claude_task = match started {
        Ok((handle, task)) => {
            *turn = Some(handle);
            Some(task)
        }
        // Claude Code isn't installed: the local AI takes the task straight away.
        Err(_) => None,
    };
    drop(turn);
    let done_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let reason = match claude_task {
            None => Some("Claude Code isn't installed".to_string()),
            Some(task) => {
                let _ = task.await;
                let o = outcome.lock().expect("outcome lock");
                (o.unavailable && !o.tool_ran)
                    .then(|| "Claude can't run right now (signed out or offline)".to_string())
            }
        };
        let st = done_app.state::<AppState>();
        if let Some(reason) = reason.filter(|_| !st.stopped.load(Ordering::SeqCst)) {
            run_local(&done_app, &prompt, reason).await;
        }
        *st.turn.lock().await = None;
        emit_chat(&done_app, None);
    });
    Ok(())
}

/// Hand the task to the local operator (D-067) and wait for it to finish.
async fn run_local(app: &AppHandle, prompt: &str, reason: String) {
    emit_chat(app, Some(chat::ChatEvent::Fallback { reason }));
    let Some(python) = state::python_path(app) else {
        emit_chat(
            app,
            Some(chat::ChatEvent::Error {
                message:
                    "Relay's Python runtime is missing, so the local AI can't run. Reinstall Relay."
                        .into(),
            }),
        );
        return;
    };
    let st = app.state::<AppState>();
    let s = st.settings();
    let events_app = app.clone();
    let mut turn = st.turn.lock().await;
    if st.stopped.load(Ordering::SeqCst) {
        return;
    }
    match local_agent::start(&python, &s, prompt, move |event| {
        emit_chat(&events_app, Some(event))
    })
    .await
    {
        Ok((handle, task)) => {
            *turn = Some(handle);
            drop(turn);
            let _ = task.await;
        }
        Err(e) => emit_chat(
            app,
            Some(chat::ChatEvent::Error {
                message: format!("{e:#}"),
            }),
        ),
    }
}

#[tauri::command]
async fn chat_stop(state: State<'_, AppState>) -> Res<()> {
    state.stopped.store(true, Ordering::SeqCst);
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
struct BrowserState {
    #[serde(flatten)]
    lane: browser::LaneState,
    /// relay-browser is in Claude Code's config right now.
    registered: bool,
}

#[tauri::command]
async fn browser_state(app: AppHandle) -> BrowserState {
    let registered = state::ensure_relay_browser(&app).await.unwrap_or(false);
    BrowserState {
        lane: browser::state(&app.state::<AppState>().settings()),
        registered,
    }
}

/// Open the lane's browser so the owner can sign in to sites (Relay's profile) or
/// add the Playwright Extension (their own browser). Only runs when they press the button.
#[tauri::command]
fn browser_open(app: AppHandle, url: Option<String>) -> Res<()> {
    let lane = browser::state(&app.state::<AppState>().settings());
    let b = lane.browser.ok_or("No Chromium-based browser was found.")?;
    let mut cmd = std::process::Command::new(&b.path);
    if lane.use_own {
        // Opens as a tab in the browser the owner already has open.
        cmd.arg(url.unwrap_or_else(|| browser::EXTENSION_URL.into()));
    } else {
        std::fs::create_dir_all(&lane.profile_dir).map_err(err)?;
        cmd.arg(format!("--user-data-dir={}", lane.profile_dir.display()))
            .arg("--no-first-run");
    }
    cmd.spawn().map_err(err)?;
    Ok(())
}

/// Keep the Playwright Extension's token in Credential Manager and hand it to
/// relay-browser through the launcher. An empty token removes it.
#[tauri::command]
async fn browser_set_token(app: AppHandle, token: String) -> Res<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        let _ = relay_core::secrets::delete(mcp::RELAY_BROWSER, browser::TOKEN_SECRET);
    } else {
        relay_core::secrets::set(mcp::RELAY_BROWSER, browser::TOKEN_SECRET, &token)
            .map_err(anyhow_err)?;
    }
    app.state::<AppState>()
        .update(|s| {
            let names = s
                .connections
                .secret_names
                .entry(mcp::RELAY_BROWSER.into())
                .or_default();
            names.retain(|n| n != browser::TOKEN_SECRET);
            if !token.is_empty() {
                names.push(browser::TOKEN_SECRET.into());
            }
            if names.is_empty() {
                s.connections.secret_names.remove(mcp::RELAY_BROWSER);
            }
        })
        .map_err(anyhow_err)?;
    state::ensure_relay_browser(&app)
        .await
        .map_err(anyhow_err)?;
    Ok(())
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
