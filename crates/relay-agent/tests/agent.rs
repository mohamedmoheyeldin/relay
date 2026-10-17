#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! End-to-end tests of the agent with a scripted AI provider and the real MCP server.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use relay_agent::{
    Agent, AgentConfig, AgentParts, ConfirmBridge, EventSink, LocalConfig, LocalProvider,
    MemoryStore, RouteMode, SubmitError,
};
use relay_ai::{
    AiError, AiEvent, AiProvider, Finish, ProviderStatus, RunHandle, RunRequest, SessionMode,
    Timeouts,
};
use relay_mcp::{HostOptions, KillSwitch, McpServer, ToolHost};
use relay_os::driver::{Driver, Timing};
use relay_os::testing::FakeDesktop;
use relay_os::InputEvent;
use relay_protocol::{ConfirmAnswer, ErrorKind, RunOutcome, UiEvent};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
enum Script {
    /// Emit text and a tool round trip, then complete.
    Complete,
    /// Wait until cancelled.
    Hang,
    /// Fail before the process exists.
    RunError(AiError),
    /// Start fine, then report a failed run.
    FinishFailed(AiError),
    /// Act like the AI CLI: call a destructive click through the MCP server, then report the outcome.
    DestructiveClick,
}

struct Scripted {
    script: Mutex<Script>,
    seen: Mutex<Vec<RunRequest>>,
}

impl Scripted {
    fn new(script: Script) -> Arc<Self> {
        Arc::new(Self {
            script: Mutex::new(script),
            seen: Mutex::new(Vec::new()),
        })
    }
    fn set(&self, script: Script) {
        *self.script.lock().unwrap() = script;
    }
    fn requests(&self) -> Vec<RunRequest> {
        self.seen.lock().unwrap().clone()
    }
}

