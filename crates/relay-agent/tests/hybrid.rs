#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! The hybrid engine end to end: routing, planning (local and cloud), permissions, confirmations,
//! verification, recovery, privacy and cancellation, against mock providers and mock native services.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use relay_agent::browser::{BrowserDriver, BrowserOutput, BrowserStatus};
use relay_agent::hybrid::{
    HybridParts, HybridSettings, ProviderOpts, WorkflowRecord, WorkflowStore,
};
use relay_agent::{ConfirmBridge, EventSink, Hybrid, LocalConfig, LocalProvider, MemoryStore};
use relay_ai::{
    AiError, AiEvent, AiProvider, Finish, ProviderCaps, ProviderStatus, RunHandle, RunRequest,
};
use relay_mcp::{HostOptions, KillSwitch, ToolHost};
use relay_orchestrator::context::DesktopFacts;
use relay_orchestrator::permissions::{ConfirmLevel, PrivacyMode, SharePolicy};
use relay_orchestrator::plan::{UiTarget, Verification};
use relay_orchestrator::router::ProviderKind;
use relay_orchestrator::services::{ServiceError, ServiceResult, Services};
use relay_os::driver::{Driver, Timing};
use relay_os::testing::FakeDesktop;
use relay_protocol::{ConfirmAnswer, UiEvent};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// ---------- mock native services ----------

#[derive(Default)]
struct Svc {
    calls: Mutex<Vec<String>>,
    fail: Mutex<HashMap<String, ServiceError>>,
    truths: Mutex<HashSet<String>>,
    delay_ms: Mutex<u64>,
    facts: Mutex<DesktopFacts>,
}

impl Svc {
    fn log(&self, s: String) {
        self.calls.lock().unwrap().push(s);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    async fn step(&self, name: &str, desc: String) -> ServiceResult<String> {
        self.log(format!("{name} {desc}"));
        let d = *self.delay_ms.lock().unwrap();
        if d > 0 {
            tokio::time::sleep(Duration::from_millis(d)).await;
        }
        if let Some(e) = self.fail.lock().unwrap().get(name).cloned() {
            return Err(e);
        }
        Ok(format!("ok {name}"))
    }
}

#[async_trait]
impl Services for Svc {
    async fn open_app(&self, n: &str) -> ServiceResult<String> {
        let r = self.step("open_app", n.into()).await;
        if r.is_ok() {
            self.truths.lock().unwrap().insert(n.to_string());
        }
        r
    }
    async fn open_url(&self, u: &str) -> ServiceResult<String> {
        self.step("open_url", u.into()).await
    }
    async fn focus_window(&self, t: &str) -> ServiceResult<String> {
        self.step("focus", t.into()).await
    }
    async fn find_files(&self, q: &str, _r: Option<&str>, _m: usize) -> ServiceResult<Vec<String>> {
        self.step("find", q.into()).await?;
        Ok(vec![
            r"C:\Users\me\Downloads\old.zip".into(),
            r"C:\Users\me\Downloads\new.zip".into(),
        ])
    }
    async fn list_dir(&self, p: &str) -> ServiceResult<Vec<String>> {
        self.step("ls", p.into()).await?;
        Ok(vec!["a".into()])
    }
    async fn read_text(&self, p: &str, _m: usize) -> ServiceResult<String> {
        self.step("read", p.into()).await
    }
    async fn create_dir(&self, p: &str) -> ServiceResult<String> {
        self.step("mkdir", p.into()).await
    }
    async fn move_path(&self, f: &str, t: &str) -> ServiceResult<String> {
        self.step("mv", format!("{f}->{t}")).await
    }
    async fn write_text(&self, p: &str, _c: &str, _o: bool) -> ServiceResult<String> {
        self.step("write", p.into()).await
    }
    async fn move_to_trash(&self, p: &[String]) -> ServiceResult<String> {
        self.step("trash", p.join(",")).await
    }
    async fn ui_click(&self, t: &UiTarget) -> ServiceResult<String> {
        self.step("click", format!("{:?}", t.name)).await
    }
    async fn ui_set_text(&self, t: &UiTarget, text: &str) -> ServiceResult<String> {
        self.step("set", format!("{:?} {text}", t.name)).await
    }
    async fn type_text(&self, t: &str) -> ServiceResult<String> {
        self.step("type", t.into()).await
    }
    async fn press_keys(&self, k: &str) -> ServiceResult<String> {
        self.step("keys", k.into()).await
    }
    async fn check(&self, c: &Verification) -> ServiceResult<bool> {
        let key = match c {
            Verification::WindowExists { title } | Verification::WindowActive { title } => {
                title.clone()
            }
            Verification::FileExists { path } | Verification::FileMissing { path } => path.clone(),
            Verification::UiElementExists { target } => target.name.clone().unwrap_or_default(),
            Verification::UiTextContains { text, .. } => text.clone(),
        };
        let truthy = self.truths.lock().unwrap().contains(&key);
        Ok(if matches!(c, Verification::FileMissing { .. }) {
            !truthy
        } else {
            truthy
        })
    }
    async fn snapshot(&self) -> ServiceResult<DesktopFacts> {
        Ok(self.facts.lock().unwrap().clone())
    }
}

// ---------- scripted cloud provider ----------

struct Cloud {
    id: &'static str,
    replies: Mutex<VecDeque<Result<String, AiError>>>,
    seen: Mutex<Vec<RunRequest>>,
    signed_in: bool,
}

impl Cloud {
    fn new(id: &'static str, signed_in: bool) -> Arc<Self> {
        Arc::new(Self {
            id,
            replies: Mutex::default(),
            seen: Mutex::default(),
            signed_in,
        })
    }
    fn push_plan(&self, plan: Value) {
        self.replies.lock().unwrap().push_back(Ok(plan.to_string()));
    }
    fn push_err(&self, e: AiError) {
        self.replies.lock().unwrap().push_back(Err(e));
    }
    fn seen(&self) -> Vec<RunRequest> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl AiProvider for Cloud {
    fn name(&self) -> &str {
        self.id
    }
    fn capabilities(&self) -> ProviderCaps {
        ProviderCaps {
            id: self.id,
            label: self.id,
            structured_output: true,
            session_resume: false,
            mcp_tools: false,
            streaming_deltas: false,
            reasoning_levels: vec![],
            models: vec![],
        }
    }
    async fn status(&self) -> ProviderStatus {
        ProviderStatus {
            installed: true,
            logged_in: self.signed_in,
            detail: "x".into(),
            ..ProviderStatus::default()
        }
    }
    async fn run(&self, request: RunRequest) -> Result<RunHandle, AiError> {
        self.seen.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| {
            Err(AiError::Cli {
                message: "no scripted reply".into(),
                exit_code: None,
            })
        });
        let (tx, rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            let fin = tokio::select! {
                _ = c2.cancelled() => Finish::Cancelled,
                _ = async {} => match reply {
                    Ok(text) => Finish::Completed { session_id: None, result: text, cost_usd: None },
                    Err(e) => Finish::Failed(e),
                },
            };
            let _ = tx.send(AiEvent::Finished(fin)).await;
        });
        Ok(RunHandle::new(rx, cancel))
    }
}

