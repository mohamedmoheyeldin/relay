#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! Integration tests: tool host + safety gate + MCP server against a fake desktop.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use relay_mcp::audit::AuditLog;
use relay_mcp::{
    ApprovalMode, ApprovalRequest, Approver, CallCtx, Decision, HostOptions, KillSwitch, McpServer,
    ToolHost, STOPPED_MESSAGE,
};
use relay_os::driver::{Driver, Timing};
use relay_os::testing::FakeDesktop;
use relay_os::InputEvent;
use relay_protocol::Risk;
use serde_json::{json, Value};

/// Approver that records requests and answers with a fixed decision.
struct Scripted {
    decision: Mutex<Decision>,
    asked: Mutex<Vec<ApprovalRequest>>,
}

impl Scripted {
    fn new(decision: Decision) -> Arc<Self> {
        Arc::new(Self {
            decision: Mutex::new(decision),
            asked: Mutex::new(Vec::new()),
        })
    }
    fn asked(&self) -> Vec<ApprovalRequest> {
        self.asked.lock().unwrap().clone()
    }
}

#[async_trait]
impl Approver for Scripted {
    async fn ask(&self, request: ApprovalRequest) -> Decision {
        self.asked.lock().unwrap().push(request);
        *self.decision.lock().unwrap()
    }
}

struct Rig {
    fake: Arc<FakeDesktop>,
    host: Arc<ToolHost>,
    approver: Arc<Scripted>,
    kill: KillSwitch,
}

fn big_display() -> FakeDesktop {
    FakeDesktop::with_displays(vec![relay_os::DisplayInfo {
        id: 1,
        rect: relay_os::Rect {
            x: 0,
            y: 0,
            w: 3136,
            h: 1764,
        },
        primary: true,
    }])
}

fn rig_with(decision: Decision, mode: ApprovalMode) -> Rig {
    let fake = Arc::new(big_display());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        fake.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let approver = Scripted::new(decision);
    let host = Arc::new(ToolHost::new(
        fake.clone(),
        driver,
        kill.clone(),
        approver.clone(),
        None,
        HostOptions {
            approval_mode: mode,
            change_verdicts: false,
            ..HostOptions::default()
        },
    ));
    Rig {
        fake,
        host,
        approver,
        kill,
    }
}

fn rig() -> Rig {
    rig_with(Decision::Approved, ApprovalMode::Destructive)
}

const CTX: CallCtx = CallCtx { run_id: 1 };

async fn call(rig: &Rig, name: &str, args: Value) -> relay_mcp::ToolOutput {
    rig.host.call(&CTX, name, &args).await
}

#[tokio::test]
async fn screenshot_returns_an_image_scaled_to_the_budget() {
    let r = rig();
    let out = call(&r, "screenshot", json!({})).await;
    assert!(!out.is_error);
    assert!(out.has_image());
    assert!(out.joined_text().contains("1568x882"));
}

#[tokio::test]
async fn click_converts_screenshot_pixels_to_physical_pixels() {
    let r = rig();
    let out = call(&r, "left_click", json!({"coordinate": [100, 100]})).await;
    assert!(!out.is_error, "{}", out.joined_text());
    assert_eq!(out.joined_text(), "OK");
    let events = r.fake.events();
    assert_eq!(events[0], InputEvent::MoveTo { x: 201, y: 201 });
    assert!(r.approver.asked().is_empty());
}

#[tokio::test]
async fn out_of_range_coordinates_send_nothing() {
    let r = rig();
    let out = call(&r, "left_click", json!({"coordinate": [5000, 5]})).await;
    assert!(out.is_error);
    assert!(out.joined_text().contains("outside"));
    assert!(r.fake.events().is_empty());
}

#[tokio::test]
async fn invalid_arguments_are_reported_without_side_effects() {
    let r = rig();
    for (name, args) in [
        ("left_click", json!({"coordinate": [1.5, 2]})),
        ("left_click", json!({"colour": 1})),
        ("key", json!({"keys": "a+b"})),
        ("nonsense", json!({})),
    ] {
        let out = call(&r, name, args).await;
        assert!(out.is_error, "{name}");
    }
    assert!(r.fake.events().is_empty());
}

#[tokio::test]
async fn destructive_flag_requires_approval_and_denial_sends_nothing() {
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    let out = call(
        &r,
        "left_click",
        json!({"coordinate": [10, 10], "destructive": true}),
    )
    .await;
    assert!(out.is_error);
    assert!(out.joined_text().contains("declined"));
    assert!(r.fake.events().is_empty());
    let asked = r.approver.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].risk, Risk::Destructive);
    assert_eq!(asked[0].run_id, 1);
}

