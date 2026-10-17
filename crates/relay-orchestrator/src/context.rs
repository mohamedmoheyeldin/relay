//! Structured workflow state, and the minimal, redacted context that may leave the PC.

use serde::{Deserialize, Serialize};

use crate::permissions::SharePolicy;
use crate::plan::{ExecutionPlan, PlanStep};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    /// The action ran and its effect was checked.
    Verified,
    /// The action ran but there was nothing to check it against.
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: PlanStep,
    pub status: StepStatus,
    pub attempts: u32,
    pub summary: String,
}

/// Everything known about one task, kept as data rather than conversation text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowContext {
    pub id: String,
    pub objective: String,
    pub steps: Vec<StepRecord>,
    pub approvals: Vec<String>,
    pub errors: Vec<String>,
    /// Short note about what the desktop looked like when the task began.
    pub desktop_note: String,
}

impl WorkflowContext {
    pub fn new(id: impl Into<String>, objective: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            objective: objective.into(),
            steps: Vec::new(),
            approvals: Vec::new(),
            errors: Vec::new(),
            desktop_note: String::new(),
        }
    }

    pub fn adopt(&mut self, plan: &ExecutionPlan) {
        self.steps = plan
            .steps
            .iter()
            .map(|s| StepRecord {
                step: s.clone(),
                status: StepStatus::Pending,
                attempts: 0,
                summary: String::new(),
            })
            .collect();
    }

    pub fn record_mut(&mut self, id: &str) -> Option<&mut StepRecord> {
        self.steps.iter_mut().find(|r| r.step.id == id)
    }

    pub fn completed(&self) -> Vec<&StepRecord> {
        self.steps
            .iter()
            .filter(|r| matches!(r.status, StepStatus::Verified | StepStatus::Done))
            .collect()
    }

    pub fn all_succeeded(&self) -> bool {
        !self.steps.is_empty()
            && self
                .steps
                .iter()
                .all(|r| matches!(r.status, StepStatus::Verified | StepStatus::Done))
    }

    /// Compact text for a planner that must continue or repair a task.
    pub fn progress_summary(&self) -> String {
        if self.steps.is_empty() {
            return "No steps have run yet.".into();
        }
        self.steps
            .iter()
            .map(|r| {
                format!(
                    "- {} [{}]: {}",
                    r.step.title,
                    match r.status {
                        StepStatus::Pending => "not run",
                        StepStatus::Running => "running",
                        StepStatus::Verified => "done and verified",
                        StepStatus::Done => "done",
                        StepStatus::Failed => "FAILED",
                        StepStatus::Skipped => "skipped",
                    },
                    if r.summary.is_empty() {
                        "-"
                    } else {
                        &r.summary
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A category of local information that was put in a prompt (for the privacy ledger).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shared {
    pub category: String,
    pub items: usize,
}

/// Raw facts gathered locally. Only the parts the policy allows are used for cloud prompts.
#[derive(Debug, Clone, Default)]
pub struct DesktopFacts {
    pub active_window: Option<String>,
    pub window_titles: Vec<String>,
    /// Pre-rendered UI outline of the active window (names and roles).
    pub ui_outline: Option<String>,
    pub file_names: Vec<String>,
}

/// Mask things that look like secrets or personal identifiers.
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_inclusive(char::is_whitespace) {
        let core = word.trim_end();
        let tail = &word[core.len()..];
        let digits = core.chars().filter(char::is_ascii_digit).count();
        let looks_email = core.contains('@') && core.contains('.');
        let long_secret = core.len() >= 24
            && core
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_=+/".contains(c))
            && core.chars().any(|c| c.is_ascii_digit())
            && core.chars().any(|c| c.is_ascii_alphabetic());
        let looks_number = digits >= 9 && digits * 10 >= core.len() * 6;
        let token_prefix = ["sk-", "ghp_", "gho_", "xoxb-", "xoxp-", "AKIA", "eyJ"]
            .iter()
            .any(|p| core.starts_with(p) && core.len() > 12);
        if looks_email {
            out.push_str("[email]");
        } else if token_prefix || long_secret {
            out.push_str("[secret]");
        } else if looks_number {
            out.push_str("[number]");
        } else {
            out.push_str(core);
        }
        out.push_str(tail);
    }
    out
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max_chars).collect::<String>())
    }
}

/// The context placed in a cloud prompt, plus a record of what it contains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopedContext {
    pub text: String,
    pub shared: Vec<Shared>,
}

impl ScopedContext {
    pub fn bytes(&self) -> usize {
        self.text.len()
    }
}