// ---------- fake local model server ----------

#[derive(Default)]
struct Ollama {
    replies: Mutex<VecDeque<String>>,
    chats: Mutex<Vec<Value>>,
    models: Mutex<Vec<String>>,
}

type Shared = Arc<Ollama>;

async fn tags(State(s): State<Shared>) -> Json<Value> {
    let m: Vec<Value> = s
        .models
        .lock()
        .unwrap()
        .iter()
        .map(|n| json!({"name": n, "size": 1}))
        .collect();
    Json(json!({"models": m}))
}
async fn show() -> Json<Value> {
    Json(json!({"capabilities": ["completion", "tools"]}))
}
async fn chat(State(s): State<Shared>, Json(body): Json<Value>) -> String {
    let streaming = body["stream"].as_bool().unwrap_or(false);
    s.chats.lock().unwrap().push(body);
    let reply = s
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or_else(|| "not json".into());
    if streaming {
        // NDJSON: the reply in two pieces, then done.
        let mid = reply.len() / 2;
        let mid = (0..=mid)
            .rev()
            .find(|i| reply.is_char_boundary(*i))
            .unwrap_or(0);
        let mut out = String::new();
        for piece in [&reply[..mid], &reply[mid..]] {
            out.push_str(
                &json!({"message": {"role": "assistant", "content": piece}, "done": false})
                    .to_string(),
            );
            out.push('\n');
        }
        out.push_str(
            &json!({"message": {"role": "assistant", "content": ""}, "done": true}).to_string(),
        );
        out.push('\n');
        return out;
    }
    json!({"message": {"role": "assistant", "content": reply}}).to_string()
}

// ---------- scripted browser ----------

#[derive(Default)]
struct Browser {
    calls: Mutex<Vec<(String, Value)>>,
    url: Mutex<String>,
    fail_next: Mutex<u32>,
}

fn page(url: &str) -> String {
    format!(
        "### Page
- Page URL: {url}
- Page Title: Jobs
### Snapshot
```yaml
- generic [ref=e1]:
  - textbox \"Search jobs\" [ref=e10]
  - link \"Forward Deployed Engineer\" [ref=e11]
  - button \"Easy Apply\" [ref=e12]
  - textbox \"Password\" [ref=e14]
```"
    )
}

