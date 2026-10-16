//! Safety policy: what needs the owner's approval, the kill switch, and the approval channel.

use async_trait::async_trait;
use relay_os::driver::StopFlag;
use relay_protocol::{Risk, RunId};
use serde_json::Value;

/// Text returned to the AI for any action refused because the owner pressed stop.
pub const STOPPED_MESSAGE: &str = "Relay was stopped by the owner. Do not retry this action or try to work around the stop. Tell the owner what you were doing and wait for their next instruction.";

/// What an approval prompt asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub run_id: RunId,
    pub title: String,
    pub detail: String,
    pub risk: Risk,
}

/// The owner's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Denied,
    TimedOut,
}

impl Decision {
    pub fn is_approved(self) -> bool {
        self == Decision::Approved
    }

    /// Message handed back to the AI when the action is not allowed.
    pub fn refusal_message(self) -> &'static str {
        match self {
            Decision::Approved => "approved",
            Decision::Denied => "The owner declined this action. Do not retry it; ask the owner what they want instead.",
            Decision::TimedOut => "The owner did not answer in time, so the action was not taken. Ask them in your reply before trying again.",
        }
    }
}

/// Asks the human. Implemented by the app (a confirmation card in the chat window).
#[async_trait]
pub trait Approver: Send + Sync {
    async fn ask(&self, request: ApprovalRequest) -> Decision;
}

/// How often approvals are required for desktop actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalMode {
    /// Only for actions that look destructive or irreversible (default).
    #[default]
    Destructive,
    /// Every action that changes anything.
    Always,
    /// Never (for unattended testing only).
    Off,
}

/// The global kill switch shared by the input driver, the tools and the hotkey.
#[derive(Debug, Clone, Default)]
pub struct KillSwitch {
    stop: StopFlag,
}

impl KillSwitch {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn trip(&self) {
        self.stop.trip();
    }
    pub fn rearm(&self) {
        self.stop.reset();
    }
    pub fn is_tripped(&self) -> bool {
        self.stop.is_tripped()
    }
    /// The flag the input driver watches.
    pub fn flag(&self) -> StopFlag {
        self.stop.clone()
    }
}

/// Outcome of classifying an action that is about to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Ask {
        title: String,
        detail: String,
        risk: Risk,
    },
}

impl Verdict {
    fn ask(title: impl Into<String>, detail: impl Into<String>, risk: Risk) -> Self {
        Verdict::Ask {
            title: title.into(),
            detail: detail.into(),
            risk,
        }
    }
}

/// Words on a control that suggest an irreversible effect.
const RISKY_CONTROL_WORDS: [&str; 30] = [
    "send",
    "submit",
    "pay",
    "buy",
    "purchase",
    "order",
    "checkout",
    "delete",
    "remove",
    "erase",
    "uninstall",
    "format",
    "transfer",
    "confirm",
    "publish",
    "post",
    "reply",
    "discard",
    "empty",
    "reset",
    "book",
    "reserve",
    "subscribe",
    "donate",
    "place",
    "sign",
    "agree",
    "accept",
    "install",
    "overwrite",
];

/// Whether a control's accessible name looks like it triggers something irreversible.
pub fn control_name_is_risky(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| RISKY_CONTROL_WORDS.contains(&word))
}

/// Shorten text for display, on a character boundary.
pub fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    format!("{cut}…")
}

const DESTRUCTIVE_COMMANDS: [&str; 24] = [
    "rm",
    "rmdir",
    "rd",
    "del",
    "erase",
    "remove-item",
    "ri",
    "format",
    "mkfs",
    "dd",
    "shutdown",
    "restart-computer",
    "stop-computer",
    "kill",
    "taskkill",
    "stop-process",
    "clear-content",
    "truncate",
    "shred",
    "sdelete",
    "diskpart",
    "cipher",
    "robocopy",
    "rename-item",
];

const DESTRUCTIVE_SEQUENCES: [&[&str]; 14] = [
    &["git", "push"],
    &["git", "reset", "--hard"],
    &["git", "clean"],
    &["git", "checkout", "--"],
    &["git", "branch", "-d"],
    &["git", "branch", "-D"],
    &["npm", "publish"],
    &["cargo", "publish"],
    &["docker", "rm"],
    &["docker", "system", "prune"],
    &["reg", "delete"],
    &["net", "user"],
    &["gh", "pr", "merge"],
    &["gh", "repo", "delete"],
];

const SAFE_COMMANDS: [&str; 28] = [
    "ls",
    "dir",
    "cat",
    "type",
    "echo",
    "pwd",
    "cd",
    "where",
    "which",
    "get-childitem",
    "gci",
    "get-content",
    "gc",
    "select-string",
    "findstr",
    "grep",
    "rg",
    "whoami",
    "hostname",
    "get-date",
    "date",
    "get-process",
    "tasklist",
    "systeminfo",
    "ver",
    "get-location",
    "test-path",
    "write-output",
];

