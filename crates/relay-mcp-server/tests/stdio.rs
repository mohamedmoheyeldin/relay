#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]
//! The MCP server end to end: JSON-RPC over a pipe against real filesystem actions in a temp folder,
//! plus the shipped binary over real stdio.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use relay_mcp::KillSwitch;
use relay_mcp_server::approver::{DenyPrompter, DialogApprover, Prompter};
use relay_mcp_server::{protocol, Toolbox};
use relay_orchestrator::config::HybridConfig;
use relay_orchestrator::SystemServices;
use relay_os::driver::{Driver, Timing};
use relay_os::testing::FakeDesktop;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as TBufReader};

struct Yes;
#[async_trait]
impl Prompter for Yes {
    async fn ask(&self, _t: &str, _x: &str, _d: Duration) -> bool {
        true
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("relay-mcp-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn config_for(root: &Path) -> HybridConfig {
    HybridConfig {
        allowed_roots: vec![root.display().to_string()],
        protected_roots: vec![],
        ..HybridConfig::default()
    }
}

fn toolbox(root: &Path, prompter: Box<dyn Prompter>, with_screen: bool) -> Arc<Toolbox> {
    let desktop = Arc::new(FakeDesktop::new());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        desktop.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let cfg = config_for(root);
    let services = Arc::new(SystemServices::new(
        desktop.clone(),
        driver.clone(),
        vec![root.to_path_buf()],
    ));
    let approver = Arc::new(DialogApprover::new(prompter));
    let host = with_screen.then(|| {
        Arc::new(relay_mcp::ToolHost::new(
            desktop,
            driver,
            kill.clone(),
            approver.clone(),
            None,
            relay_mcp::HostOptions::default(),
        ))
    });
    Arc::new(Toolbox::new(host, services, cfg.policy(), approver, kill))
}

/// A client wired to the server through in-memory pipes.
struct Client {
    to_server: tokio::io::DuplexStream,
    from_server: tokio::io::Lines<TBufReader<tokio::io::DuplexStream>>,
    next: u64,
}

impl Client {
    fn start(toolbox: Arc<Toolbox>) -> Self {
        let (client_w, server_r) = tokio::io::duplex(1 << 16);
        let (server_w, client_r) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move {
            protocol::serve(toolbox, TBufReader::new(server_r), server_w).await;
        });
        Self {
            to_server: client_w,
            from_server: TBufReader::new(client_r).lines(),
            next: 1,
        }
    }

    async fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        self.to_server
            .write_all(format!("{msg}\n").as_bytes())
            .await
            .unwrap();
        let line = tokio::time::timeout(Duration::from_secs(20), self.from_server.next_line())
            .await
            .expect("server answered in time")
            .unwrap()
            .expect("server is still there");
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["id"], json!(id));
        v
    }

    async fn call(&mut self, name: &str, args: Value) -> (bool, String) {
        let v = self
            .rpc("tools/call", json!({"name": name, "arguments": args}))
            .await;
        let r = &v["result"];
        let text = r["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (r["isError"].as_bool().unwrap(), text)
    }
}

#[tokio::test]
async fn the_handshake_lists_verified_tools_and_screen_control() {
    let dir = temp("list");
    let mut c = Client::start(toolbox(&dir, Box::new(DenyPrompter), true));
    let init = c
        .rpc("initialize", json!({"protocolVersion": "2025-06-18"}))
        .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "relay");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let odd = c
        .rpc("initialize", json!({"protocolVersion": "1999-01-01"}))
        .await;
    assert_eq!(
        odd["result"]["protocolVersion"], "2025-06-18",
        "falls back to a version we speak"
    );

    let tools = c.rpc("tools/list", json!({})).await;
    let list = tools["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = list.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "find_files",
        "move_to_trash",
        "run_plan",
        "screenshot",
        "left_click",
        "resume_after_stop",
    ] {
        assert!(names.contains(&want), "missing {want}: {names:?}");
    }
    assert!(
        !names.contains(&"approve") && !names.contains(&"ask_user"),
        "internal tools stay hidden"
    );
    assert_eq!(
        names.iter().filter(|n| **n == "focus_window").count(),
        1,
        "no duplicate names"
    );
    let trash = list.iter().find(|t| t["name"] == "move_to_trash").unwrap();
    assert_eq!(trash["annotations"]["destructiveHint"], true);
    let find = list.iter().find(|t| t["name"] == "find_files").unwrap();
    assert_eq!(find["annotations"]["readOnlyHint"], true);
    assert_eq!(find["inputSchema"]["type"], "object");

    let safe = Client::start(toolbox(&dir, Box::new(DenyPrompter), false))
        .rpc("tools/list", json!({}))
        .await;
    let _ = safe;
    let mut s = Client::start(toolbox(&dir, Box::new(DenyPrompter), false));
    let tools = s.rpc("tools/list", json!({})).await;
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"screenshot") && names.contains(&"focus_window"),
        "{names:?}"
    );
    let unknown = s.rpc("nope", json!({})).await;
    assert_eq!(unknown["error"]["code"], -32601);
}