#[async_trait]
impl BrowserDriver for Browser {
    async fn status(&self) -> BrowserStatus {
        BrowserStatus {
            available: true,
            detail: "ok".into(),
            action_url: None,
            mode: "yours",
        }
    }
    async fn call(&self, tool: &str, args: Value, _t: Duration) -> Result<BrowserOutput, String> {
        self.calls
            .lock()
            .unwrap()
            .push((tool.to_string(), args.clone()));
        if tool == "browser_navigate" {
            *self.url.lock().unwrap() = args["url"].as_str().unwrap_or_default().to_string();
        }
        let mut f = self.fail_next.lock().unwrap();
        if *f > 0 {
            *f -= 1;
            return Ok(BrowserOutput {
                text: "### Error
Error: element not found"
                    .into(),
                is_error: true,
            });
        }
        let url = self.url.lock().unwrap().clone();
        Ok(BrowserOutput {
            text: page(if url.is_empty() { "about:blank" } else { &url }),
            is_error: false,
        })
    }
    async fn shutdown(&self) {}
}

impl Browser {
    fn tools(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| c.0.clone())
            .filter(|t| t != "browser_snapshot")
            .collect()
    }
}

async fn serve_ollama(o: Shared) -> String {
    let app = Router::new()
        .route("/api/tags", get(tags))
        .route("/api/show", post(show))
        .route("/api/chat", post(chat))
        .with_state(o);
    let l = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(l, app).await.unwrap();
    });
    url
}

// ---------- store + rig ----------

#[derive(Default)]
struct Records(Mutex<Vec<WorkflowRecord>>);
impl WorkflowStore for Records {
    fn save(&self, r: &WorkflowRecord) {
        self.0.lock().unwrap().push(r.clone());
    }
}

struct Rig {
    hybrid: Arc<Hybrid>,
    svc: Arc<Svc>,
    claude: Arc<Cloud>,
    codex: Arc<Cloud>,
    ollama: Shared,
    events: Arc<Mutex<Vec<UiEvent>>>,
    records: Arc<Records>,
    store: Arc<MemoryStore>,
    browser: Arc<Browser>,
}

fn allowed() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Users\me\Downloads"),
        PathBuf::from(r"C:\Users\me\Documents"),
    ]
}

struct Opts {
    privacy: PrivacyMode,
    local_models: Vec<&'static str>,
    claude_signed_in: bool,
    confirm: ConfirmLevel,
    share: SharePolicy,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            privacy: PrivacyMode::HybridAuto,
            local_models: vec!["qwen3:14b"],
            claude_signed_in: true,
            confirm: ConfirmLevel::Destructive,
            share: SharePolicy::default(),
        }
    }
}

async fn rig(o: Opts) -> Rig {
    let ollama: Shared = Arc::default();
    *ollama.models.lock().unwrap() = o.local_models.iter().map(|s| s.to_string()).collect();
    let url = serve_ollama(ollama.clone()).await;
    let events: Arc<Mutex<Vec<UiEvent>>> = Arc::default();
    let sink: EventSink = {
        let e = events.clone();
        Arc::new(move |ev| e.lock().unwrap().push(ev))
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
        HostOptions::default(),
    ));
    let local = LocalProvider::new(
        LocalConfig {
            base_url: url,
            model: None,
        },
        host,
    );
    let svc = Arc::new(Svc::default());
    svc.facts.lock().unwrap().window_titles = vec![
        "Budget.xlsx - Excel".into(),
        "Inbox - jo@example.com".into(),
    ];
    svc.facts.lock().unwrap().active_window = Some("Budget.xlsx - Excel".into());
    let claude = Cloud::new("claude", o.claude_signed_in);
    let codex = Cloud::new("codex", true);
    let store = Arc::new(MemoryStore::default());
    let records = Arc::new(Records::default());
    let browser = Arc::new(Browser::default());
    let mut provider_opts = HashMap::new();
    provider_opts.insert(
        ProviderKind::Codex,
        ProviderOpts {
            model: Some("gpt-x".into()),
            reasoning: Some("high".into()),
        },
    );
    let hybrid = Hybrid::new(HybridParts {
        services: svc.clone(),
        local,
        claude: claude.clone(),
        codex: codex.clone(),
        bridge,
        kill,
        conv_store: store.clone(),
        workflows: records.clone(),
        work_dir: std::env::temp_dir(),
        settings: HybridSettings {
            privacy: o.privacy,
            default_provider: ProviderKind::Claude,
            confirm: o.confirm,
            share: o.share,
            allowed_roots: allowed(),
            protected_roots: vec![],
            disabled_tools: HashSet::new(),
            provider_opts,
        },
        sink,
        browser: browser.clone(),
    });
    hybrid.set_exec_config(relay_orchestrator::ExecConfig {
        step_timeout: Duration::from_secs(2),
        verify_timeout: Duration::from_millis(150),
        verify_poll: Duration::from_millis(20),
        max_attempts: 2,
        retry_delay: Duration::from_millis(5),
    });
    Rig {
        hybrid,
        svc,
        claude,
        codex,
        ollama,
        events,
        records,
        store,
        browser,
    }
}