#[tokio::test]
async fn approved_destructive_action_runs() {
    let r = rig();
    let out = call(&r, "key", json!({"keys": "Delete", "destructive": true})).await;
    assert!(!out.is_error);
    assert!(!r.fake.events().is_empty());
    assert_eq!(r.approver.asked().len(), 1);
}

#[tokio::test]
async fn timeout_is_reported_as_not_taken() {
    let r = rig_with(Decision::TimedOut, ApprovalMode::Destructive);
    let out = call(&r, "type", json!({"text": "hi", "destructive": true})).await;
    assert!(out.is_error);
    assert!(out.joined_text().contains("did not answer"));
    assert!(r.fake.events().is_empty());
}

#[tokio::test]
async fn dangerous_keys_ask_even_without_the_flag() {
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    let out = call(&r, "key", json!({"keys": "shift+Delete"})).await;
    assert!(out.is_error);
    assert_eq!(r.approver.asked().len(), 1);
}

#[tokio::test]
async fn risky_control_names_ask_before_clicking() {
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    r.fake.set_control_name(Some("Delete all messages"));
    let out = call(&r, "left_click", json!({"coordinate": [10, 10]})).await;
    assert!(out.is_error);
    assert!(r.approver.asked()[0].title.contains("Delete all messages"));
    assert!(r.fake.events().is_empty());

    r.fake.set_control_name(Some("Search"));
    let out = call(&r, "left_click", json!({"coordinate": [10, 10]})).await;
    assert!(!out.is_error);
}

#[tokio::test]
async fn always_mode_asks_for_acts_but_not_reads() {
    let r = rig_with(Decision::Approved, ApprovalMode::Always);
    call(&r, "screenshot", json!({})).await;
    call(&r, "cursor_position", json!({})).await;
    assert!(r.approver.asked().is_empty());
    call(&r, "left_click", json!({"coordinate": [1, 1]})).await;
    call(&r, "type", json!({"text": "a"})).await;
    assert_eq!(r.approver.asked().len(), 2);
}

#[tokio::test]
async fn off_mode_never_asks() {
    let r = rig_with(Decision::Denied, ApprovalMode::Off);
    let out = call(&r, "left_click", json!({"destructive": true})).await;
    assert!(!out.is_error);
    assert!(r.approver.asked().is_empty());
}

#[tokio::test]
async fn kill_switch_refuses_acts_but_allows_reads() {
    let r = rig();
    r.kill.trip();
    let out = call(&r, "left_click", json!({"coordinate": [1, 1]})).await;
    assert!(out.is_error);
    assert_eq!(out.joined_text(), STOPPED_MESSAGE);
    assert!(r.fake.events().is_empty());
    assert!(!call(&r, "screenshot", json!({})).await.is_error);
    r.kill.rearm();
    assert!(
        !call(&r, "left_click", json!({"coordinate": [1, 1]}))
            .await
            .is_error
    );
}

#[tokio::test]
async fn permission_prompt_allows_read_only_and_asks_for_destructive() {
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    let allow = call(
        &r,
        "approve",
        json!({"tool_name": "Read", "input": {"file_path": "a"}}),
    )
    .await;
    let v: Value = serde_json::from_str(&allow.joined_text()).unwrap();
    assert_eq!(v["behavior"], "allow");
    assert_eq!(v["updatedInput"]["file_path"], "a");
    assert!(r.approver.asked().is_empty());

    let deny = call(
        &r,
        "approve",
        json!({"tool_name": "Bash", "input": {"command": "rm -rf build"}}),
    )
    .await;
    let v: Value = serde_json::from_str(&deny.joined_text()).unwrap();
    assert_eq!(v["behavior"], "deny");
    assert!(v["message"].as_str().unwrap().contains("declined"));
    assert_eq!(r.approver.asked()[0].risk, Risk::Destructive);
}

#[tokio::test]
async fn permission_prompt_denies_everything_after_a_stop() {
    let r = rig();
    r.kill.trip();
    let out = call(&r, "approve", json!({"tool_name": "Read", "input": {}})).await;
    let v: Value = serde_json::from_str(&out.joined_text()).unwrap();
    assert_eq!(v["behavior"], "deny");
}

#[tokio::test]
async fn ask_user_returns_the_decision() {
    let r = rig();
    let out = call(
        &r,
        "ask_user",
        json!({"question": "Send the email?", "risk": "destructive"}),
    )
    .await;
    assert!(!out.is_error);
    assert_eq!(r.approver.asked()[0].risk, Risk::Destructive);
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    let out = call(&r, "ask_user", json!({"question": "Send?"})).await;
    assert!(out.is_error);
    assert!(call(&r, "ask_user", json!({})).await.is_error);
}

