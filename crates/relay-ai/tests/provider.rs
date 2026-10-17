#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! Integration tests for the Claude provider against a fake CLI process.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use relay_ai::claude::ClaudeProvider;
use relay_ai::{
    AiError, AiEvent, AiProvider, Finish, RunHandle, RunRequest, SessionMode, TimeoutKind, Timeouts,
};

fn fake() -> ClaudeProvider {
    ClaudeProvider::with_program(env!("CARGO_BIN_EXE_fake_cli"))
}

fn request(prompt: &str) -> RunRequest {
    RunRequest {
        run_id: 1,
        prompt: prompt.to_string(),
        session: SessionMode::New("11111111-1111-4111-8111-111111111111".into()),
        work_dir: std::env::temp_dir(),
        mcp_servers: serde_json::json!({"relay": {"type": "http", "url": "http://127.0.0.1:1/mcp"}}),
        permission_prompt_tool: Some("mcp__relay__approve".into()),
        allowed_tools: vec!["mcp__relay".into()],
        disallowed_tools: vec!["AskUserQuestion".into()],
        reasoning: None,
        output_schema: None,
        plan_only: false,
        system_prompt_append: Some("be careful".into()),
        model: None,
        timeouts: Timeouts {
            total: Duration::from_secs(20),
            idle: Duration::from_secs(10),
        },
    }
}

async fn collect(mut handle: RunHandle) -> Vec<AiEvent> {
    let mut out = Vec::new();
    while let Some(ev) = handle.events.recv().await {
        out.push(ev);
    }
    out
}

fn finish_of(events: &[AiEvent]) -> &Finish {
    match events.last() {
        Some(AiEvent::Finished(f)) => f,
        other => panic!("stream did not end with Finished: {other:?}"),
    }
}

#[tokio::test]
async fn streams_events_and_completes() {
    let handle = fake()
        .run(request("SCENARIO:echo\nhi there"))
        .await
        .unwrap();
    let events = collect(handle).await;
    assert!(
        matches!(&events[0], AiEvent::Started { session_id, .. } if session_id == "fake-session")
    );
    assert!(events.contains(&AiEvent::TextDelta("he".into())));
    // The prompt reaches the process through stdin.
    assert!(events.contains(&AiEvent::Text("hello:hi there".into())));
    assert!(events
        .iter()
        .any(|e| matches!(e, AiEvent::ToolUse { name, .. } if name == "mcp__relay__screenshot")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AiEvent::ToolResult { content, .. } if content == "OK")));
    assert!(matches!(
        finish_of(&events),
        Finish::Completed { result, .. } if result == "all done"
    ));
}