fn plan(steps: Vec<Value>) -> Value {
    json!({"objective": "o", "steps": steps})
}

fn step(id: &str, action: Value, verify: Option<Value>) -> Value {
    let mut s = json!({"id": id, "title": format!("Step {id}"), "action": action});
    if let Some(v) = verify {
        s["verify"] = v;
    }
    s
}

async fn finished(r: &Rig) -> relay_protocol::RunOutcome {
    for _ in 0..600 {
        if let Some(UiEvent::RunFinished { outcome, .. }) = r
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
    panic!("never finished: {:?}", r.events.lock().unwrap());
}

fn has(r: &Rig, f: impl Fn(&UiEvent) -> bool) -> bool {
    r.events.lock().unwrap().iter().any(f)
}

fn message(r: &Rig) -> String {
    r.events
        .lock()
        .unwrap()
        .iter()
        .find_map(|e| match e {
            UiEvent::AssistantMessage { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

const HEAVY: &str = "organize my documents folder and compare the duplicates in it";

// ---------- tests ----------

#[tokio::test]
async fn trivial_requests_run_natively_without_any_ai() {
    let r = rig(Opts::default()).await;
    r.hybrid.submit_in("c1", "open notepad", &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert_eq!(r.svc.calls(), vec!["open_app notepad"]);
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::Route { strategy, .. } if strategy == "native")
    ));
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::PlanProposed { source, .. } if source == "native")
    ));
    assert!(r.claude.seen().is_empty() && r.codex.seen().is_empty());
    assert!(r.ollama.chats.lock().unwrap().is_empty());
    assert!(message(&r).contains("can't check"), "{}", message(&r));
    let rec = r.records.0.lock().unwrap()[0].clone();
    assert_eq!(
        (rec.strategy.as_str(), rec.outcome.as_str(), rec.steps_total),
        ("native", "completed", 1)
    );
}

#[tokio::test]
async fn the_local_model_plans_routine_tasks_and_results_are_reported() {
    let r = rig(Opts::default()).await;
    r.ollama.replies.lock().unwrap().push_back(
        plan(vec![step(
            "s1",
            json!({"tool": "find_files", "query": "*.zip", "root": r"C:\Users\me\Downloads"}),
            None,
        )])
        .to_string(),
    );
    r.hybrid
        .submit_in("c1", "find the zip files in my downloads", &[])
        .unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::PlanProposed { source, .. } if source == "local")
    ));
    assert!(message(&r).contains("old.zip") && message(&r).contains("new.zip"));
    assert!(
        r.claude.seen().is_empty(),
        "no cloud request for a routine task"
    );
    // The local request carried the schema and the desktop picture.
    let chat = r.ollama.chats.lock().unwrap()[0].clone();
    assert!(chat["format"].is_object());
    assert!(chat["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("Budget.xlsx"));
}

#[tokio::test]
async fn heavy_tasks_go_to_the_cloud_planner_with_minimal_redacted_context() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "open_app", "name": "Notepad"}),
        Some(json!({"check": "window_exists", "title": "Notepad"})),
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    let req = &r.claude.seen()[0];
    assert!(
        req.plan_only && req.output_schema.is_some(),
        "plans only, structured"
    );
    assert!(
        req.allowed_tools.is_empty()
            && req.permission_prompt_tool.is_none()
            && req.mcp_servers == json!({})
    );
    assert!(req.prompt.contains("Budget.xlsx - Excel"));
    assert!(
        req.prompt.contains("[email]") && !req.prompt.contains("jo@example.com"),
        "redacted: {}",
        req.prompt
    );
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::ContextShared { provider, categories, .. }
        if provider == "claude" && categories.iter().any(|c| c.category == "window_titles"))
    ));
    assert!(
        message(&r).contains("checked afterwards"),
        "{}",
        message(&r)
    );
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::StepFinished { status, .. } if status == "verified")
    ));
}

