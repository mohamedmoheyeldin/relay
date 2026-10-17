#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! The local engine against a fake Ollama server, routed through the real agent.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use relay_agent::local::{OllamaClient, PullProgress};
use relay_agent::{
    Agent, AgentConfig, AgentParts, ConfirmBridge, EventSink, LocalConfig, LocalProvider,
    MemoryStore, RouteMode,
};
use relay_ai::{
    AiError, AiEvent, AiProvider, Finish, ProviderStatus, RunHandle, RunRequest, Timeouts,
};
use relay_mcp::{ApprovalMode, HostOptions, KillSwitch, McpServer, ToolHost};
use relay_os::driver::{Driver, Timing};
use relay_os::testing::FakeDesktop;
use relay_protocol::{RunOutcome, UiEvent};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Fake {
    replies: Mutex<VecDeque<Value>>,
    chats: Mutex<Vec<Value>>,
    models: Mutex<Vec<(String, Vec<&'static str>)>>,
    hang: Mutex<bool>,
}

type Shared = Arc<Fake>;

async fn tags(State(s): State<Shared>) -> Json<Value> {
    let models: Vec<Value> = s
        .models
        .lock()
        .unwrap()
        .iter()
        .map(|(n, _)| json!({"name": n, "size": 5_000_000_000u64}))
        .collect();
    Json(json!({ "models": models }))
}

async fn show(State(s): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    let name = body["model"].as_str().unwrap_or_default().to_string();
    let caps = s
        .models
        .lock()
        .unwrap()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, c)| c.clone())
        .unwrap_or_default();
    Json(json!({ "capabilities": caps }))
}

async fn chat(State(s): State<Shared>, Json(body): Json<Value>) -> Json<Value> {
    s.chats.lock().unwrap().push(body);
    if *s.hang.lock().unwrap() {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
    let next = s.replies.lock().unwrap().pop_front();
    Json(json!({"message": next.unwrap_or_else(|| json!({"role":"assistant","content":"done"}))}))
}

async fn pull() -> String {
    [
        json!({"status": "pulling manifest"}),
        json!({"status": "downloading", "completed": 50, "total": 100}),
        json!({"status": "downloading", "completed": 100, "total": 100}),
        json!({"status": "success"}),
    ]
    .iter()
    .map(|l| format!("{l}\n"))
    .collect()
}

async fn serve(fake: Shared) -> String {
    let app = Router::new()
        .route("/api/tags", get(tags))
        .route("/api/show", post(show))
        .route("/api/chat", post(chat))
        .route("/api/pull", post(pull))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

fn tool_call(name: &str, args: Value) -> Value {
    json!({"role":"assistant","content":"","tool_calls":[{"function":{"name":name,"arguments":args}}]})
}

fn say(text: &str) -> Value {
    json!({"role":"assistant","content":text})
}

/// A cloud provider that records requests and completes.
struct Cloud {
    seen: Mutex<Vec<RunRequest>>,
}

#[async_trait]
impl AiProvider for Cloud {
    fn name(&self) -> &str {
        "Cloud"
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
        self.seen.lock().unwrap().push(request);
        let (tx, rx) = mpsc::channel(8);
        tokio::spawn(async move {
            let _ = tx.send(AiEvent::Text("cloud finished".into())).await;
            let _ = tx
                .send(AiEvent::Finished(Finish::Completed {
                    session_id: Some("cloud-session".into()),
                    result: "cloud finished".into(),
                    cost_usd: None,
                }))
                .await;
        });
        Ok(RunHandle::new(rx, CancellationToken::new()))
    }
}

struct World {
    agent: Arc<Agent>,
    fake: Shared,
    cloud: Arc<Cloud>,
    events: Arc<Mutex<Vec<UiEvent>>>,
    _server: Arc<McpServer>,
}

async fn world(mode: RouteMode, models: Vec<(&str, Vec<&'static str>)>) -> World {
    let fake: Shared = Arc::default();
    *fake.models.lock().unwrap() = models
        .into_iter()
        .map(|(n, c)| (n.to_string(), c))
        .collect();
    let url = serve(fake.clone()).await;
    let events: Arc<Mutex<Vec<UiEvent>>> = Arc::default();
    let sink: EventSink = {
        let events = events.clone();
        Arc::new(move |e| events.lock().unwrap().push(e))
    };
    let desktop = Arc::new(FakeDesktop::new());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        desktop.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let bridge = ConfirmBridge::new(sink.clone(), Duration::from_secs(5));
    let host = Arc::new(ToolHost::new(
        desktop,
        driver,
        kill.clone(),
        bridge.clone(),
        None,
        HostOptions {
            approval_mode: ApprovalMode::Off,
            change_verdicts: false,
            ..HostOptions::default()
        },
    ));
    let server = Arc::new(McpServer::start(host.clone()).await.unwrap());
    let local = LocalProvider::new(
        LocalConfig {
            base_url: url,
            model: None,
        },
        host,
    );
    let cloud = Arc::new(Cloud {
        seen: Mutex::new(Vec::new()),
    });
    let agent = Agent::new(
        AgentParts {
            cloud: cloud.clone(),
            local,
            route: mode,
            mcp: server.clone(),
            kill,
            bridge,
            store: Arc::new(MemoryStore::default()),
        },
        AgentConfig {
            work_dir: std::env::temp_dir(),
            user_mcp_config: None,
            timeouts: Timeouts::default(),
            model: None,
            kill_hotkey: "Ctrl+Alt+Shift+Esc".into(),
        },
        sink,
    );
    World {
        agent,
        fake,
        cloud,
        events,
        _server: server,
    }
}

async fn wait_finished(w: &World) -> RunOutcome {
    for _ in 0..1000 {
        if let Some(UiEvent::RunFinished { outcome, .. }) = w
            .events
            .lock()
            .unwrap()
            .iter()
            .find(|e| matches!(e, UiEvent::RunFinished { .. }))
            .cloned()
        {
            return outcome;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never finished: {:?}", w.events.lock().unwrap());
}

fn has(w: &World, pred: impl Fn(&UiEvent) -> bool) -> bool {
    w.events.lock().unwrap().iter().any(pred)
}

const TOOLS: [&str; 2] = ["completion", "tools"];

#[tokio::test]
async fn the_local_model_runs_tools_and_answers_without_the_cloud() {
    let w = world(RouteMode::Auto, vec![("qwen3:14b", TOOLS.to_vec())]).await;
    {
        let mut q = w.fake.replies.lock().unwrap();
        q.push_back(tool_call(
            "list_dir",
            json!({"path": std::env::temp_dir().to_string_lossy()}),
        ));
        q.push_back(say("Listed the temp folder."));
    }
    w.agent.submit("list the files in my temp folder").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::Engine { kind, name, .. } if kind == "local" && name == "qwen3:14b")
    ));
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::ToolStarted { name, .. } if name == "list_dir")
    ));
    assert!(has(&w, |e| matches!(
        e,
        UiEvent::ToolFinished { ok: true, .. }
    )));
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::AssistantMessage { text, .. } if text == "Listed the temp folder.")
    ));
    assert!(
        w.cloud.seen.lock().unwrap().is_empty(),
        "cloud must not run"
    );
    // The second chat request carried the tool result back to the model.
    let chats = w.fake.chats.lock().unwrap().clone();
    assert_eq!(chats.len(), 2);
    let last = chats[1]["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(last["role"], "tool");
    assert!(chats[0]["tools"].as_array().unwrap().len() > 6);
}