/// Build the least context the policy allows. Anything not allowed is simply absent.
pub fn scope_for_cloud(facts: &DesktopFacts, share: &SharePolicy) -> ScopedContext {
    let mut text = String::new();
    let mut shared = Vec::new();
    if share.window_titles {
        let mut titles: Vec<String> = facts
            .window_titles
            .iter()
            .take(15)
            .map(|t| redact(&clip(t, 80)))
            .collect();
        if let Some(active) = &facts.active_window {
            titles.retain(|t| t != &redact(&clip(active, 80)));
            titles.insert(0, format!("{} (active)", redact(&clip(active, 80))));
        }
        if !titles.is_empty() {
            text.push_str("Open windows:\n");
            for t in &titles {
                text.push_str(&format!("- {t}\n"));
            }
            shared.push(Shared {
                category: "window_titles".into(),
                items: titles.len(),
            });
        }
    }
    if share.ui_tree {
        if let Some(outline) = &facts.ui_outline {
            let outline = redact(&clip(outline, 6_000));
            text.push_str("\nActive window controls:\n");
            text.push_str(&outline);
            text.push('\n');
            shared.push(Shared {
                category: "ui_tree".into(),
                items: outline.lines().count(),
            });
        }
    }
    if share.file_names && !facts.file_names.is_empty() {
        let names: Vec<String> = facts
            .file_names
            .iter()
            .take(200)
            .map(|n| redact(&clip(n, 120)))
            .collect();
        text.push_str("\nRelevant file names:\n");
        for n in &names {
            text.push_str(&format!("- {n}\n"));
        }
        shared.push(Shared {
            category: "file_names".into(),
            items: names.len(),
        });
    }
    ScopedContext { text, shared }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanAction;

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            objective: "o".into(),
            steps: vec![
                PlanStep {
                    id: "s1".into(),
                    title: "Open".into(),
                    action: PlanAction::OpenApp {
                        name: "Notepad".into(),
                    },
                    verify: None,
                },
                PlanStep {
                    id: "s2".into(),
                    title: "Type".into(),
                    action: PlanAction::TypeText { text: "x".into() },
                    verify: None,
                },
            ],
            notes: None,
            answer: None,
        }
    }

    #[test]
    fn workflow_state_tracks_steps() {
        let mut ctx = WorkflowContext::new("w1", "do it");
        assert!(!ctx.all_succeeded());
        ctx.adopt(&plan());
        assert_eq!(ctx.steps.len(), 2);
        ctx.record_mut("s1").unwrap().status = StepStatus::Verified;
        assert_eq!(ctx.completed().len(), 1);
        assert!(!ctx.all_succeeded());
        ctx.record_mut("s2").unwrap().status = StepStatus::Done;
        assert!(ctx.all_succeeded());
        assert!(ctx.record_mut("nope").is_none());
        ctx.record_mut("s2").unwrap().status = StepStatus::Failed;
        ctx.record_mut("s2").unwrap().summary = "no window".into();
        let s = ctx.progress_summary();
        assert!(s.contains("Open [done and verified]") && s.contains("Type [FAILED]: no window"));
    }

    #[test]
    fn redaction_masks_secrets_and_identifiers() {
        assert_eq!(
            redact("mail me at jo@example.com now"),
            "mail me at [email] now"
        );
        assert_eq!(redact("key sk-abcdef1234567890abcd"), "key [secret]");
        assert_eq!(redact("card 4111111111111111 ok"), "card [number] ok");
        assert_eq!(redact("token ghp_ABCDEFGHIJKLMNOPQRSTUV"), "token [secret]");
        assert_eq!(
            redact("plain words stay: Report 2024.docx"),
            "plain words stay: Report 2024.docx"
        );
        assert_eq!(redact("SSN 123456789"), "SSN [number]");
    }

    #[test]
    fn nothing_is_shared_that_the_policy_does_not_allow() {
        let facts = DesktopFacts {
            active_window: Some("Budget.xlsx - Excel".into()),
            window_titles: vec!["Budget.xlsx - Excel".into(), "Inbox - Mail".into()],
            ui_outline: Some("button Save\nedit Name".into()),
            file_names: vec!["taxes.pdf".into()],
        };
        let minimal = scope_for_cloud(&facts, &SharePolicy::default());
        assert!(minimal.text.contains("Excel (active)") && minimal.text.contains("Inbox"));
        assert!(!minimal.text.contains("taxes") && !minimal.text.contains("button Save"));
        assert_eq!(
            minimal.shared,
            vec![Shared {
                category: "window_titles".into(),
                items: 2
            }]
        );

        let none = scope_for_cloud(
            &facts,
            &SharePolicy {
                window_titles: false,
                ..SharePolicy::default()
            },
        );
        assert!(none.text.is_empty() && none.shared.is_empty());

        let all = scope_for_cloud(
            &facts,
            &SharePolicy {
                window_titles: true,
                ui_tree: true,
                file_names: true,
                file_contents: false,
                screenshots: false,
            },
        );
        assert!(all.text.contains("button Save") && all.text.contains("taxes.pdf"));
        assert_eq!(all.shared.len(), 3);
    }

    #[test]
    fn shared_text_is_redacted_and_bounded() {
        let facts = DesktopFacts {
            window_titles: vec!["Mail - jo@example.com".into()],
            ..DesktopFacts::default()
        };
        let s = scope_for_cloud(&facts, &SharePolicy::default());
        assert!(s.text.contains("[email]") && !s.text.contains("example.com"));
        let big = DesktopFacts {
            window_titles: (0..100).map(|i| format!("Window {i}")).collect(),
            ..DesktopFacts::default()
        };
        assert_eq!(
            scope_for_cloud(&big, &SharePolicy::default()).shared[0].items,
            15
        );
    }
}