#[tokio::test]
async fn local_only_mode_never_reaches_the_cloud() {
    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    r.ollama.replies.lock().unwrap().push_back(
        plan(vec![step(
            "s1",
            json!({"tool": "list_dir", "path": r"C:\Users\me\Documents"}),
            None,
        )])
        .to_string(),
    );
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(r.claude.seen().is_empty() && r.codex.seen().is_empty());
    assert!(!has(&r, |e| matches!(e, UiEvent::ContextShared { .. })));

    let none = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        local_models: vec![],
        ..Opts::default()
    })
    .await;
    none.hybrid
        .submit_in("c1", "tidy my documents folder", &[])
        .unwrap();
    finished(&none).await;
    assert!(has(
        &none,
        |e| matches!(e, UiEvent::Route { strategy, .. } if strategy == "unavailable")
    ));
    assert!(has(
        &none,
        |e| matches!(e, UiEvent::Error { message, .. } if message.contains("Local only"))
    ));
    assert!(none.claude.seen().is_empty());
}

#[tokio::test]
async fn sharing_switches_control_exactly_what_leaves_the_pc() {
    let r = rig(Opts {
        share: SharePolicy {
            window_titles: false,
            ..SharePolicy::default()
        },
        ..Opts::default()
    })
    .await;
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "press_keys", "keys": "ctrl+s"}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    finished(&r).await;
    let req = &r.claude.seen()[0];
    assert!(!req.prompt.contains("Budget") && !req.prompt.contains("Excel"));
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::ContextShared { categories, .. } if categories.is_empty())
    ));
}

#[tokio::test]
async fn a_failing_provider_falls_back_to_the_other_one() {
    let r = rig(Opts::default()).await;
    r.claude.push_err(AiError::NotLoggedIn("expired".into()));
    r.codex.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "open_app", "name": "Notepad"}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::Notice { text } if text.contains("Trying Codex"))
    ));
    let req = &r.codex.seen()[0];
    assert_eq!(
        (req.model.as_deref(), req.reasoning.as_deref()),
        (Some("gpt-x"), Some("high")),
        "per-provider options apply"
    );
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::PlanProposed { source, .. } if source == "codex")
    ));
}

#[tokio::test]
async fn unreadable_or_unsafe_plans_are_rejected_and_nothing_runs() {
    let r = rig(Opts::default()).await;
    let bad = json!({"objective": "o", "steps": [{"id": "s1", "title": "x", "action": {"tool": "run_shell", "command": "del /s C:\\"}}]});
    r.claude.push_plan(bad.clone());
    r.claude.push_plan(bad);
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    finished(&r).await;
    assert!(r.svc.calls().is_empty());
    assert_eq!(
        r.claude.seen().len(),
        2,
        "one retry with the rejection reason"
    );
    assert!(r.claude.seen()[1].prompt.contains("rejected"));
    assert!(has(&r, |e| matches!(e, UiEvent::Error { .. })));
}

#[tokio::test]
async fn permission_policy_refuses_steps_outside_the_allowed_folders() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(plan(vec![
        step(
            "s1",
            json!({"tool": "move_to_trash", "paths": [r"C:\Windows\System32\drivers\etc\hosts"]}),
            None,
        ),
        step("s2", json!({"tool": "type_text", "text": "after"}), None),
    ]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Failed);
    assert!(
        r.svc.calls().is_empty(),
        "refused actions never reach the native layer"
    );
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::StepFinished { status, summary, .. } if status == "failed" && summary.starts_with("Refused"))
    ));
    assert!(message(&r).contains("refused"));
    assert_eq!(r.records.0.lock().unwrap()[0].outcome, "denied");
}

#[tokio::test]
async fn destructive_steps_wait_for_the_owner_and_declining_stops_the_run() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "move_to_trash", "paths": [r"C:\Users\me\Downloads\old.zip"]}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    let card = loop {
        if let Some(UiEvent::Confirm(c)) = r
            .events
            .lock()
            .unwrap()
            .iter()
            .find(|e| matches!(e, UiEvent::Confirm(_)))
            .cloned()
        {
            break c;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(card.risk, relay_protocol::Risk::Destructive);
    assert!(r.svc.calls().is_empty(), "nothing happens before approval");
    assert!(r.hybrid.answer(ConfirmAnswer {
        id: card.id,
        approved: false
    }));
    finished(&r).await;
    assert!(r.svc.calls().is_empty());
    assert_eq!(r.records.0.lock().unwrap()[0].outcome, "declined");
    assert!(message(&r).contains("declined"));
}

#[tokio::test]
async fn approving_lets_the_step_run_and_is_recorded() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "move_to_trash", "paths": [r"C:\Users\me\Downloads\old.zip"]}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    loop {
        if let Some(UiEvent::Confirm(c)) = r
            .events
            .lock()
            .unwrap()
            .iter()
            .find(|e| matches!(e, UiEvent::Confirm(_)))
            .cloned()
        {
            r.hybrid.answer(ConfirmAnswer {
                id: c.id,
                approved: true,
            });
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert_eq!(r.svc.calls(), vec![r"trash C:\Users\me\Downloads\old.zip"]);
    assert_eq!(r.records.0.lock().unwrap()[0].approvals.len(), 1);
}

#[tokio::test]
async fn a_step_that_runs_but_cannot_be_verified_is_not_reported_as_success() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "focus_window", "title": "Notepad"}),
        Some(json!({"check": "window_active", "title": "Notepad"})),
    )]));
    // Recovery plans fail the same way.
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "focus_window", "title": "Notepad"}),
        Some(json!({"check": "window_active", "title": "Notepad"})),
    )]));
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "focus_window", "title": "Notepad"}),
        Some(json!({"check": "window_active", "title": "Notepad"})),
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Failed);
    assert!(
        message(&r).contains("could not be confirmed"),
        "{}",
        message(&r)
    );
    assert!(!message(&r).contains("Verified"));
    assert_eq!(r.records.0.lock().unwrap()[0].steps_verified, 0);
}

