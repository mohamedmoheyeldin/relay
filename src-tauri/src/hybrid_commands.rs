//! Commands for the hybrid assistant: provider status, privacy and permission settings, task history.
//! See `docs/ui-contract.md` (v2).

use relay_agent::WorkflowRecord;
use relay_mcp::ApprovalMode;
use relay_orchestrator::permissions::{ConfirmLevel, PrivacyMode};
use relay_orchestrator::router::ProviderKind;
use serde_json::{json, Value};
use tauri::State;

use crate::app::AppState;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn get_platform() -> CmdResult<Value> {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    // Window materials are reported only once they are actually applied to the window.
    Ok(json!({
        "os": os,
        "materials": "none",
        // Windows draws its own window buttons so the title bar and the content are one surface.
        "title_bar": if cfg!(windows) { "custom" } else { "native" },
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Every tool a plan may use, with its risk class, for the permissions screen.
const TOOLS: &[(&str, &str, &str)] = &[
    ("open_app", "Open an app", "safe"),
    ("open_url", "Open a link", "safe"),
    ("focus_window", "Switch to a window", "safe"),
    ("find_files", "Find files by name", "safe"),
    ("list_dir", "List a folder", "safe"),
    ("create_dir", "Create a folder", "safe"),
    ("press_keys", "Press keyboard shortcuts", "safe"),
    ("wait_for", "Wait for something to appear", "safe"),
    ("read_text_file", "Read a text file", "sensitive"),
    ("ui_click", "Click a control", "sensitive"),
    ("ui_set_text", "Fill in a field", "sensitive"),
    ("type_text", "Type text", "sensitive"),
    ("move_path", "Move or rename files", "destructive"),
    ("write_text_file", "Write a file", "destructive"),
    (
        "move_to_trash",
        "Move files to the Recycle Bin",
        "destructive",
    ),
];

async fn provider_info(state: &AppState, kind: ProviderKind, force: bool) -> Value {
    let status = state.hybrid.provider_status(kind, force).await;
    let caps = state.hybrid.provider(kind).capabilities();
    let choice = state
        .settings
        .lock()
        .ok()
        .and_then(|s| s.hybrid.providers.get(kind.as_str()).cloned())
        .unwrap_or_default();
    json!({
        "id": kind.as_str(),
        "label": kind.label(),
        "installed": status.installed,
        "logged_in": status.logged_in,
        "version": status.version,
        "auth_method": status.auth_method,
        "detail": status.detail,
        "caps": {
            "structured_output": caps.structured_output,
            "session_resume": caps.session_resume,
            "mcp_tools": caps.mcp_tools,
            "streaming_deltas": caps.streaming_deltas,
            "reasoning_levels": caps.reasoning_levels,
            "models": caps.models,
        },
        "model": choice.model,
        "reasoning": choice.reasoning,
    })
}

async fn hybrid_json(state: &AppState, force: bool) -> CmdResult<Value> {
    let cfg = state.settings.lock().map_err(err)?.hybrid.clone();
    let (claude, codex, browser) = tokio::join!(
        provider_info(state, ProviderKind::Claude, force),
        provider_info(state, ProviderKind::Codex, force),
        relay_agent::browser::BrowserDriver::status(state.browser.as_ref())
    );
    Ok(json!({
        "privacy_mode": cfg.privacy_mode.as_str(),
        "default_provider": cfg.default_provider,
        "providers": [claude, codex],
        "confirm_level": serde_json::to_value(cfg.confirm_level).map_err(err)?,
        "share": cfg.share,
        "allowed_roots": cfg.allowed_roots,
        "protected_roots": cfg.protected_roots,
        "disabled_tools": cfg.disabled_tools,
        "browser": {
            "enabled": cfg.browser_enabled,
            "available": browser.available,
            "server": relay_agent::browser::SERVER_LABEL,
            "detail": browser.detail,
            "mode": browser.mode,
            "action_url": browser.action_url,
            "token_set": cfg.browser_token.is_some(),
        },
        "tools": TOOLS
            .iter()
            .map(|(id, label, risk)| json!({ "id": id, "label": label, "risk": risk }))
            .collect::<Vec<_>>(),
    }))
}

#[tauri::command]
pub async fn get_hybrid(state: State<'_, AppState>) -> CmdResult<Value> {
    hybrid_json(&state, false).await
}

/// Re-probe install and sign-in state (slow: runs the CLIs).
#[tauri::command]
pub async fn refresh_providers(state: State<'_, AppState>) -> CmdResult<Value> {
    let (claude, codex) = tokio::join!(
        provider_info(&state, ProviderKind::Claude, true),
        provider_info(&state, ProviderKind::Codex, true)
    );
    Ok(json!([claude, codex]))
}

fn string_list(value: Value, what: &str) -> CmdResult<Vec<String>> {
    serde_json::from_value::<Vec<String>>(value)
        .map_err(|_| format!("{what} must be a list of text"))
}

fn trimmed(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// A partial update; absent fields are left alone.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn set_hybrid(
    privacy_mode: Option<String>,
    default_provider: Option<String>,
    confirm_level: Option<String>,
    share: Option<Value>,
    allowed_roots: Option<Value>,
    disabled_tools: Option<Value>,
    provider: Option<Value>,
    browser_enabled: Option<bool>,
    browser_token: Option<String>,
    state: State<'_, AppState>,
) -> CmdResult<Value> {
    {
        let mut settings = state.settings.lock().map_err(err)?;
        let h = &mut settings.hybrid;
        if let Some(m) = privacy_mode {
            h.privacy_mode = PrivacyMode::parse(&m).ok_or(format!("unknown privacy mode {m:?}"))?;
        }
        if let Some(p) = default_provider {
            ProviderKind::parse(&p).ok_or(format!("unknown provider {p:?}"))?;
            h.default_provider = p;
        }
        if let Some(c) = confirm_level {
            h.confirm_level = serde_json::from_value(json!(c))
                .map_err(|_| format!("unknown confirmation level {c:?}"))?;
        }
        if let Some(Value::Object(map)) = share {
            let mut cur = serde_json::to_value(h.share).map_err(err)?;
            if let Some(obj) = cur.as_object_mut() {
                for (k, v) in map {
                    if obj.contains_key(&k) && v.is_boolean() {
                        obj.insert(k, v);
                    }
                }
            }
            h.share = serde_json::from_value(cur).map_err(err)?;
        }
        if let Some(v) = allowed_roots {
            h.allowed_roots = string_list(v, "allowedRoots")?;
        }
        if let Some(v) = disabled_tools {
            h.disabled_tools = string_list(v, "disabledTools")?
                .into_iter()
                .filter(|t| TOOLS.iter().any(|(id, ..)| id == t))
                .collect();
        }
        if let Some(t) = browser_token {
            // An empty string clears it.
            let t = t.trim().to_string();
            h.browser_token = (!t.is_empty()).then_some(t);
            relay_agent::browser::set_extension_token(h.browser_token.clone());
        }
        if let Some(on) = browser_enabled {
            h.browser_enabled = on;
            state.browser.set_enabled(on);
        }
        if let Some(Value::Object(p)) = provider {
            let id = p.get("id").and_then(Value::as_str).unwrap_or_default();
            ProviderKind::parse(id).ok_or(format!("unknown provider {id:?}"))?;
            let choice = h.providers.entry(id.to_string()).or_default();
            if let Some(v) = p.get("model") {
                choice.model = trimmed(v);
            }
            if let Some(v) = p.get("reasoning") {
                choice.reasoning = trimmed(v);
            }
        }
        // Keep the legacy approval setting in step with the confirmation level.
        let approval = match h.confirm_level {
            ConfirmLevel::Off => ApprovalMode::Off,
            ConfirmLevel::Destructive => ApprovalMode::Destructive,
            ConfirmLevel::Always => ApprovalMode::Always,
        };
        settings.approval_mode = crate::settings::mode_to_str(approval).to_string();
        settings.save(&state.paths.settings_file).map_err(err)?;
        let engine = crate::settings::engine_settings(&settings.hybrid);
        drop(settings);
        state.hybrid.update_settings(|s| *s = engine);
        state.host.set_approval_mode(approval);
    }
    hybrid_json(&state, false).await
}

fn summary_of(rec: &WorkflowRecord) -> Value {
    json!({
        "id": rec.id,
        "conv_id": rec.conv_id,
        "objective": rec.objective,
        "strategy": rec.strategy,
        "provider": rec.provider,
        "outcome": rec.outcome,
        "steps_total": rec.steps_total,
        "steps_verified": rec.steps_verified,
        "created_at": rec.created_at,
        "updated_at": rec.updated_at,
        "intent": rec.intent,
        "used": rec.used,
    })
}

fn load_record(state: &AppState, id: &str) -> CmdResult<WorkflowRecord> {
    let body = state.store.workflow_get(id).map_err(err)?;
    serde_json::from_str(&body).map_err(err)
}

#[tauri::command]
pub async fn workflow_list(
    conv_id: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> CmdResult<Value> {
    let limit = limit.unwrap_or(100).clamp(1, 500);
    let rows = state.store.workflow_list(500).map_err(err)?;
    let out: Vec<Value> = rows
        .into_iter()
        .filter(|r| conv_id.is_none() || r.conv_id == conv_id)
        .take(limit)
        .filter_map(|r| load_record(&state, &r.id).ok())
        .map(|rec| summary_of(&rec))
        .collect();
    Ok(Value::Array(out))
}

#[tauri::command]
pub async fn workflow_get(id: String, state: State<'_, AppState>) -> CmdResult<Value> {
    let rec = load_record(&state, &id)?;
    let mut out = summary_of(&rec);
    if let (Some(obj), Value::Object(full)) = (
        out.as_object_mut(),
        serde_json::to_value(&rec).map_err(err)?,
    ) {
        for key in [
            "plan_source",
            "steps",
            "shared",
            "shared_bytes",
            "approvals",
            "errors",
            "notes",
        ] {
            if let Some(v) = full.get(key) {
                obj.insert(key.to_string(), v.clone());
            }
        }
    }
    Ok(out)
}

/// The owner's appearance choices (code themes, fonts, text size, task view). Opaque to the backend.
#[tauri::command]
pub async fn get_appearance(state: State<'_, AppState>) -> CmdResult<Value> {
    let s = state.settings.lock().map_err(err)?;
    Ok(if s.appearance.is_object() {
        s.appearance.clone()
    } else {
        json!({})
    })
}

/// Merge `patch` (an object) into the stored appearance and return the result.
#[tauri::command]
pub async fn set_appearance(patch: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let Value::Object(patch) = patch else {
        return Err("appearance must be an object".into());
    };
    if patch.len() > 64 || patch.keys().any(|k| k.len() > 64) {
        return Err("too many appearance settings".into());
    }
    let mut s = state.settings.lock().map_err(err)?;
    if !s.appearance.is_object() {
        s.appearance = json!({});
    }
    if let Some(obj) = s.appearance.as_object_mut() {
        for (k, v) in patch {
            if v.is_null() {
                obj.remove(&k);
            } else {
                obj.insert(k, v);
            }
        }
    }
    s.save(&state.paths.settings_file).map_err(err)?;
    Ok(s.appearance.clone())
}

fn check_item(id: &str, label: &str, status: &str, detail: String, action: Option<Value>) -> Value {
    json!({ "id": id, "label": label, "status": status, "detail": detail, "action": action })
}

/// Everything Relay depends on, what is missing, and how to fix it. `status` is ok, warn or missing;
/// `action` is `{ kind: "url" | "command", value, label }` (a command is a Relay command name to invoke).
#[tauri::command]
pub async fn system_check(state: State<'_, AppState>) -> CmdResult<Value> {
    let (overview, claude, codex, browser) = tokio::join!(
        state.agent.local().overview(),
        state.hybrid.provider_status(ProviderKind::Claude, true),
        state.hybrid.provider_status(ProviderKind::Codex, true),
        relay_agent::browser::BrowserDriver::status(state.browser.as_ref())
    );
    let mut items = Vec::new();

    let engine_installed = state.setup.engine().is_installed();
    items.push(match (&overview.active, engine_installed) {
        (Some(m), _) => check_item(
            "local_ai",
            "Relay Local AI",
            "ok",
            format!("Running {} on this PC.", m.name),
            None,
        ),
        (None, true) => check_item(
            "local_ai",
            "Relay Local AI",
            "warn",
            "Installed but not ready. Relay can start it and download a model that fits this PC."
                .into(),
            Some(json!({"kind": "command", "value": "local_ai_setup", "label": "Set up now"})),
        ),
        (None, false) => check_item(
            "local_ai",
            "Relay Local AI",
            "missing",
            "Not installed. Relay downloads and sets it up for you (several GB).".into(),
            Some(json!({"kind": "command", "value": "local_ai_setup", "label": "Install"})),
        ),
    });

    let cli = |id: &str,
               label: &str,
               st: &relay_ai::ProviderStatus,
               install: &str,
               signin: &str|
     -> Value {
        if st.installed && st.logged_in {
            check_item(
                id,
                label,
                "ok",
                format!(
                    "Installed and signed in{}.",
                    st.version
                        .as_deref()
                        .map(|v| format!(" ({v})"))
                        .unwrap_or_default()
                ),
                None,
            )
        } else if st.installed {
            check_item(
                id,
                label,
                "warn",
                format!("Installed but not signed in. {signin}"),
                None,
            )
        } else {
            check_item(
                id,
                label,
                "missing",
                format!("Not installed (optional). {install}"),
                None,
            )
        }
    };
    items.push(cli(
        "claude",
        "Claude Code",
        &claude,
        "Install it, then sign in, to let the cloud AI help with hard tasks.",
        "Open a terminal, run `claude`, and sign in.",
    ));
    items.push(cli(
        "codex",
        "Codex",
        &codex,
        "Install it, then sign in, to use it as a second cloud AI.",
        "Open a terminal, run `codex login`.",
    ));

    let node = relay_agent::browser::find_npx().is_some();
    items.push(if node {
        check_item(
            "node",
            "Node.js",
            "ok",
            "Found. Needed for browser control.".into(),
            None,
        )
    } else {
        check_item(
            "node",
            "Node.js",
            "missing",
            "Needed for browser control (website tasks).".into(),
            Some(json!({"kind": "url", "value": "https://nodejs.org/", "label": "Get Node.js"})),
        )
    });

    items.push(check_item(
        "browser",
        "Browser control",
        if browser.available { if browser.mode == "yours" { "ok" } else { "warn" } } else { "missing" },
        browser.detail.clone(),
        browser.action_url.as_ref().map(|u| {
            json!({"kind": "url", "value": u, "label": if u.contains("nodejs") { "Get Node.js" } else { "Get the Playwright extension" }})
        }),
    ));

    let ok = items.iter().filter(|i| i["status"] == "ok").count();
    Ok(json!({ "items": items, "ok": ok, "total": items.len() }))
}
