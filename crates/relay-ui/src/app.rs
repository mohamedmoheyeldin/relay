//! The chat window.

use leptos::prelude::*;
use leptos::task::spawn_local;
use relay_protocol::{ErrorKind, Risk, StatusInfo, UiEvent};
use serde_json::{json, Value};
use std::time::Duration;

use crate::bridge;
use crate::chat::{ChatState, ConfirmState, Item, ToolState};
use crate::settings_view::SettingsView;

fn report(chat: RwSignal<ChatState>, message: String) {
    chat.update(|c| {
        c.apply(&UiEvent::Error {
            run_id: None,
            kind: ErrorKind::Internal,
            message,
        })
    });
}

fn call(chat: RwSignal<ChatState>, cmd: &'static str, args: Value) {
    spawn_local(async move {
        if let Err(e) = bridge::invoke(cmd, args).await {
            report(chat, e);
        }
    });
}

async fn refresh_status(chat: RwSignal<ChatState>) {
    match bridge::invoke("get_status", json!({})).await {
        Ok(Some(text)) => match serde_json::from_str::<StatusInfo>(&text) {
            Ok(status) => chat.update(|c| c.apply(&UiEvent::Status(status))),
            Err(e) => report(chat, format!("bad status: {e}")),
        },
        Ok(None) => {}
        Err(e) => report(chat, e),
    }
}

fn send(chat: RwSignal<ChatState>, draft: RwSignal<String>) {
    let text = draft.get_untracked().trim().to_string();
    if text.is_empty() || chat.with_untracked(|c| c.running) {
        return;
    }
    draft.set(String::new());
    chat.update(|c| c.push_user(&text));
    call(chat, "send_message", json!({ "text": text }));
}

fn error_hint(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::CliNotInstalled => "Claude Code is missing",
        ErrorKind::NotLoggedIn => "Sign-in needed",
        ErrorKind::Timeout => "Timed out",
        ErrorKind::Cancelled => "Stopped",
        ErrorKind::Cli => "Claude Code reported a problem",
        ErrorKind::Internal => "Something went wrong",
    }
}

fn render_item(item: Item, chat: RwSignal<ChatState>) -> AnyView {
    match item {
        Item::User(text) => {
            view! { <div class="msg user"><div class="bubble">{text}</div></div> }.into_any()
        }
        Item::Assistant { text, open } => view! {
            <div class="msg assistant">
                <div class="bubble" class:streaming=open>{text}</div>
            </div>
        }
        .into_any(),
        Item::Tool {
            name,
            summary,
            state,
            ..
        } => {
            let (mark, class, detail) = match &state {
                ToolState::Running => ("…", "tool running", String::new()),
                ToolState::Done => ("✓", "tool ok", String::new()),
                ToolState::Failed(why) => ("✕", "tool failed", why.clone()),
            };
            let shown = if summary.is_empty() {
                name.clone()
            } else {
                summary
            };
            view! {
                <div class=class title=detail.clone()>
                    <span class="mark">{mark}</span>
                    <span class="what">{shown}</span>
                    {(!detail.is_empty()).then(|| view! { <span class="why">{detail.clone()}</span> })}
                </div>
            }
            .into_any()
        }
        Item::Error { kind, message } => view! {
            <div class="banner error" role="alert">
                <strong>{error_hint(kind)}</strong>
                <span>{message}</span>
            </div>
        }
        .into_any(),
        Item::Notice(text) => view! { <div class="notice">{text}</div> }.into_any(),
        Item::Confirm { request, state } => {
            let id = request.id;
            let destructive = request.risk == Risk::Destructive;
            let waiting = state == ConfirmState::Waiting;
            let outcome = match state {
                ConfirmState::Waiting => "",
                ConfirmState::Approved => "Approved",
                ConfirmState::Declined => "Declined",
            };
            view! {
                <div class="card" class:destructive=destructive role="alertdialog" aria-label="Approval needed">
                    <div class="card-title">
                        {if destructive { "Approval needed: this can't be undone" } else { "Approval needed" }}
                    </div>
                    <div class="card-what">{request.title}</div>
                    <div class="card-detail">{request.detail}</div>
                    {if waiting {
                        view! {
                            <div class="card-actions">
                                <button class="btn primary" on:click=move |_| call(chat, "answer_confirm", json!({"id": id, "approved": true}))>"Approve"</button>
                                <button class="btn" on:click=move |_| call(chat, "answer_confirm", json!({"id": id, "approved": false}))>"Decline"</button>
                            </div>
                        }
                        .into_any()
                    } else {
                        view! { <div class="card-outcome">{outcome}</div> }.into_any()
                    }}
                </div>
            }
            .into_any()
        }
    }
}