#[tokio::test]
async fn failures_are_repaired_by_asking_for_a_corrected_plan() {
    let r = rig(Opts::default()).await;
    r.svc.fail.lock().unwrap().insert(
        "open_app".into(),
        ServiceError::Failed("no such app".into()),
    );
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "open_app", "name": "Nonexistent"}),
        None,
    )]));
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "open_url", "url": "https://example.com"}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::Notice { text } if text.contains("corrected plan"))
    ));
    let second = &r.claude.seen()[1];
    assert!(second.prompt.contains("no such app") && second.prompt.contains("NEW plan"));
    assert!(r
        .svc
        .calls()
        .contains(&"open_url https://example.com".to_string()));
}

#[tokio::test]
async fn local_failures_escalate_to_the_cloud_when_allowed() {
    let r = rig(Opts::default()).await;
    r.svc.fail.lock().unwrap().insert(
        "click".into(),
        ServiceError::Failed("no such button".into()),
    );
    r.ollama.replies.lock().unwrap().push_back(
        plan(vec![step(
            "s1",
            json!({"tool": "ui_click", "target": {"name": "Save"}}),
            None,
        )])
        .to_string(),
    );
    r.claude.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "press_keys", "keys": "ctrl+s"}),
        None,
    )]));
    r.hybrid
        .submit_in("c1", "save the document in the current window", &[])
        .unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::PlanProposed { source, .. } if source == "local")
    ));
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::PlanProposed { source, .. } if source == "claude")
    ));
    assert!(r.svc.calls().contains(&"keys ctrl+s".to_string()));
}

#[tokio::test]
async fn an_answer_alone_is_delivered_without_running_anything() {
    let r = rig(Opts::default()).await;
    r.claude.push_plan(json!({"objective": "explain", "steps": [], "answer": "Use `Get-ChildItem` to list files."}));
    r.hybrid
        .submit_in(
            "c1",
            "explain how to list files in powershell and write a script to do it",
            &[],
        )
        .unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(message(&r).contains("Get-ChildItem"));
    assert!(r.svc.calls().is_empty());
}

