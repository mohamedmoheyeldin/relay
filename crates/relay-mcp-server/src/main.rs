//! `relay-mcp`: Relay as an MCP server over stdio.
//!
//! Add it to a client, for example:
//!   claude mcp add relay -- relay-mcp
//!   codex mcp add relay -- relay-mcp
//!
//! Options:
//!   --tools safe|all       safe = verified actions only; all (default) also offers screen control
//!   --approvals native|deny   how destructive actions are confirmed (default native dialog)
//!   --data-dir PATH        where settings.json and logs live (default: the Relay app's folder)
//!   --version, --help

use std::sync::Arc;

use relay_mcp_server::approver::{DenyPrompter, DialogApprover, NativePrompter, Prompter};
use relay_mcp_server::{build_toolbox, default_data_dir, hotkey, protocol, Surface};
use relay_orchestrator::config::HybridConfig;
use tokio::io::BufReader;

const HELP: &str = "relay-mcp: Relay as an MCP server (stdio).\n\n\
Usage: relay-mcp [--tools safe|all] [--approvals native|deny] [--data-dir PATH]\n\
Add to Claude Code: claude mcp add relay -- relay-mcp\n";

struct Options {
    surface: Surface,
    deny: bool,
    data_dir: std::path::PathBuf,
}

fn parse_args(args: &[String]) -> Result<Option<Options>, String> {
    let mut o = Options {
        surface: Surface::All,
        deny: false,
        data_dir: default_data_dir(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--version" | "-V" => {
                println!("relay-mcp {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--tools" => {
                o.surface = match it.next().map(String::as_str) {
                    Some("safe") => Surface::Safe,
                    Some("all") => Surface::All,
                    other => return Err(format!("--tools expects safe or all, got {other:?}")),
                }
            }
            "--approvals" => {
                o.deny = match it.next().map(String::as_str) {
                    Some("native") => false,
                    Some("deny") => true,
                    other => {
                        return Err(format!("--approvals expects native or deny, got {other:?}"))
                    }
                }
            }
            "--data-dir" => {
                o.data_dir = it
                    .next()
                    .map(std::path::PathBuf::from)
                    .ok_or("--data-dir needs a path")?
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    Ok(Some(o))
}

fn init_logging(dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let logs = dir.join("logs");
    std::fs::create_dir_all(&logs).ok()?;
    let appender = tracing_appender::rolling::daily(logs, "relay-mcp.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    Some(guard)
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(&args) {
        Ok(Some(o)) => o,
        Ok(None) => return,
        Err(e) => {
            eprintln!("relay-mcp: {e}\n{HELP}");
            std::process::exit(2);
        }
    };
    let _guard = init_logging(&options.data_dir);
    let config = HybridConfig::load_from_settings(&options.data_dir.join("settings.json"));
    let prompter: Box<dyn Prompter> = if options.deny {
        Box::new(DenyPrompter)
    } else {
        Box::new(NativePrompter::new())
    };
    let approver = Arc::new(DialogApprover::new(prompter));
    let audit_dir = options.data_dir.join("runs").join(format!(
        "mcp-{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default(),
        std::process::id()
    ));
    let (toolbox, kill) = match build_toolbox(&config, options.surface, approver, Some(audit_dir)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("relay-mcp: cannot open the desktop: {e}");
            std::process::exit(1);
        }
    };
    hotkey::spawn(kill);
    tracing::info!(
        "relay-mcp {} ready ({:?} tools)",
        env!("CARGO_PKG_VERSION"),
        options.surface
    );
    let stdin = BufReader::new(tokio::io::stdin());
    protocol::serve(toolbox, stdin, tokio::io::stdout()).await;
}
