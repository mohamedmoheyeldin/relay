//! The Settings page: engines, local models, cloud CLI, safety, connections and data.

use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::{json, Value};
use wasm_bindgen::JsCast;

use crate::bridge;
use crate::chat::ChatState;

fn text(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for key in path {
        cur = &cur[*key];
    }
    cur.as_str().unwrap_or_default().to_string()
}

fn flag(v: &Value, path: &[&str]) -> bool {
    let mut cur = v;
    for key in path {
        cur = &cur[*key];
    }
    cur.as_bool().unwrap_or(false)
}

fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
}

async fn fetch(data: RwSignal<Value>) {
    if let Ok(Some(raw)) = bridge::invoke("get_engines", json!({})).await {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            data.set(v);
        }
    }
}

/// Run a backend command, then reload the page data. Failures are shown as an error banner.
fn act(chat: RwSignal<ChatState>, data: RwSignal<Value>, cmd: &'static str, args: Value) {
    spawn_local(async move {
        if let Err(message) = bridge::invoke(cmd, args).await {
            chat.update(|c| {
                c.apply(&relay_protocol::UiEvent::Error {
                    run_id: None,
                    kind: relay_protocol::ErrorKind::Internal,
                    message,
                })
            });
        }
        fetch(data).await;
    });
}

fn select_value(ev: &web_sys::Event) -> String {
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
        .map(|s| s.value())
        .unwrap_or_default()
}