#[cfg(windows)]
#[tokio::test]
async fn the_local_model_can_run_powershell() {
    let w = world(RouteMode::LocalOnly, vec![("m", TOOLS.to_vec())]).await;
    {
        let mut q = w.fake.replies.lock().unwrap();
        q.push_back(tool_call(
            "powershell",
            json!({"command": "Write-Output relay-local-ok"}),
        ));
        q.push_back(say("ok"));
    }
    w.agent.submit("say hi from the shell").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    let chats = w.fake.chats.lock().unwrap().clone();
    let tool_msg = chats[1]["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert!(tool_msg["content"]
        .as_str()
        .unwrap()
        .contains("relay-local-ok"));
}

#[tokio::test]
async fn escalation_hands_the_task_and_progress_to_the_cloud() {
    let w = world(RouteMode::Auto, vec![("m", TOOLS.to_vec())]).await;
    {
        let mut q = w.fake.replies.lock().unwrap();
        q.push_back(tool_call(
            "list_dir",
            json!({"path": std::env::temp_dir().to_string_lossy()}),
        ));
        q.push_back(tool_call(
            "escalate",
            json!({"reason": "I cannot read this PDF"}),
        ));
    }
    w.agent.submit("sort my downloads by type").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::Notice { text } if text.contains("I cannot read this PDF"))
    ));
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::Engine { kind, .. } if kind == "cloud")
    ));
    let seen = w.cloud.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].prompt.contains("sort my downloads by type"));
    assert!(seen[0].prompt.contains("I cannot read this PDF"));
    assert!(
        seen[0].prompt.contains("list_dir"),
        "summary of local steps is handed over"
    );
}