#[tokio::test]
async fn command_line_carries_the_expected_flags() {
    let handle = fake().run(request("SCENARIO:args")).await.unwrap();
    let events = collect(handle).await;
    let line = events
        .iter()
        .find_map(|e| match e {
            AiEvent::Text(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap();
    for needle in [
        "stream-json",
        "--strict-mcp-config",
        "--permission-prompt-tool|mcp__relay__approve",
        "--allowedTools|mcp__relay",
        "--session-id|11111111-1111-4111-8111-111111111111",
        "--append-system-prompt-file",
    ] {
        assert!(line.contains(needle), "{needle} missing from {line}");
    }
    assert!(!line.contains("SCENARIO"), "prompt leaked to argv");
}

#[tokio::test]
async fn cancel_stops_a_hung_process_quickly() {
    let handle = fake().run(request("SCENARIO:hang")).await.unwrap();
    let cancel = handle.canceller();
    let started = Instant::now();
    let task = tokio::spawn(collect(handle));
    tokio::time::sleep(Duration::from_millis(400)).await;
    cancel.cancel();
    let events = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finish_of(&events), &Finish::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(8));
}

#[tokio::test]
async fn idle_timeout_fires_when_the_process_goes_quiet() {
    let mut req = request("SCENARIO:hang");
    req.timeouts = Timeouts {
        total: Duration::from_secs(30),
        idle: Duration::from_millis(500),
    };
    let events = collect(fake().run(req).await.unwrap()).await;
    assert_eq!(finish_of(&events), &Finish::TimedOut(TimeoutKind::Idle));
}

#[tokio::test]
async fn total_timeout_fires_even_when_output_keeps_flowing() {
    let mut req = request("SCENARIO:chatty");
    req.timeouts = Timeouts {
        total: Duration::from_millis(400),
        idle: Duration::from_secs(10),
    };
    let events = collect(fake().run(req).await.unwrap()).await;
    assert_eq!(finish_of(&events), &Finish::TimedOut(TimeoutKind::Total));
}

#[tokio::test]
async fn steady_output_does_not_trip_the_idle_timeout() {
    let mut req = request("SCENARIO:chatty");
    req.timeouts = Timeouts {
        total: Duration::from_secs(30),
        idle: Duration::from_millis(900),
    };
    let events = collect(fake().run(req).await.unwrap()).await;
    assert!(matches!(finish_of(&events), Finish::Completed { .. }));
}

#[tokio::test]
async fn missing_cli_is_reported_as_not_installed() {
    let provider = ClaudeProvider::with_program(PathBuf::from("definitely-not-a-real-cli-xyz"));
    let err = provider.run(request("hi")).await.err().unwrap();
    assert!(matches!(err, AiError::NotInstalled { .. }), "{err:?}");
    let status = provider.status().await;
    assert!(!status.installed);
    assert!(!status.ready());
}

#[tokio::test]
async fn login_failure_is_classified() {
    let events = collect(fake().run(request("SCENARIO:notlogged")).await.unwrap()).await;
    assert!(matches!(
        finish_of(&events),
        Finish::Failed(AiError::NotLoggedIn(_))
    ));
}

#[tokio::test]
async fn crash_surfaces_stderr_and_exit_code() {
    let events = collect(fake().run(request("SCENARIO:crash")).await.unwrap()).await;
    match finish_of(&events) {
        Finish::Failed(AiError::Cli { message, exit_code }) => {
            assert!(message.contains("kaboom"));
            assert_eq!(*exit_code, Some(3));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn clean_exit_without_a_result_still_finishes() {
    let events = collect(fake().run(request("SCENARIO:noresult")).await.unwrap()).await;
    assert!(matches!(
        finish_of(&events),
        Finish::Completed { result, .. } if result == "partial answer"
    ));
}

#[tokio::test]
async fn status_reads_login_state() {
    let status = fake().status().await;
    assert!(status.installed);
    assert!(status.logged_in);
    assert!(status.ready());
}

#[tokio::test]
async fn cancel_kills_grandchildren_too() {
    let handle = fake().run(request("SCENARIO:tree")).await.unwrap();
    let cancel = handle.canceller();
    let mut handle = handle;
    let mut child_pid = None;
    while let Some(ev) = handle.events.recv().await {
        if let AiEvent::Text(t) = ev {
            child_pid = t.strip_prefix("child:").and_then(|p| p.parse::<u32>().ok());
            break;
        }
    }
    let pid = child_pid.expect("fake CLI reports its child pid");
    assert!(pid_alive(pid), "grandchild should be running before cancel");
    cancel.cancel();
    while handle.events.recv().await.is_some() {}
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!pid_alive(pid), "grandchild must die with the tree");
}

fn pid_alive(pid: u32) -> bool {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist runs");
        String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
    } else {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

// ---- Codex provider ----

use relay_ai::codex::CodexProvider;

fn codex() -> CodexProvider {
    CodexProvider::with_program(env!("CARGO_BIN_EXE_fake_cli"))
}

#[tokio::test]
async fn codex_streams_items_and_completes() {
    let handle = codex()
        .run(request("SCENARIO:echo\nhello codex"))
        .await
        .unwrap();
    let events = collect(handle).await;
    assert!(
        matches!(&events[0], AiEvent::Started { session_id, .. } if session_id == "codex-thread-1")
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, AiEvent::ToolUse { name, .. } if name == "command")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AiEvent::ToolResult { content, .. } if content == "a.txt\n")));
    assert!(matches!(
        finish_of(&events),
        Finish::Completed { result, session_id: Some(s), .. } if result.starts_with("codex:") && s == "codex-thread-1"
    ));
}

#[tokio::test]
async fn codex_command_line_is_sandboxed_and_keeps_the_prompt_off_argv() {
    let mut req = request("SCENARIO:args");
    req.plan_only = true;
    req.reasoning = Some("high".into());
    req.output_schema = Some(serde_json::json!({"type":"object"}));
    let events = collect(codex().run(req).await.unwrap()).await;
    let line = events
        .iter()
        .find_map(|e| match e {
            AiEvent::Text(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap();
    for needle in [
        "exec",
        "--json",
        "--sandbox|read-only",
        "--ephemeral",
        "--ignore-user-config",
        "--output-schema",
        r#"model_reasoning_effort="high""#,
    ] {
        assert!(line.contains(needle), "{needle} missing from {line}");
    }
    assert!(!line.contains("SCENARIO"));
    assert!(!line.contains("mcp_servers"), "plan-only runs get no tools");
}

#[tokio::test]
async fn codex_returns_a_parseable_plan_and_classifies_login_failures() {
    let events = collect(codex().run(request("SCENARIO:plan")).await.unwrap()).await;
    assert!(
        matches!(finish_of(&events), Finish::Completed { result, .. } if result.contains("open_app"))
    );
    let events = collect(codex().run(request("SCENARIO:notlogged")).await.unwrap()).await;
    assert!(matches!(
        finish_of(&events),
        Finish::Failed(AiError::NotLoggedIn(_))
    ));
}

#[tokio::test]
async fn codex_can_be_cancelled_and_missing_cli_is_reported() {
    let handle = codex().run(request("SCENARIO:hang")).await.unwrap();
    let cancel = handle.canceller();
    let task = tokio::spawn(collect(handle));
    tokio::time::sleep(Duration::from_millis(400)).await;
    cancel.cancel();
    let events = tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(finish_of(&events), &Finish::Cancelled);
    let missing = CodexProvider::with_program(PathBuf::from("definitely-not-codex-xyz"));
    assert!(matches!(
        missing.run(request("hi")).await.err().unwrap(),
        AiError::NotInstalled { .. }
    ));
    let st = missing.status().await;
    assert!(!st.installed && !st.logged_in && st.detail.contains("npm install"));
}

#[tokio::test]
async fn provider_status_uses_exit_codes_and_reports_versions() {
    std::env::set_var("FAKE_FLAVOR", "codex");
    let st = codex().status().await;
    assert!(st.installed && st.logged_in);
    assert_eq!(st.version.as_deref(), Some("0.163.0"));
    assert_eq!(codex().capabilities().id, "codex");
    assert!(!codex().capabilities().streaming_deltas);
    assert!(fake().capabilities().streaming_deltas);
    std::env::remove_var("FAKE_FLAVOR");
}