#[component]
pub fn SettingsView(chat: RwSignal<ChatState>, open: RwSignal<bool>) -> impl IntoView {
    let data = RwSignal::new(Value::Null);
    let model_draft = RwSignal::new(String::new());
    let url_draft = RwSignal::new(String::new());

    Effect::new(move |_| {
        if open.get() {
            spawn_local(async move {
                fetch(data).await;
                let v = data.get_untracked();
                if model_draft.get_untracked().is_empty() {
                    model_draft.set(text(&v, &["recommended_model"]));
                }
                url_draft.set(text(&v, &["local", "base_url"]));
            });
        }
    });

    // Reload after a download finishes so the new model shows up.
    Effect::new(move |_| {
        if let Some(p) = chat.with(|c| c.progress.clone()) {
            if p.done {
                spawn_local(async move { fetch(data).await });
            }
        }
    });

    let route_button = move |mode: &'static str, label: &'static str, hint: &'static str| {
        view! {
            <button
                class="seg-item"
                class:on=move || text(&data.get(), &["route_mode"]) == mode
                on:click=move |_| act(chat, data, "set_route_mode", json!({"mode": mode}))
            >
                <strong>{label}</strong>
                <span>{hint}</span>
            </button>
        }
    };

    let local_view = move || {
        let v = data.get();
        let available = flag(&v, &["local", "available"]);
        let selected = text(&v, &["local", "selected"]);
        let active = text(&v, &["local", "active", "name"]);
        let models = v["local"]["models"].as_array().cloned().unwrap_or_default();
        let progress = chat.with(|c| c.progress.clone());
        view! {
            <div class="kv">
                <span>"Server"</span>
                <span class=if available { "dot ok" } else { "dot bad" }>
                    {if available { "Running" } else { "Not running" }}
                </span>
            </div>
            <div class="kv">
                <span>"In use now"</span>
                <strong>{if active.is_empty() { "none".to_string() } else { active.clone() }}</strong>
            </div>
            {(!available).then(|| view! {
                <p class="hint">
                    "Relay uses Ollama to run AI models on this PC. Install it, start it, then press Re-check. "
                    "Until then, every task goes to Claude."
                </p>
                <div class="actions">
                    <button class="btn primary" on:click=move |_| act(chat, data, "open_link", json!({"kind": "ollama"}))>"Get Ollama"</button>
                    <button class="btn" on:click=move |_| spawn_local(async move { fetch(data).await })>"Re-check"</button>
                </div>
            })}
            {available.then(|| {
                let auto_on = selected.is_empty();
                view! {
                    <div class="models" role="radiogroup" aria-label="Local model">
                        <label class="model" class:on=auto_on>
                            <input type="radio" name="model" checked=auto_on
                                on:change=move |_| act(chat, data, "set_local_model", json!({"model": ""})) />
                            <span class="name">"Automatic"</span>
                            <span class="tags">"best installed model"</span>
                        </label>
                        {models.iter().map(|m| {
                            let name = m["name"].as_str().unwrap_or_default().to_string();
                            let on = selected == name;
                            let picked = name.clone();
                            let size = gigabytes(m["size_bytes"].as_u64().unwrap_or_default());
                            let vision = m["vision"].as_bool().unwrap_or(false);
                            let tools = m["tools"].as_bool().unwrap_or(false);
                            view! {
                                <label class="model" class:on=on>
                                    <input type="radio" name="model" checked=on
                                        on:change=move |_| act(chat, data, "set_local_model", json!({"model": picked.clone()})) />
                                    <span class="name">{name}</span>
                                    <span class="tags">
                                        {size}
                                        {if tools { " · tools" } else { " · no tool support" }}
                                        {if vision { " · vision" } else { "" }}
                                    </span>
                                </label>
                            }
                        }).collect_view()}
                    </div>
                }
            })}
            {available.then(|| view! {
                <div class="pull">
                    <input
                        type="text"
                        aria-label="Model to download"
                        placeholder="model name, e.g. qwen3:14b"
                        prop:value=move || model_draft.get()
                        on:input=move |ev| model_draft.set(event_target_value(&ev))
                    />
                    <button class="btn primary" on:click=move |_| {
                        let name = model_draft.get_untracked();
                        act(chat, data, "pull_model", json!({"model": name}));
                    }>"Download"</button>
                </div>
            })}
            {progress.map(|p| {
                let pct = p.percent.unwrap_or(if p.done { 100 } else { 0 });
                let failed = p.error.clone();
                view! {
                    <div class="progress" role="status">
                        <div class="bar"><div class="fill" style=format!("width:{pct}%")></div></div>
                        <span class="hint">
                            {match (&failed, p.done) {
                                (Some(e), _) => format!("Download failed: {e}"),
                                (None, true) => format!("{} is ready", p.label),
                                (None, false) => format!("{} {pct}%", p.label),
                            }}
                        </span>
                    </div>
                }
            })}
            <div class="server-row">
                <input
                    type="text"
                    aria-label="Local AI server address"
                    prop:value=move || url_draft.get()
                    on:input=move |ev| url_draft.set(event_target_value(&ev))
                />
                <button class="btn" on:click=move |_| act(chat, data, "set_local_url", json!({"url": url_draft.get_untracked()}))>"Save address"</button>
                <button class="btn" on:click=move |_| spawn_local(async move { fetch(data).await })>"Re-check"</button>
            </div>
        }
    };

    view! {
        <Show when=move || open.get()>
            <section class="page" aria-label="Settings">
                <header class="page-top">
                    <button class="iconbtn" aria-label="Back to chat" on:click=move |_| open.set(false)>"←"</button>
                    <h2>"Settings"</h2>
                </header>
                <div class="page-body">
                    <section class="panel">
                        <h3>"Which AI does the work"</h3>
                        <p class="hint">"Relay can use an AI running on this PC for everyday automation and hand harder jobs to Claude."</p>
                        <div class="seg" role="group" aria-label="Routing">
                            {route_button("auto", "Auto", "Local first, Claude for hard or web tasks")}
                            {route_button("local_only", "Local only", "Never use the cloud")}
                            {route_button("cloud_only", "Claude only", "Always use Claude Code")}
                        </div>
                    </section>

                    <section class="panel">
                        <h3>"Local AI"</h3>
                        {local_view}
                    </section>

                    <section class="panel">
                        <h3>"Cloud AI: Claude Code"</h3>
                        <div class="kv"><span>"Installed"</span><span class=move || if flag(&data.get(), &["cloud", "installed"]) { "dot ok" } else { "dot bad" }>{move || if flag(&data.get(), &["cloud", "installed"]) { "Yes" } else { "No" }}</span></div>
                        <div class="kv"><span>"Signed in"</span><span class=move || if flag(&data.get(), &["cloud", "logged_in"]) { "dot ok" } else { "dot bad" }>{move || if flag(&data.get(), &["cloud", "logged_in"]) { "Yes" } else { "No" }}</span></div>
                        <code class="path">{move || text(&data.get(), &["cloud", "path"])}</code>
                        <p class="hint">"Uses your Claude sign-in through the command line. To sign in, run: claude auth login"</p>
                    </section>

                    <section class="panel">
                        <h3>"Safety"</h3>
                        <label class="field" for="approval">"Ask me before"</label>
                        <select
                            id="approval"
                            prop:value=move || text(&data.get(), &["approval_mode"])
                            on:change=move |ev| act(chat, data, "set_approval_mode", json!({"mode": select_value(&ev)}))
                        >
                            <option value="off">"Nothing: work without asking"</option>
                            <option value="destructive">"Destructive or irreversible actions"</option>
                            <option value="always">"Every action"</option>
                        </select>
                        <div class="kv"><span>"Emergency stop"</span><strong>{move || text(&data.get(), &["hotkey"])}</strong></div>
                    </section>

                    <section class="panel">
                        <h3>"Connections (MCP)"</h3>
                        <p class="hint">"Gmail, GitHub and other services connect as MCP servers listed in this file."</p>
                        <code class="path">{move || text(&data.get(), &["mcp_config"])}</code>
                        {move || {
                            let v = data.get();
                            let names: Vec<String> = v["mcp_servers"].as_array().map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default();
                            let error = text(&v, &["mcp_error"]);
                            view! {
                                {if names.is_empty() {
                                    view! { <p class="hint">"No servers added yet."</p> }.into_any()
                                } else {
                                    view! { <ul class="chips">{names.into_iter().map(|n| view! { <li>{n}</li> }).collect_view()}</ul> }.into_any()
                                }}
                                {(!error.is_empty()).then(|| view! { <p class="hint warn">{error}</p> })}
                            }
                        }}
                        <button class="btn" on:click=move |_| act(chat, data, "open_link", json!({"kind": "mcp"}))>"Open folder"</button>
                    </section>

                    <section class="panel">
                        <h3>"Data"</h3>
                        <code class="path">{move || text(&data.get(), &["data_dir"])}</code>
                        <div class="actions">
                            <button class="btn" on:click=move |_| act(chat, data, "open_link", json!({"kind": "data"}))>"Relay folder"</button>
                            <button class="btn" on:click=move |_| act(chat, data, "open_link", json!({"kind": "audit"}))>"Action history"</button>
                            <button class="btn" on:click=move |_| act(chat, data, "open_link", json!({"kind": "logs"}))>"Logs"</button>
                        </div>
                        <p class="hint">{move || format!("Relay {}", text(&data.get(), &["version"]))}</p>
                    </section>
                </div>
            </section>
        </Show>
    }
}