#[tokio::test]
async fn repeated_failures_escalate_automatically() {
    let w = world(RouteMode::Auto, vec![("m", TOOLS.to_vec())]).await;
    {
        let mut q = w.fake.replies.lock().unwrap();
        for _ in 0..3 {
            q.push_back(tool_call(
                "read_file",
                json!({"path": "C:\\definitely\\missing.txt"}),
            ));
        }
    }
    w.agent.submit("read that file").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert_eq!(w.cloud.seen.lock().unwrap().len(), 1);
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::Notice { text } if text.contains("kept failing"))
    ));
}

#[tokio::test]
async fn local_only_mode_does_not_use_the_cloud_even_when_asked_to_escalate() {
    let w = world(RouteMode::LocalOnly, vec![("m", TOOLS.to_vec())]).await;
    w.fake
        .replies
        .lock()
        .unwrap()
        .push_back(tool_call("escalate", json!({"reason": "too hard"})));
    w.agent.submit("do something hard").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Failed);
    assert!(w.cloud.seen.lock().unwrap().is_empty());
    assert!(has(
        &w,
        |e| matches!(e, UiEvent::Error { message, .. } if message.contains("too hard"))
    ));
}

#[tokio::test]
async fn web_tasks_and_missing_models_go_straight_to_the_cloud() {
    let w = world(RouteMode::Auto, vec![("m", TOOLS.to_vec())]).await;
    w.agent
        .submit("go to https://example.com and search for rust jobs")
        .unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert_eq!(w.cloud.seen.lock().unwrap().len(), 1);
    assert!(w.fake.chats.lock().unwrap().is_empty());

    let w = world(RouteMode::Auto, vec![]).await;
    w.agent.submit("list my files").unwrap();
    assert_eq!(wait_finished(&w).await, RunOutcome::Completed);
    assert_eq!(
        w.cloud.seen.lock().unwrap().len(),
        1,
        "no local model means cloud"
    );
}

#[tokio::test]
async fn stop_cancels_a_local_run_in_flight() {
    let w = world(RouteMode::LocalOnly, vec![("m", TOOLS.to_vec())]).await;
    *w.fake.hang.lock().unwrap() = true;
    w.agent.submit("do a thing").unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    w.agent.stop();
    assert_eq!(wait_finished(&w).await, RunOutcome::Cancelled);
}

#[tokio::test]
async fn conversation_history_reaches_the_local_model() {
    let w = world(RouteMode::LocalOnly, vec![("m", TOOLS.to_vec())]).await;
    w.fake
        .replies
        .lock()
        .unwrap()
        .push_back(say("first answer"));
    w.agent.submit("first question").unwrap();
    wait_finished(&w).await;
    w.events.lock().unwrap().clear();
    w.fake
        .replies
        .lock()
        .unwrap()
        .push_back(say("second answer"));
    w.agent.submit("second question").unwrap();
    wait_finished(&w).await;
    let chats = w.fake.chats.lock().unwrap().clone();
    let user = chats[1]["messages"][1]["content"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        user.contains("first question")
            && user.contains("first answer")
            && user.contains("second question")
    );
}

#[tokio::test]
async fn models_and_capabilities_are_discovered() {
    let w = world(
        RouteMode::Auto,
        vec![
            ("plain", TOOLS.to_vec()),
            ("seer", vec!["completion", "tools", "vision"]),
            ("chat-only", vec!["completion"]),
        ],
    )
    .await;
    let overview = w.agent.local().overview().await;
    assert!(overview.available);
    assert_eq!(overview.models.len(), 3);
    assert_eq!(overview.active.as_ref().unwrap().name, "seer");
    assert!(overview.ability().vision);
    w.agent.local().set_model(Some("plain".into()));
    assert_eq!(
        w.agent.local().overview().await.active.unwrap().name,
        "plain"
    );
    let status = w.agent.status().await;
    assert_eq!(status.local_model.as_deref(), Some("plain"));
    assert_eq!(status.route_mode, "auto");
}

#[tokio::test]
async fn model_downloads_report_progress() {
    let w = world(RouteMode::Auto, vec![]).await;
    let client = OllamaClient::new(w.agent.local().client().base_url());
    let mut seen: Vec<PullProgress> = Vec::new();
    client.pull("qwen3:14b", |p| seen.push(p)).await.unwrap();
    assert!(seen.iter().any(|p| p.percent() == Some(50)));
    assert_eq!(seen.last().unwrap().status, "success");
}

#[tokio::test]
async fn an_unreachable_server_is_reported_cleanly() {
    let client = OllamaClient::new("http://127.0.0.1:1");
    assert!(!client.available().await);
    assert!(client.models().await.is_err());
}