#[tokio::test]
async fn stopping_cancels_a_running_step_and_the_task() {
    let r = rig(Opts::default()).await;
    *r.svc.delay_ms.lock().unwrap() = 5_000;
    r.claude.push_plan(plan(vec![
        step("s1", json!({"tool": "open_app", "name": "Slow"}), None),
        step("s2", json!({"tool": "type_text", "text": "x"}), None),
    ]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    for _ in 0..200 {
        if !r.svc.calls().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    r.hybrid.stop();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Cancelled);
    assert_eq!(r.svc.calls().len(), 1, "the second step never ran");
    assert!(!r.hybrid.is_busy());
}

#[tokio::test]
async fn one_task_at_a_time_and_history_is_remembered_per_conversation() {
    let r = rig(Opts::default()).await;
    *r.svc.delay_ms.lock().unwrap() = 300;
    r.hybrid.submit_in("c1", "open notepad", &[]).unwrap();
    assert!(r.hybrid.submit_in("c1", "open calc", &[]).is_err());
    assert_eq!(
        r.hybrid.submit_in("c1", "   ", &[]).unwrap_err(),
        relay_agent::hybrid::SubmitError::Empty
    );
    finished(&r).await;
    use relay_agent::ConvStore;
    let ctx = r.store.load("c1");
    assert_eq!(ctx.history.len(), 1);
    assert_eq!(ctx.history[0].0, "open notepad");
    assert!(r.store.load("other").history.is_empty());
}

#[tokio::test]
async fn provider_status_is_cached_and_can_be_forced() {
    let r = rig(Opts {
        claude_signed_in: false,
        ..Opts::default()
    })
    .await;
    let st = r.hybrid.provider_status(ProviderKind::Claude, false).await;
    assert!(st.installed && !st.logged_in);
    // With Claude signed out, heavy work goes to the other signed-in provider.
    r.codex.push_plan(plan(vec![step(
        "s1",
        json!({"tool": "press_keys", "keys": "ctrl+s"}),
        None,
    )]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    finished(&r).await;
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::Route { provider: Some(p), .. } if p == "codex")
    ));
}

// ---------- questions, websites, repeats ----------

fn events_of(r: &Rig) -> Vec<UiEvent> {
    r.events.lock().unwrap().clone()
}

fn used_labels(r: &Rig) -> Vec<(String, String)> {
    events_of(r)
        .into_iter()
        .filter_map(|e| match e {
            UiEvent::Used { kind, label, .. } => Some((kind, label)),
            _ => None,
        })
        .collect()
}

async fn answer_confirm(r: &Rig, approve: bool) -> relay_protocol::ConfirmRequest {
    for _ in 0..300 {
        if let Some(UiEvent::Confirm(c)) = events_of(r)
            .into_iter()
            .find(|e| matches!(e, UiEvent::Confirm(_)))
        {
            r.hybrid.answer(ConfirmAnswer {
                id: c.id,
                approved: approve,
            });
            return c;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no confirmation card: {:?}", events_of(r));
}

#[tokio::test]
async fn a_coding_question_is_answered_in_words_and_nothing_runs() {
    let r = rig(Opts::default()).await;
    r.claude
        .replies
        .lock()
        .unwrap()
        .push_back(Ok("```python\nprint(\"hi\")\n```".into()));
    r.hybrid
        .submit_in(
            "c1",
            "example of playwright code for testing amazon search?",
            &[],
        )
        .unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert!(message(&r).contains("```python"), "{}", message(&r));
    assert!(r.svc.calls().is_empty(), "a question never runs anything");
    assert!(
        r.browser.calls.lock().unwrap().is_empty(),
        "and never opens pages"
    );
    assert!(
        !has(&r, |e| matches!(e, UiEvent::PlanProposed { .. })),
        "no task card for a question"
    );
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::Route { intent: Some(i), .. } if i == "chat")
    ));
    let req = &r.claude.seen()[0];
    assert!(req.plan_only && req.output_schema.is_none() && req.allowed_tools.is_empty());
    assert_eq!(
        req.model.as_deref(),
        Some("sonnet"),
        "a sensible default model when the owner chose none"
    );
    assert!(used_labels(&r)
        .iter()
        .any(|(k, l)| k == "cloud_ai" && l == "Claude Code"));
}

#[tokio::test]
async fn local_answers_stream_and_stay_on_the_pc() {
    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    r.ollama
        .replies
        .lock()
        .unwrap()
        .push_back("A mutex lets one thread in at a time.".into());
    r.hybrid.submit_in("c1", "what is a mutex?", &[]).unwrap();
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    let deltas: String = events_of(&r)
        .into_iter()
        .filter_map(|e| match e {
            UiEvent::AssistantDelta { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, "A mutex lets one thread in at a time.");
    assert!(r.claude.seen().is_empty() && r.codex.seen().is_empty());
    assert!(used_labels(&r)
        .iter()
        .any(|(k, l)| k == "local_ai" && l == "Qwen3 14B"));
}

#[tokio::test]
async fn a_website_task_runs_step_by_step_and_asks_before_applying() {
    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    for reply in [
        json!({"action": "navigate", "title": "Open Dice", "url": "https://www.dice.com/jobs?q=fde&easy=1"}),
        json!({"action": "click", "title": "Open the job", "target": "e11", "element": "job link"}),
        json!({"action": "click", "title": "Easy Apply", "target": "e12", "element": "Easy Apply button"}),
        json!({"action": "done", "title": "Finished", "answer": "Applied to Forward Deployed Engineer."}),
    ] {
        r.ollama
            .replies
            .lock()
            .unwrap()
            .push_back(reply.to_string());
    }
    r.hybrid
        .submit_in(
            "c1",
            "go to https://www.dice.com/ and apply for fde jobs with easy apply",
            &[],
        )
        .unwrap();
    let card = answer_confirm(&r, true).await;
    assert_eq!(card.risk, relay_protocol::Risk::Destructive);
    assert_eq!(finished(&r).await, relay_protocol::RunOutcome::Completed);
    assert_eq!(
        r.browser.tools(),
        ["browser_navigate", "browser_click", "browser_click"],
        "one page, no extra tabs"
    );
    assert!(message(&r).contains("Applied to Forward Deployed Engineer"));
    assert!(has(&r, |e| matches!(e, UiEvent::StepAdded { step, .. }
        if step.via.as_deref() == Some("mcp") && step.server.as_deref() == Some("playwright"))));
    assert!(
        has(
            &r,
            |e| matches!(e, UiEvent::StepFinished { status, .. } if status == "verified")
        ),
        "the navigation is checked"
    );
    let used = used_labels(&r);
    assert!(used.iter().any(|(k, _)| k == "mcp") && used.iter().any(|(k, _)| k == "local_ai"));
    assert!(
        r.svc.calls().is_empty(),
        "no desktop tools for a website task"
    );
    let rec = r.records.0.lock().unwrap()[0].clone();
    assert_eq!((rec.strategy.as_str(), rec.steps.len()), ("web_local", 3));
}

#[tokio::test]
async fn declining_an_application_submits_nothing() {
    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    r.ollama
        .replies
        .lock()
        .unwrap()
        .push_back(json!({"action": "click", "title": "Easy Apply", "target": "e12"}).to_string());
    r.hybrid
        .submit_in("c1", "apply for the fde job on dice", &[])
        .unwrap();
    answer_confirm(&r, false).await;
    finished(&r).await;
    assert!(r.browser.tools().is_empty(), "the click never happened");
    assert!(
        message(&r).contains("Nothing was submitted"),
        "{}",
        message(&r)
    );
}

#[tokio::test]
async fn private_fields_are_never_typed_and_loops_are_stopped() {
    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    r.ollama.replies.lock().unwrap().push_back(
        json!({"action": "type", "title": "Sign in", "target": "e14", "text": "hunter2"})
            .to_string(),
    );
    r.hybrid
        .submit_in("c1", "sign in to dice.com for me", &[])
        .unwrap();
    finished(&r).await;
    assert!(r.browser.tools().is_empty());
    assert!(message(&r).contains("won't type"), "{}", message(&r));
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::StepAdded { step, .. } if step.sensitive_data)
    ));

    let r = rig(Opts {
        privacy: PrivacyMode::LocalOnly,
        ..Opts::default()
    })
    .await;
    for _ in 0..5 {
        r.ollama.replies.lock().unwrap().push_back(
            json!({"action": "click", "title": "Open the job", "target": "e11"}).to_string(),
        );
    }
    r.hybrid
        .submit_in("c1", "open the fde job on dice.com", &[])
        .unwrap();
    finished(&r).await;
    assert_eq!(
        r.browser.tools().len(),
        2,
        "the third identical click is not made"
    );
    assert!(message(&r).contains("nothing changed"), "{}", message(&r));
}