fn status_chip(
    status: Option<StatusInfo>,
    running: bool,
    engine: Option<(String, String)>,
) -> (String, &'static str) {
    match status {
        None => ("Checking…".into(), "chip"),
        Some(_) if running => (
            match engine {
                Some((kind, name)) if kind == "local" => format!("Working · local {name}"),
                Some((_, name)) => format!("Working · {name}"),
                None => "Working".to_string(),
            },
            "chip busy",
        ),
        Some(s) if s.ready => ("Ready".into(), "chip ok"),
        Some(s) => (s.detail, "chip warn"),
    }
}

#[component]
pub fn App() -> impl IntoView {
    let chat = RwSignal::new(ChatState::default());
    let draft = RwSignal::new(String::new());
    let settings_open = RwSignal::new(false);
    let list_ref = NodeRef::<leptos::html::Div>::new();

    spawn_local(async move {
        bridge::listen_events(move |event| chat.update(|c| c.apply(&event))).await;
        refresh_status(chat).await;
    });

    // Keep the newest message in view.
    Effect::new(move |_| {
        chat.track();
        set_timeout(
            move || {
                if let Some(el) = list_ref.get_untracked() {
                    el.set_scroll_top(el.scroll_height());
                }
            },
            Duration::from_millis(0),
        );
    });

    let running = move || chat.with(|c| c.running);
    let chip = move || chat.with(|c| status_chip(c.status.clone(), c.running, c.engine.clone()));
    let hotkey = move || {
        chat.with(|c| {
            c.status
                .as_ref()
                .map(|s| s.kill_hotkey.clone())
                .unwrap_or_default()
        })
    };

    view! {
        <main class="app">
            <header class="top">
                <h1><span class="logo" aria-hidden="true">"✻"</span>"Relay"</h1>
                <span class=move || chip().1>{move || chip().0}</span>
                <div class="spacer"></div>
                <button class="iconbtn" title="New chat" aria-label="New chat" on:click=move |_| {
                    chat.update(|c| c.clear());
                    call(chat, "new_conversation", json!({}));
                }>"＋"</button>
                <button
                    class="iconbtn"
                    title="Settings"
                    aria-label="Settings"
                    aria-expanded=move || settings_open.get().to_string()
                    on:click=move |_| settings_open.update(|o| *o = !*o)
                >
                    "⚙"
                </button>
            </header>
            <SettingsView chat=chat open=settings_open />
            <div class="list" node_ref=list_ref aria-live="polite">
                {move || {
                    let items = chat.with(|c| c.items.clone());
                    if items.is_empty() {
                        view! {
                            <div class="empty">
                                <div class="spark" aria-hidden="true">"✻"</div>
                                <h2>"What can I do for you?"</h2>
                                <p class="hint">{move || format!("Emergency stop: {}", hotkey())}</p>
                            </div>
                        }
                        .into_any()
                    } else {
                        items
                            .into_iter()
                            .map(|item| render_item(item, chat))
                            .collect_view()
                            .into_any()
                    }
                }}
            </div>
            <footer class="composer">
                <div class="box">
                    <textarea
                        rows="1"
                        placeholder="How can I help you today?"
                        prop:value=move || draft.get()
                        on:input=move |ev| draft.set(event_target_value(&ev))
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if ev.key() == "Enter" && !ev.shift_key() && !ev.is_composing() {
                                ev.prevent_default();
                                send(chat, draft);
                            }
                        }
                    ></textarea>
                    <div class="row">
                        <span class="note">{move || if running() { format!("Working… stop with {}", hotkey()) } else { String::new() }}</span>
                        <Show
                            when=running
                            fallback=move || view! {
                                <button
                                    class="send"
                                    aria-label="Send message"
                                    disabled=move || draft.get().trim().is_empty()
                                    on:click=move |_| send(chat, draft)
                                >"↑"</button>
                            }
                        >
                            <button
                                class="send stop"
                                aria-label="Stop"
                                title=move || format!("Stop everything ({})", hotkey())
                                on:click=move |_| call(chat, "stop_task", json!({}))
                            >"■"</button>
                        </Show>
                    </div>
                </div>
            </footer>
        </main>
    }
}

/// Mount the app.
pub fn start() {
    console_error_panic_hook_setup();
    leptos::mount::mount_to_body(App);
}

fn console_error_panic_hook_setup() {
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("panic: {info}").into());
    }));
}