#[tokio::test]
async fn batch_runs_in_order_and_ends_with_a_screenshot() {
    let r = rig();
    let out = call(
        &r,
        "computer_batch",
        json!({"actions": [
            {"name": "mouse_move", "input": {"coordinate": [10, 10]}},
            {"name": "left_click", "input": {}},
            {"name": "type", "input": {"text": "a"}}
        ]}),
    )
    .await;
    assert!(!out.is_error, "{}", out.joined_text());
    assert!(out.has_image());
    assert!(matches!(r.fake.events()[0], InputEvent::MoveTo { .. }));
}

#[tokio::test]
async fn batch_halts_at_the_first_failure() {
    let r = rig();
    let out = call(
        &r,
        "computer_batch",
        json!({"actions": [
            {"name": "mouse_move", "input": {"coordinate": [10, 10]}},
            {"name": "left_click", "input": {"coordinate": [9999, 1]}},
            {"name": "type", "input": {"text": "should not be typed"}}
        ]}),
    )
    .await;
    assert!(out.is_error);
    let text = out.joined_text();
    assert!(text.contains("[2/3] left_click failed"));
    assert!(text.contains("[3/3] Not executed: an earlier computer action in this turn failed."));
    assert!(r
        .fake
        .events()
        .iter()
        .all(|e| !matches!(e, InputEvent::Unicode { .. } | InputEvent::Vk { .. })));
}

#[tokio::test]
async fn batch_rejects_malformed_and_nested_entries() {
    let r = rig();
    assert!(call(&r, "computer_batch", json!({})).await.is_error);
    assert!(
        call(&r, "computer_batch", json!({"actions": []}))
            .await
            .is_error
    );
    let out = call(&r, "computer_batch", json!({"actions": [{"input": {}}]})).await;
    assert!(out.is_error);
    let out = call(
        &r,
        "computer_batch",
        json!({"actions": [{"name": "computer_batch", "input": {}}]}),
    )
    .await;
    assert!(out.joined_text().contains("cannot be batched"));
}

#[tokio::test]
async fn batch_asks_for_each_destructive_step() {
    let r = rig_with(Decision::Denied, ApprovalMode::Destructive);
    let out = call(
        &r,
        "computer_batch",
        json!({"actions": [
            {"name": "left_click", "input": {"coordinate": [1, 1], "destructive": true}},
            {"name": "type", "input": {"text": "x"}}
        ]}),
    )
    .await;
    assert!(out.is_error);
    assert!(out.joined_text().contains("Not executed"));
    assert!(r.fake.events().is_empty());
}

#[tokio::test]
async fn windows_and_displays_are_listed() {
    let r = rig();
    r.fake.set_windows(vec![relay_os::WindowInfo {
        title: "Notepad".into(),
        rect: relay_os::Rect {
            x: 0,
            y: 0,
            w: 1000,
            h: 800,
        },
        foreground: true,
    }]);
    let out = call(&r, "list_windows", json!({})).await;
    assert!(out.joined_text().contains("* \"Notepad\""));
    let out = call(&r, "focus_window", json!({"title": "note"})).await;
    assert!(!out.is_error, "{}", out.joined_text());
    assert!(
        call(&r, "focus_window", json!({"title": "zzz"}))
            .await
            .is_error
    );
    let out = call(&r, "list_displays", json!({})).await;
    assert!(out.joined_text().contains("3136x1764"));
    assert!(
        call(&r, "select_display", json!({"display": 7}))
            .await
            .is_error
    );
    assert!(
        !call(&r, "select_display", json!({"display": 1}))
            .await
            .is_error
    );
}

#[tokio::test]
async fn change_verdicts_report_whether_the_screen_changed() {
    let fake = Arc::new(big_display());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        fake.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let host = ToolHost::new(
        fake.clone(),
        driver,
        kill,
        Scripted::new(Decision::Approved),
        None,
        HostOptions::default(),
    );
    let out = host
        .call(&CTX, "left_click", &json!({"coordinate": [5, 5]}))
        .await;
    assert_eq!(out.joined_text(), "OK · no visible change");

    // A background task changes the screen shortly after the click.
    let f2 = fake.clone();
    let flipper = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        f2.set_shade(220);
    });
    let out = host
        .call(&CTX, "left_click", &json!({"coordinate": [5, 5]}))
        .await;
    flipper.await.unwrap();
    assert_eq!(out.joined_text(), "OK · screen changed");
}