#[tokio::test]
async fn reads_run_inside_allowed_folders_and_nowhere_else() {
    let dir = temp("reads");
    std::fs::write(dir.join("notes.txt"), "hello relay").unwrap();
    let outside = temp("outside");
    std::fs::write(outside.join("secret.txt"), "no").unwrap();
    let mut c = Client::start(toolbox(&dir, Box::new(Yes), false));

    // Reading file contents asks the owner first: the text is about to leave for a cloud AI.
    let (err, text) = c
        .call("read_text_file", json!({"path": dir.join("notes.txt")}))
        .await;
    assert!(!err && text.contains("hello relay"), "{text}");
    let (err, text) = c.call("list_dir", json!({"path": &dir})).await;
    assert!(!err && text.contains("notes.txt"), "{text}");
    let (err, text) = c
        .call("find_files", json!({"query": "*.txt", "root": &dir}))
        .await;
    assert!(!err && text.contains("notes.txt"), "{text}");

    let (err, text) = c
        .call(
            "read_text_file",
            json!({"path": outside.join("secret.txt")}),
        )
        .await;
    assert!(
        err && text.contains("refused"),
        "outside the allowed folders: {text}"
    );
    let (err, text) = c
        .call("read_text_file", json!({"path": "relative.txt"}))
        .await;
    assert!(err, "relative paths are rejected: {text}");
}

#[tokio::test]
async fn changes_are_verified_on_the_machine() {
    let dir = temp("verify");
    let mut c = Client::start(toolbox(&dir, Box::new(Yes), false));
    let made = dir.join("projects").join("new");
    let (err, text) = c.call("create_dir", json!({"path": &made})).await;
    assert!(!err && text.contains("verified") && made.is_dir(), "{text}");

    let file = made.join("a.txt");
    let (err, text) = c
        .call("write_text_file", json!({"path": &file, "content": "x"}))
        .await;
    assert!(!err && text.contains("verified"), "{text}");
    let moved = made.join("b.txt");
    let (err, text) = c
        .call("move_path", json!({"from": &file, "to": &moved}))
        .await;
    assert!(
        !err && text.contains("verified") && moved.exists() && !file.exists(),
        "{text}"
    );

    let (err, text) = c.call("move_to_trash", json!({"paths": [&moved]})).await;
    assert!(
        !err && text.contains("verified") && !moved.exists(),
        "{text}"
    );
}

#[tokio::test]
async fn destructive_actions_need_the_owner_and_declining_changes_nothing() {
    let dir = temp("decline");
    let f = dir.join("keep.txt");
    std::fs::write(&f, "keep me").unwrap();
    let mut c = Client::start(toolbox(&dir, Box::new(DenyPrompter), false));
    let (err, text) = c.call("move_to_trash", json!({"paths": [&f]})).await;
    assert!(err && text.contains("declined"), "{text}");
    assert!(f.exists(), "the file must still be there");
    let (err, _) = c
        .call(
            "write_text_file",
            json!({"path": &f, "content": "overwritten", "overwrite": true}),
        )
        .await;
    assert!(err);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), "keep me");
}