async fn mcp_call(request: &RunRequest, name: &str, args: Value) -> Value {
    let cfg = &request.mcp_servers["relay"];
    let url = cfg["url"].as_str().unwrap();
    let auth = cfg["headers"]["Authorization"].as_str().unwrap();
    reqwest::Client::new()
        .post(url)
        .header("Authorization", auth)
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[async_trait]
impl AiProvider for Scripted {
    fn name(&self) -> &str {
        "Scripted"
    }

    fn capabilities(&self) -> relay_ai::ProviderCaps {
        relay_ai::ProviderCaps {
            id: "test",
            label: "Test",
            structured_output: false,
            session_resume: false,
            mcp_tools: true,
            streaming_deltas: false,
            reasoning_levels: Vec::new(),
            models: Vec::new(),
        }
    }

    async fn status(&self) -> ProviderStatus {
        ProviderStatus {
            installed: true,
            logged_in: true,
            detail: "Ready".into(),
            ..ProviderStatus::default()
        }
    }

    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError> {
        let script = self.script.lock().unwrap().clone();
        self.seen.lock().unwrap().push(request.clone());
        if let Script::RunError(e) = &script {
            return Err(e.clone());
        }
        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let session = match &request.session {
            SessionMode::New(id) | SessionMode::Resume(id) => id.clone(),
        };
        tokio::spawn(async move {
            let _ = tx
                .send(AiEvent::Started {
                    session_id: session.clone(),
                    model: None,
                })
                .await;
            match script {
                Script::Complete => {
                    let _ = tx.send(AiEvent::TextDelta("Hel".into())).await;
                    let _ = tx.send(AiEvent::Text("Hello".into())).await;
                    let _ = tx
                        .send(AiEvent::ToolUse {
                            id: "t1".into(),
                            name: "mcp__relay__screenshot".into(),
                            input: json!({}),
                        })
                        .await;
                    let _ = tx
                        .send(AiEvent::ToolResult {
                            id: "t1".into(),
                            is_error: false,
                            content: "Screenshot".into(),
                        })
                        .await;
                    let _ = tx
                        .send(AiEvent::Finished(Finish::Completed {
                            session_id: Some(session),
                            result: "Hello".into(),
                            cost_usd: None,
                        }))
                        .await;
                }
                Script::Hang => {
                    token.cancelled().await;
                    let _ = tx.send(AiEvent::Finished(Finish::Cancelled)).await;
                }
                Script::FinishFailed(e) => {
                    let _ = tx.send(AiEvent::Finished(Finish::Failed(e))).await;
                }
                Script::DestructiveClick => {
                    let reply = mcp_call(
                        &request,
                        "left_click",
                        json!({"coordinate": [10, 10], "destructive": true}),
                    )
                    .await;
                    let text = reply["result"]["content"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let is_error = reply["result"]["isError"].as_bool().unwrap_or(false);
                    let _ = tx
                        .send(AiEvent::ToolResult {
                            id: "c1".into(),
                            is_error,
                            content: text.clone(),
                        })
                        .await;
                    let _ = tx
                        .send(AiEvent::Finished(Finish::Completed {
                            session_id: Some(session),
                            result: text,
                            cost_usd: None,
                        }))
                        .await;
                }
                Script::RunError(_) => {}
            }
        });
        Ok(RunHandle::new(rx, cancel))
    }
}

struct World {
    agent: Arc<Agent>,
    provider: Arc<Scripted>,
    events: Arc<Mutex<Vec<UiEvent>>>,
    fake: Arc<FakeDesktop>,
    kill: KillSwitch,
    _server: Arc<McpServer>,
}

async fn world(script: Script, user_mcp: Option<std::path::PathBuf>) -> World {
    let events: Arc<Mutex<Vec<UiEvent>>> = Arc::default();
    let sink: EventSink = {
        let events = events.clone();
        Arc::new(move |e| events.lock().unwrap().push(e))
    };
    let fake = Arc::new(FakeDesktop::new());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        fake.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let bridge = ConfirmBridge::new(sink.clone(), Duration::from_secs(10));
    let host = Arc::new(ToolHost::new(
        fake.clone(),
        driver,
        kill.clone(),
        bridge.clone(),
        None,
        HostOptions {
            change_verdicts: false,
            ..HostOptions::default()
        },
    ));
    let server = Arc::new(McpServer::start(host.clone()).await.unwrap());
    let provider = Scripted::new(script);
    let local = LocalProvider::new(
        LocalConfig {
            base_url: "http://127.0.0.1:1".into(),
            model: None,
        },
        host.clone(),
    );
    let agent = Agent::new(
        AgentParts {
            cloud: provider.clone(),
            local,
            route: RouteMode::Auto,
            mcp: server.clone(),
            kill: kill.clone(),
            bridge,
            store: Arc::new(MemoryStore::default()),
        },
        AgentConfig {
            work_dir: std::env::temp_dir(),
            user_mcp_config: user_mcp,
            timeouts: Timeouts::default(),
            model: None,
            kill_hotkey: "Ctrl+Alt+Shift+Esc".into(),
        },
        sink,
    );
    World {
        agent,
        provider,
        events,
        fake,
        kill,
        _server: server,
    }
}

async fn wait_for<F: Fn(&UiEvent) -> bool>(w: &World, pred: F) -> UiEvent {
    for _ in 0..500 {
        if let Some(e) = w.events.lock().unwrap().iter().find(|e| pred(e)).cloned() {
            return e;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("event never arrived; saw {:?}", w.events.lock().unwrap());
}

async fn wait_finished(w: &World) -> RunOutcome {
    match wait_for(w, |e| matches!(e, UiEvent::RunFinished { .. })).await {
        UiEvent::RunFinished { outcome, .. } => outcome,
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn a_task_streams_progress_and_completes() {
    let w = world(Script::Complete, None).await;
    let run_id = w.agent.submit("do the thing").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    let events = w.events.lock().unwrap().clone();
    assert_eq!(
        events[0],
        UiEvent::RunStarted {
            run_id,
            conv_id: "default".into()
        }
    );
    assert!(events.contains(&UiEvent::AssistantDelta {
        run_id,
        text: "Hel".into()
    }));
    assert!(events.contains(&UiEvent::AssistantMessage {
        run_id,
        text: "Hello".into(),
        needs_user: false,
    }));
    assert!(events.iter().any(|e| matches!(e, UiEvent::ToolStarted { name, summary, .. } if name == "screenshot" && summary == "Take a screenshot")));
    assert!(events
        .iter()
        .any(|e| matches!(e, UiEvent::ToolFinished { ok: true, .. })));
    assert!(!w.agent.is_busy());
}

#[tokio::test]
async fn request_carries_prompt_tools_and_safety_settings() {
    let w = world(Script::Complete, None).await;
    w.agent.submit("  open notepad  ").unwrap();
    wait_finished(&w).await;
    let req = &w.provider.requests()[0];
    assert_eq!(req.prompt, "open notepad");
    assert_eq!(
        req.permission_prompt_tool.as_deref(),
        Some("mcp__relay__approve")
    );
    assert_eq!(req.allowed_tools, vec!["mcp__relay".to_string()]);
    assert_eq!(req.disallowed_tools, vec!["AskUserQuestion".to_string()]);
    assert!(req
        .system_prompt_append
        .as_deref()
        .unwrap()
        .contains("Do not ask the owner questions"));
    assert_eq!(req.mcp_servers["relay"]["type"], "http");
    assert!(matches!(req.session, SessionMode::New(_)));
}

#[tokio::test]
async fn empty_and_concurrent_submissions_are_rejected() {
    let w = world(Script::Hang, None).await;
    assert_eq!(w.agent.submit("   ").unwrap_err(), SubmitError::Empty);
    w.agent.submit("first").unwrap();
    assert_eq!(w.agent.submit("second").unwrap_err(), SubmitError::Busy);
    w.agent.stop();
    wait_finished(&w).await;
}

#[tokio::test]
async fn follow_up_messages_resume_the_same_conversation() {
    let w = world(Script::Complete, None).await;
    w.agent.submit("one").unwrap();
    wait_finished(&w).await;
    w.events.lock().unwrap().clear();
    w.agent.submit("two").unwrap();
    wait_finished(&w).await;
    let reqs = w.provider.requests();
    let first = match &reqs[0].session {
        SessionMode::New(id) => id.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(reqs[1].session, SessionMode::Resume(first));
    w.agent.forget("default");
    w.events.lock().unwrap().clear();
    w.agent.submit("three").unwrap();
    wait_finished(&w).await;
    assert!(matches!(
        w.provider.requests()[2].session,
        SessionMode::New(_)
    ));
}

#[tokio::test]
async fn stop_cancels_the_run_and_the_next_task_rearms() {
    let w = world(Script::Hang, None).await;
    w.agent.submit("long task").unwrap();
    wait_for(&w, |e| matches!(e, UiEvent::RunStarted { .. })).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    w.agent.stop();
    assert!(w.kill.is_tripped());
    assert_eq!(wait_finished(&w).await, RunOutcome::Cancelled);
    assert!(!w.agent.is_busy());

    w.provider.set(Script::Complete);
    w.events.lock().unwrap().clear();
    w.agent.submit("next").unwrap();
    assert!(!w.kill.is_tripped(), "a new task lifts the stop");
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
}

#[tokio::test]
async fn destructive_action_waits_for_the_owner_and_runs_when_approved() {
    let w = world(Script::DestructiveClick, None).await;
    w.agent.submit("delete things").unwrap();
    let card = match wait_for(&w, |e| matches!(e, UiEvent::Confirm(_))).await {
        UiEvent::Confirm(c) => c,
        _ => unreachable!(),
    };
    assert_eq!(card.risk, relay_protocol::Risk::Destructive);
    assert!(
        w.fake.events().is_empty(),
        "nothing happens before approval"
    );
    assert!(w.agent.answer(ConfirmAnswer {
        id: card.id,
        approved: true
    }));
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert!(w
        .fake
        .events()
        .iter()
        .any(|e| matches!(e, InputEvent::Button { down: true, .. })));
    let events = w.events.lock().unwrap().clone();
    assert!(events
        .iter()
        .any(|e| matches!(e, UiEvent::ConfirmResolved { approved: true, .. })));
}

#[tokio::test]
async fn declined_action_never_reaches_the_desktop() {
    let w = world(Script::DestructiveClick, None).await;
    w.agent.submit("delete things").unwrap();
    let card = match wait_for(&w, |e| matches!(e, UiEvent::Confirm(_))).await {
        UiEvent::Confirm(c) => c,
        _ => unreachable!(),
    };
    w.agent.answer(ConfirmAnswer {
        id: card.id,
        approved: false,
    });
    wait_finished(&w).await;
    assert!(w.fake.events().is_empty());
    let events = w.events.lock().unwrap().clone();
    assert!(events.iter().any(|e| matches!(e, UiEvent::ToolFinished { ok: false, summary, .. } if summary.contains("declined"))));
}

#[tokio::test]
async fn stop_while_a_card_is_open_denies_it() {
    let w = world(Script::DestructiveClick, None).await;
    w.agent.submit("delete things").unwrap();
    wait_for(&w, |e| matches!(e, UiEvent::Confirm(_))).await;
    w.agent.stop();
    wait_finished(&w).await;
    assert!(w.fake.events().is_empty());
    wait_for(&w, |e| {
        matches!(
            e,
            UiEvent::ConfirmResolved {
                approved: false,
                ..
            }
        )
    })
    .await;
}

#[tokio::test]
async fn the_runs_credentials_stop_working_afterwards() {
    let w = world(Script::Complete, None).await;
    w.agent.submit("x").unwrap();
    wait_finished(&w).await;
    let req = w.provider.requests()[0].clone();
    let cfg = &req.mcp_servers["relay"];
    let status = reqwest::Client::new()
        .post(cfg["url"].as_str().unwrap())
        .header(
            "Authorization",
            cfg["headers"]["Authorization"].as_str().unwrap(),
        )
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 401);
}

#[tokio::test]
async fn startup_failures_become_actionable_errors() {
    let w = world(
        Script::RunError(AiError::NotInstalled {
            program: "claude".into(),
        }),
        None,
    )
    .await;
    w.agent.submit("hi").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Failed);
    let err = wait_for(&w, |e| matches!(e, UiEvent::Error { .. })).await;
    assert!(matches!(
        err,
        UiEvent::Error {
            kind: ErrorKind::CliNotInstalled,
            ..
        }
    ));
    assert!(!w.agent.is_busy(), "a failed start must free the agent");

    w.provider
        .set(Script::FinishFailed(AiError::NotLoggedIn("x".into())));
    w.events.lock().unwrap().clear();
    w.agent.submit("hi again").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Failed);
    let err = wait_for(&w, |e| matches!(e, UiEvent::Error { .. })).await;
    assert!(
        matches!(err, UiEvent::Error { kind: ErrorKind::NotLoggedIn, ref message, .. } if message.contains("claude auth login"))
    );
}

#[tokio::test]
async fn owners_mcp_servers_are_added_but_cannot_replace_relay() {
    let dir = std::env::temp_dir().join(format!("relay-agent-it-{}", std::process::id()));
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let file = dir.join("mcp.json");
    tokio::fs::write(
        &file,
        r#"{"mcpServers":{"gmail":{"command":"gmail-mcp"},"relay":{"command":"evil"}}}"#,
    )
    .await
    .unwrap();
    let w = world(Script::Complete, Some(file)).await;
    w.agent.submit("x").unwrap();
    wait_finished(&w).await;
    let servers = &w.provider.requests()[0].mcp_servers;
    assert_eq!(servers["gmail"]["command"], "gmail-mcp");
    assert_eq!(servers["relay"]["type"], "http");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn status_reports_readiness_busy_and_hotkey() {
    let w = world(Script::Hang, None).await;
    let s = w.agent.status().await;
    assert!(s.ready && !s.busy);
    assert_eq!(s.provider, "Scripted");
    assert_eq!(s.kill_hotkey, "Ctrl+Alt+Shift+Esc");
    w.agent.submit("x").unwrap();
    assert!(w.agent.status().await.busy);
    w.agent.stop();
    wait_finished(&w).await;
}

#[tokio::test]
async fn each_conversation_keeps_its_own_session_and_attachments_reach_the_prompt() {
    let w = world(Script::Complete, None).await;
    w.agent.submit_in("a", "first in a", &[]).unwrap();
    wait_finished(&w).await;
    w.events.lock().unwrap().clear();
    w.agent
        .submit_in("b", "first in b", &["C:\\docs\\cv.pdf".to_string()])
        .unwrap();
    wait_finished(&w).await;
    w.events.lock().unwrap().clear();
    w.agent.submit_in("a", "second in a", &[]).unwrap();
    wait_finished(&w).await;
    let reqs = w.provider.requests();
    let a1 = match &reqs[0].session {
        SessionMode::New(id) => id.clone(),
        other => panic!("{other:?}"),
    };
    assert!(
        matches!(reqs[1].session, SessionMode::New(_)),
        "b is a separate chat"
    );
    assert!(reqs[1].prompt.contains("Attached files:") && reqs[1].prompt.contains("cv.pdf"));
    assert_eq!(reqs[2].session, SessionMode::Resume(a1));
    let started = w
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, UiEvent::RunStarted { conv_id, .. } if conv_id == "a"));
    assert!(started);
}