#[tokio::test]
async fn audit_log_records_calls() {
    let dir = std::env::temp_dir().join(format!("relay-mcp-audit-{}", std::process::id()));
    let fake = Arc::new(big_display());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        fake.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let host = ToolHost::new(
        fake,
        driver,
        kill,
        Scripted::new(Decision::Approved),
        Some(Arc::new(AuditLog::new(&dir))),
        HostOptions {
            change_verdicts: false,
            ..HostOptions::default()
        },
    );
    host.call(&CTX, "left_click", &json!({"coordinate": [5, 5]}))
        .await;
    host.call(&CTX, "bogus", &json!({})).await;
    let text = tokio::fs::read_to_string(dir.join("actions.jsonl"))
        .await
        .unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["tool"], "left_click");
    assert_eq!(lines[1]["error"], true);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// ---- MCP server over HTTP ----

async fn rpc(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    body: Value,
) -> reqwest::Response {
    let mut req = client.post(url).json(&body);
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    req.send().await.unwrap()
}

#[tokio::test]
async fn http_server_speaks_mcp() {
    let r = rig();
    let server = McpServer::start(r.host.clone()).await.unwrap();
    assert!(server.addr().ip().is_loopback());
    let grant = server.grant(42);
    let client = reqwest::Client::new();

    let init: Value = rpc(
        &client,
        &grant.url,
        Some(&grant.token),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "relay");

    let note = rpc(
        &client,
        &grant.url,
        Some(&grant.token),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    assert_eq!(note.status(), 202);

    let list: Value = rpc(
        &client,
        &grant.url,
        Some(&grant.token),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await
    .json()
    .await
    .unwrap();
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"screenshot") && names.contains(&"approve"));

    let shot: Value = rpc(
        &client,
        &grant.url,
        Some(&grant.token),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"screenshot","arguments":{}}}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(shot["result"]["isError"], false);
    let parts = shot["result"]["content"].as_array().unwrap();
    assert!(parts
        .iter()
        .any(|p| p["type"] == "image" && p["mimeType"] == "image/png"));

    let unknown: Value = rpc(
        &client,
        &grant.url,
        Some(&grant.token),
        json!({"jsonrpc":"2.0","id":4,"method":"nope"}),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(unknown["error"]["code"], -32601);
}

#[tokio::test]
async fn http_server_requires_a_valid_token_and_rejects_browsers() {
    let r = rig();
    let server = McpServer::start(r.host.clone()).await.unwrap();
    let grant = server.grant(1);
    let client = reqwest::Client::new();
    let ping = json!({"jsonrpc":"2.0","id":1,"method":"ping"});

    assert_eq!(
        rpc(&client, &grant.url, None, ping.clone()).await.status(),
        401
    );
    assert_eq!(
        rpc(&client, &grant.url, Some("wrong"), ping.clone())
            .await
            .status(),
        401
    );
    assert_eq!(
        rpc(&client, &grant.url, Some(&grant.token), ping.clone())
            .await
            .status(),
        200
    );

    let with_origin = client
        .post(&grant.url)
        .bearer_auth(&grant.token)
        .header("Origin", "http://evil.example")
        .json(&ping)
        .send()
        .await
        .unwrap();
    assert_eq!(with_origin.status(), 403);

    assert_eq!(
        client
            .get(&grant.url)
            .bearer_auth(&grant.token)
            .send()
            .await
            .unwrap()
            .status(),
        405
    );

    server.revoke(&grant);
    assert_eq!(
        rpc(&client, &grant.url, Some(&grant.token), ping)
            .await
            .status(),
        401
    );
}

#[tokio::test]
async fn grant_config_is_a_valid_http_server_entry() {
    let r = rig();
    let server = McpServer::start(r.host.clone()).await.unwrap();
    let grant = server.grant(9);
    let cfg = grant.server_config();
    assert_eq!(cfg["relay"]["type"], "http");
    assert!(cfg["relay"]["url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
    assert!(cfg["relay"]["headers"]["Authorization"]
        .as_str()
        .unwrap()
        .starts_with("Bearer "));
}

#[tokio::test]
async fn ask_user_does_not_prompt_when_approvals_are_off() {
    let r = rig_with(Decision::Denied, ApprovalMode::Off);
    let out = call(
        &r,
        "ask_user",
        json!({"question": "Delete everything?", "risk": "destructive"}),
    )
    .await;
    assert!(!out.is_error);
    assert!(out.joined_text().contains("switched off"));
    assert!(r.approver.asked().is_empty());
}