#[tokio::test]
async fn a_cloud_driven_website_task_shares_redacted_pages_only_when_allowed() {
    let r = rig(Opts {
        local_models: vec![],
        ..Opts::default()
    })
    .await;
    r.claude
        .push_plan(json!({"action": "done", "title": "Done", "answer": "Nothing to do."}));
    r.hybrid
        .submit_in("c1", "go to dice.com and look at fde jobs", &[])
        .unwrap();
    finished(&r).await;
    assert!(has(
        &r,
        |e| matches!(e, UiEvent::ContextShared { categories, .. }
        if categories.iter().any(|c| c.category == "web_pages"))
    ));
    assert_eq!(
        r.claude.seen()[0].model.as_deref(),
        Some("sonnet"),
        "browser steps use a capable model at low effort"
    );

    let off = rig(Opts {
        local_models: vec![],
        share: SharePolicy {
            web_pages: false,
            ..SharePolicy::default()
        },
        ..Opts::default()
    })
    .await;
    off.hybrid
        .submit_in("c1", "go to dice.com and look at fde jobs", &[])
        .unwrap();
    finished(&off).await;
    assert!(
        off.claude.seen().is_empty(),
        "no page leaves the PC when sharing pages is off"
    );
    assert!(message(&off).contains("switched off"), "{}", message(&off));
}

#[tokio::test]
async fn a_corrected_plan_does_not_reopen_pages_that_are_already_open() {
    let r = rig(Opts::default()).await;
    r.svc
        .fail
        .lock()
        .unwrap()
        .insert("keys".into(), ServiceError::Failed("no window".into()));
    r.claude.push_plan(plan(vec![
        step(
            "s1",
            json!({"tool": "open_url", "url": "https://example.com/a"}),
            None,
        ),
        step("s2", json!({"tool": "press_keys", "keys": "ctrl+s"}), None),
    ]));
    r.claude.push_plan(plan(vec![
        step(
            "s1",
            json!({"tool": "open_url", "url": "https://example.com/a"}),
            None,
        ),
        step("s2", json!({"tool": "open_app", "name": "Notepad"}), None),
    ]));
    r.hybrid.submit_in("c1", HEAVY, &[]).unwrap();
    finished(&r).await;
    let opens = r
        .svc
        .calls()
        .iter()
        .filter(|c| c.starts_with("open_url"))
        .count();
    assert_eq!(opens, 1, "{:?}", r.svc.calls());
    assert!(r.svc.calls().iter().any(|c| c.starts_with("open_app")));
}