#[tokio::test]
async fn a_plan_runs_in_order_and_reports_each_step() {
    let dir = temp("plan");
    let a = dir.join("a");
    let plan = json!({
        "objective": "make two folders",
        "steps": [
            {"id": "s1", "title": "First", "action": {"tool": "create_dir", "path": &a},
             "verify": {"check": "file_exists", "path": &a}},
            {"id": "s2", "title": "Second", "action": {"tool": "create_dir", "path": dir.join("b")}}
        ]
    });
    let mut c = Client::start(toolbox(&dir, Box::new(DenyPrompter), false));
    let (err, text) = c.call("run_plan", plan).await;
    assert!(!err, "{text}");
    assert!(
        text.contains("First [verified]") && text.contains("Second [ran"),
        "{text}"
    );
    assert!(dir.join("a").is_dir() && dir.join("b").is_dir());

    let (err, text) = c
        .call("run_plan", json!({"objective": "x", "steps": [{"id": "s1", "title": "t", "action": {"tool": "run_shell", "command": "del /s C:\\"}}]}))
        .await;
    assert!(
        err && text.to_lowercase().contains("plan"),
        "unknown tools are rejected: {text}"
    );
}

#[tokio::test]
async fn the_stop_switch_blocks_everything_until_resumed() {
    let dir = temp("stop");
    let tb = toolbox(&dir, Box::new(DenyPrompter), false);
    let mut c = Client::start(tb.clone());
    tb.kill_switch().trip();
    let (err, text) = c.call("list_dir", json!({"path": &dir})).await;
    assert!(err && text.to_lowercase().contains("stop"), "{text}");
    let (err, _) = c.call("resume_after_stop", json!({})).await;
    assert!(!err);
    let (err, text) = c.call("list_dir", json!({"path": &dir})).await;
    assert!(!err, "{text}");
}

#[tokio::test]
async fn switched_off_tools_are_hidden_and_refused() {
    let dir = temp("off");
    let mut cfg = config_for(&dir);
    cfg.disabled_tools = vec!["move_to_trash".into(), "write_text_file".into()];
    let desktop = Arc::new(FakeDesktop::new());
    let kill = KillSwitch::new();
    let driver = Arc::new(Driver::with_timing(
        desktop.clone(),
        kill.flag(),
        Timing::instant(),
    ));
    let services = Arc::new(SystemServices::new(desktop, driver, vec![dir.clone()]));
    let tb = Arc::new(Toolbox::new(
        None,
        services,
        cfg.policy(),
        Arc::new(DialogApprover::new(Box::new(Yes))),
        kill,
    ));
    let mut c = Client::start(tb);
    let tools = c.rpc("tools/list", json!({})).await;
    let names: Vec<String> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    assert!(!names.contains(&"move_to_trash".to_string()));
    let (err, text) = c
        .call("move_to_trash", json!({"paths": [dir.join("x")]}))
        .await;
    assert!(err && text.contains("switched off"), "{text}");
}

#[test]
fn the_shipped_binary_speaks_mcp_over_real_stdio() {
    let data = temp("bin");
    let root = temp("bin-root");
    std::fs::write(root.join("hello.txt"), "from the binary").unwrap();
    let cfg =
        json!({"hybrid": {"allowed_roots": [root.display().to_string()], "protected_roots": []}});
    std::fs::write(data.join("settings.json"), cfg.to_string()).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_relay-mcp"))
        .args(["--tools", "safe", "--approvals", "deny", "--data-dir"])
        .arg(&data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut ask = |id: u64, method: &str, params: Value| -> Value {
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("stdout must be protocol only: {line:?} ({e})"))
    };
    let init = ask(1, "initialize", json!({"protocolVersion": "2025-06-18"}));
    assert_eq!(init["result"]["serverInfo"]["name"], "relay");
    let list = ask(
        2,
        "tools/call",
        json!({"name": "list_dir", "arguments": {"path": &root}}),
    );
    assert!(
        list["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello.txt"),
        "{list}"
    );
    // With approvals set to deny, reading file contents is refused: nothing is sent without a yes.
    let read = ask(
        3,
        "tools/call",
        json!({"name": "read_text_file", "arguments": {"path": root.join("hello.txt")}}),
    );
    assert_eq!(read["result"]["isError"], true, "{read}");
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "exits cleanly when the client disconnects: {status:?}"
    );
}
