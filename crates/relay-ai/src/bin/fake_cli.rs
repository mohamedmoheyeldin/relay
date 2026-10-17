//! Test double for the AI command-line tool. The first stdin line selects a scenario
//! (`SCENARIO:echo`, `SCENARIO:hang`, ...). Not shipped with the app.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::{Read, Write};
use std::time::Duration;

use serde_json::{json, Value};

fn emit(value: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

fn init() {
    emit(
        json!({"type":"system","subtype":"init","session_id":"fake-session","model":"fake-model"}),
    );
}

fn text(t: &str) {
    emit(json!({"type":"assistant","message":{"content":[{"type":"text","text":t}]}}));
}

fn result_ok(t: &str) {
    emit(
        json!({"type":"result","subtype":"success","is_error":false,"result":t,"session_id":"fake-session","total_cost_usd":0.01}),
    );
}

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Pretend to be `codex exec --json`.
fn codex_mode(args: &[String]) {
    let mut prompt = String::new();
    let _ = std::io::stdin().read_to_string(&mut prompt);
    let scenario = prompt
        .lines()
        .find_map(|l| l.strip_prefix("SCENARIO:"))
        .unwrap_or("echo")
        .trim()
        .to_string();
    let line = |v: Value| emit(v);
    line(json!({"type":"thread.started","thread_id":"codex-thread-1"}));
    line(json!({"type":"turn.started"}));
    match scenario.as_str() {
        "echo" => {
            line(
                json!({"type":"item.started","item":{"id":"i1","type":"command_execution","command":"ls","status":"in_progress"}}),
            );
            line(
                json!({"type":"item.completed","item":{"id":"i1","type":"command_execution","command":"ls","aggregated_output":"a.txt\n","exit_code":0,"status":"completed"}}),
            );
            line(
                json!({"type":"item.completed","item":{"id":"i2","type":"agent_message","text":format!("codex:{}", prompt.lines().last().unwrap_or(""))}}),
            );
            line(
                json!({"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0}}),
            );
        }
        "args" => {
            line(
                json!({"type":"item.completed","item":{"id":"i2","type":"agent_message","text":args.join("|")}}),
            );
            line(json!({"type":"turn.completed","usage":{}}));
        }
        "plan" => {
            let plan = r#"{"objective":"x","steps":[{"id":"s1","title":"Open Notepad","action":{"tool":"open_app","name":"Notepad"}}]}"#;
            line(
                json!({"type":"item.completed","item":{"id":"i2","type":"agent_message","text":plan}}),
            );
            line(json!({"type":"turn.completed","usage":{}}));
        }
        "hang" => sleep_forever(),
        "notlogged" => {
            line(
                json!({"type":"turn.failed","error":{"message":"Not logged in. Run codex login"}}),
            );
        }
        _ => {
            eprintln!("unknown codex scenario");
            std::process::exit(2);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--sleep") {
        sleep_forever();
    }
    if args.first().map(String::as_str) == Some("--version") {
        println!(
            "{}",
            if std::env::var("FAKE_FLAVOR").is_ok_and(|v| v == "codex") {
                "codex-cli 0.163.0"
            } else {
                "2.1.300 (Claude Code)"
            }
        );
        return;
    }
    if args.len() >= 2 && args[0] == "login" && args[1] == "status" {
        let logged_in = std::env::var("FAKE_LOGGED_IN").map_or(true, |v| v != "0");
        println!(
            "{}",
            if logged_in {
                "Logged in using ChatGPT"
            } else {
                "Not logged in"
            }
        );
        std::process::exit(if logged_in { 0 } else { 1 });
    }
    if args.first().map(String::as_str) == Some("exec") {
        codex_mode(&args);
        return;
    }
    if args.len() >= 2 && args[0] == "auth" && args[1] == "status" {
        let logged_in = std::env::var("FAKE_LOGGED_IN").map_or(true, |v| v != "0");
        println!("{}", json!({"loggedIn": logged_in}));
        std::process::exit(if logged_in { 0 } else { 1 });
    }

    let mut prompt = String::new();
    let _ = std::io::stdin().read_to_string(&mut prompt);
    let scenario = prompt
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("SCENARIO:"))
        .unwrap_or("echo")
        .trim()
        .to_string();

    match scenario.as_str() {
        "echo" => {
            init();
            emit(
                json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"he"}}}),
            );
            text(&format!(
                "hello:{}",
                prompt.lines().skip(1).collect::<Vec<_>>().join("\n")
            ));
            emit(
                json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__relay__screenshot","input":{}}]}}),
            );
            emit(
                json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"OK"}]}}),
            );
            result_ok("all done");
        }
        "args" => {
            init();
            text(&args.join("|"));
            result_ok("args");
        }
        "hang" => {
            init();
            sleep_forever();
        }
        "notlogged" => {
            init();
            emit(
                json!({"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}),
            );
        }
        "crash" => {
            eprintln!("kaboom");
            std::process::exit(3);
        }
        "noresult" => {
            init();
            text("partial answer");
        }
        "chatty" => {
            init();
            for i in 0..12 {
                emit(
                    json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":format!("{i} ")}}}),
                );
                std::thread::sleep(Duration::from_millis(100));
            }
            result_ok("chatty done");
        }
        "tree" => {
            init();
            let child = std::env::current_exe()
                .ok()
                .and_then(|exe| std::process::Command::new(exe).arg("--sleep").spawn().ok());
            let pid = child.as_ref().map_or(0, std::process::Child::id);
            text(&format!("child:{pid}"));
            sleep_forever();
        }
        other => {
            eprintln!("unknown scenario {other}");
            std::process::exit(2);
        }
    }
}