/// Split a shell line into command segments on `;`, `|`, `&`, and newlines.
fn segments(command: &str) -> Vec<Vec<String>> {
    command
        .split([';', '|', '&', '\n'])
        .map(|seg| {
            seg.split(|c: char| c.is_whitespace() || "$(){},\"'`".contains(c))
                .map(str::to_lowercase)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_destructive_command(command: &str) -> bool {
    let lower = command.to_lowercase();
    if lower.contains(" -method post")
        || lower.contains(" -method delete")
        || lower.contains(" -x post")
        || lower.contains(" -x delete")
        || lower.contains("> /dev/sd")
    {
        return true;
    }
    for seg in segments(command) {
        for (i, token) in seg.iter().enumerate() {
            let bare = token.rsplit(['\\', '/']).next().unwrap_or(token);
            let bare = bare.strip_suffix(".exe").unwrap_or(bare);
            // Anywhere in the line, so `ForEach-Object { Remove-Item .. }` is caught too.
            // This errs towards asking, which is the safe direction.
            if DESTRUCTIVE_COMMANDS.contains(&bare) {
                return true;
            }
            for pattern in DESTRUCTIVE_SEQUENCES {
                let tail = &seg[i..];
                if tail.len() >= pattern.len()
                    && tail
                        .iter()
                        .zip(pattern.iter())
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                {
                    return true;
                }
            }
        }
    }
    false
}

fn is_read_only_command(command: &str) -> bool {
    let segs = segments(command);
    if segs.is_empty() {
        return false;
    }
    // Redirects write files; sub-expressions, script blocks and escapes can hide other commands.
    if command.contains(['>', '`', '{']) || command.contains("$(") {
        return false;
    }
    segs.iter().all(|seg| {
        let head = seg[0].as_str();
        // PowerShell getters and output formatters only read.
        if SAFE_COMMANDS.contains(&head)
            || head.starts_with("get-")
            || matches!(
                head,
                "format-list"
                    | "format-table"
                    | "select-object"
                    | "where-object"
                    | "sort-object"
                    | "measure-object"
                    | "out-string"
                    | "test-connection"
                    | "resolve-path"
            )
        {
            return true;
        }
        match head {
            "git" => matches!(
                seg.get(1).map(String::as_str),
                Some("status" | "log" | "diff" | "show" | "branch" | "remote" | "rev-parse")
            ),
            "cargo" => matches!(
                seg.get(1).map(String::as_str),
                Some("check" | "build" | "test" | "fmt" | "clippy" | "--version")
            ),
            "node" | "python" | "pnpm" | "npm" | "claude" => {
                matches!(seg.get(1).map(String::as_str), Some("-v" | "--version"))
            }
            _ => false,
        }
    })
}

fn is_read_only_name(tool: &str) -> bool {
    let base = tool.rsplit("__").next().unwrap_or(tool).to_lowercase();
    const PREFIXES: [&str; 11] = [
        "get", "list", "search", "read", "fetch", "query", "describe", "find", "view", "show",
        "lookup",
    ];
    PREFIXES.iter().any(|p| base.starts_with(p))
}

fn is_destructive_name(tool: &str) -> bool {
    let base = tool.rsplit("__").next().unwrap_or(tool).to_lowercase();
    const WORDS: [&str; 14] = [
        "send", "delete", "remove", "merge", "push", "submit", "pay", "publish", "post", "trash",
        "drop", "reply", "forward", "cancel",
    ];
    base.split(|c: char| !c.is_alphanumeric())
        .any(|w| WORDS.contains(&w))
}

/// Classify one of the AI tool's own tool calls (the permission-prompt path).
pub fn classify_builtin(tool: &str, input: &Value) -> Verdict {
    match tool {
        "Read" | "Glob" | "Grep" | "LS" | "ToolSearch" | "TodoWrite" | "WebSearch" | "WebFetch"
        | "NotebookRead" | "BashOutput" | "Skill" => Verdict::Allow,
        "Bash" | "PowerShell" => {
            let command = input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if is_destructive_command(command) {
                Verdict::ask(
                    "Run a command that can delete or send things",
                    truncate(command, 400),
                    Risk::Destructive,
                )
            } else if is_read_only_command(command) {
                Verdict::Allow
            } else {
                Verdict::ask("Run a command", truncate(command, 400), Risk::Reversible)
            }
        }
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            let path = input
                .get("file_path")
                .or_else(|| input.get("notebook_path"))
                .and_then(Value::as_str)
                .unwrap_or("a file");
            Verdict::ask("Change a file", truncate(path, 300), Risk::Reversible)
        }
        other if other.starts_with("mcp__relay__") => Verdict::Allow,
        other if other.starts_with("mcp__") => {
            let detail = truncate(&input.to_string(), 400);
            if is_destructive_name(other) {
                Verdict::ask(format!("Use {other}"), detail, Risk::Destructive)
            } else if is_read_only_name(other) {
                Verdict::Allow
            } else {
                Verdict::ask(format!("Use {other}"), detail, Risk::Reversible)
            }
        }
        other => Verdict::ask(
            format!("Use {other}"),
            truncate(&input.to_string(), 400),
            Risk::Reversible,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(cmd: &str) -> Verdict {
        classify_builtin("Bash", &json!({ "command": cmd }))
    }

    fn risk_of(v: &Verdict) -> Option<Risk> {
        match v {
            Verdict::Ask { risk, .. } => Some(*risk),
            Verdict::Allow => None,
        }
    }

    #[test]
    fn read_only_tools_pass_without_asking() {
        for tool in [
            "Read",
            "Glob",
            "Grep",
            "WebSearch",
            "mcp__relay__screenshot",
        ] {
            assert_eq!(classify_builtin(tool, &json!({})), Verdict::Allow, "{tool}");
        }
    }

    #[test]
    fn destructive_shell_commands_ask_as_destructive() {
        for cmd in [
            "rm -rf build",
            "del /s C:\\temp\\x",
            "Remove-Item -Recurse C:\\data",
            "git push origin main",
            "git reset --hard HEAD~1",
            "echo hi; rm file.txt",
            "dir | del foo",
            "C:\\tools\\rm.exe a",
            "sudo rm -rf /",
            "Invoke-WebRequest -Uri x -Method POST",
            "cargo publish",
        ] {
            assert_eq!(risk_of(&bash(cmd)), Some(Risk::Destructive), "{cmd}");
        }
    }

    #[test]
    fn read_only_shell_commands_are_allowed() {
        for cmd in [
            "dir",
            "ls -la",
            "git status",
            "git log --oneline -5",
            "cargo test --workspace",
            "Get-ChildItem C:\\Projects | Select-String relay",
            "node --version",
            "Get-Item C:\\a.txt | Format-List FullName,Length",
            "Get-Service | Where-Object Status -eq Running | Sort-Object Name",
        ] {
            assert_eq!(bash(cmd), Verdict::Allow, "{cmd}");
        }
    }

    #[test]
    fn hidden_destructive_commands_are_not_waved_through() {
        assert_eq!(
            risk_of(&bash("Get-ChildItem | ForEach-Object { Remove-Item $_ }")),
            Some(Risk::Destructive)
        );
        assert_eq!(
            risk_of(&bash("Get-Item $(Remove-Item x)")),
            Some(Risk::Destructive)
        );
        // Not recognisably destructive, but dynamic code: never auto-allowed.
        assert!(risk_of(&bash("Get-Item $(Start-Process evil)")).is_some());
        assert!(risk_of(&bash("Get-ChildItem | ForEach-Object { $_.Name }")).is_some());
        assert!(risk_of(&bash("Get-Content a.txt | iex")).is_some());
    }

    #[test]
    fn unknown_commands_and_redirects_ask_as_reversible() {
        for cmd in [
            "python script.py",
            "echo hi > out.txt",
            "mkdir x",
            "winget install foo",
        ] {
            assert_eq!(risk_of(&bash(cmd)), Some(Risk::Reversible), "{cmd}");
        }
    }

    #[test]
    fn file_writes_ask() {
        let v = classify_builtin("Write", &json!({"file_path": "C:\\a.txt"}));
        assert_eq!(risk_of(&v), Some(Risk::Reversible));
    }

    #[test]
    fn external_mcp_tools_are_classified_by_name() {
        assert_eq!(
            risk_of(&classify_builtin("mcp__gmail__send_email", &json!({}))),
            Some(Risk::Destructive)
        );
        assert_eq!(
            risk_of(&classify_builtin(
                "mcp__github__merge_pull_request",
                &json!({})
            )),
            Some(Risk::Destructive)
        );
        assert_eq!(
            classify_builtin("mcp__github__list_issues", &json!({})),
            Verdict::Allow
        );
        assert_eq!(
            risk_of(&classify_builtin("mcp__notes__create_note", &json!({}))),
            Some(Risk::Reversible)
        );
    }

    #[test]
    fn unknown_builtin_tools_ask() {
        assert!(risk_of(&classify_builtin("SomethingNew", &json!({}))).is_some());
    }

    #[test]
    fn risky_control_names() {
        for name in [
            "Send",
            "Delete message",
            "Place order",
            "Sign in",
            "Pay now",
            "OK, accept",
        ] {
            assert!(control_name_is_risky(name), "{name}");
        }
        for name in ["Search", "Save", "Cancel", "Next", "", "Sender settings"] {
            assert!(!control_name_is_risky(name), "{name}");
        }
    }

    #[test]
    fn truncation_is_on_char_boundaries() {
        assert_eq!(truncate("héllo wörld", 5), "héllo…");
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn kill_switch_shares_state_with_its_flag() {
        let k = KillSwitch::new();
        let flag = k.flag();
        assert!(!flag.is_tripped());
        k.trip();
        assert!(flag.is_tripped());
        assert!(k.is_tripped());
        k.rearm();
        assert!(!flag.is_tripped());
    }

    #[test]
    fn decision_messages_tell_the_ai_not_to_retry() {
        assert!(Decision::Denied.refusal_message().contains("Do not retry"));
        assert!(Decision::TimedOut
            .refusal_message()
            .contains("did not answer"));
        assert!(Decision::Approved.is_approved());
    }
}
