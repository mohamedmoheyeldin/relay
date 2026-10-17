//! Owner-facing wording for how a run ended: plain sentences, honest about what was checked.

use crate::context::{StepStatus, WorkflowContext};
use crate::executor::RunOutcome;
use crate::plan::ExecutionPlan;

fn is_read(tool: &str) -> bool {
    matches!(tool, "find_files" | "list_dir" | "read_text_file")
}

fn sentence(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() || t.ends_with(['.', '!', '?', ':']) {
        t.to_string()
    } else {
        format!("{t}.")
    }
}

/// The owner-facing summary of how a run ended, without overstating what was checked.
pub fn final_message(plan: &ExecutionPlan, ctx: &WorkflowContext, outcome: &RunOutcome) -> String {
    let mut parts = Vec::new();
    if let Some(answer) = plan.answer.as_deref().filter(|a| !a.trim().is_empty()) {
        parts.push(answer.trim().to_string());
    }
    // Results of look-ups are the point of the task (file lists, text read).
    for r in &ctx.steps {
        if is_read(r.step.action.tool())
            && matches!(r.status, StepStatus::Verified | StepStatus::Done)
            && !r.summary.is_empty()
        {
            parts.push(format!("**{}**\n{}", r.step.title, r.summary));
        }
    }
    let total = ctx.steps.len();
    let verified = ctx
        .steps
        .iter()
        .filter(|r| r.status == StepStatus::Verified)
        .count();
    let unchecked: Vec<_> = ctx
        .steps
        .iter()
        .filter(|r| r.status == StepStatus::Done && !is_read(r.step.action.tool()))
        .collect();
    let changes: Vec<String> = ctx
        .steps
        .iter()
        .filter(|r| {
            matches!(r.status, StepStatus::Verified | StepStatus::Done)
                && !is_read(r.step.action.tool())
        })
        .map(|r| {
            if r.summary.is_empty() {
                sentence(&r.step.title)
            } else {
                sentence(&r.summary)
            }
        })
        .collect();
    if total > 0 {
        match outcome {
            RunOutcome::Completed => {
                if changes.len() == 1 {
                    parts.push(changes[0].clone());
                } else if changes.len() > 1 {
                    parts.push(
                        changes
                            .iter()
                            .map(|c| format!("- {c}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                let acted = changes.len();
                if acted > 0 {
                    if unchecked.is_empty() {
                        parts.push(if acted == 1 {
                            "I checked afterwards and it worked.".into()
                        } else {
                            format!("I checked all {acted} steps afterwards and they worked.")
                        });
                    } else if unchecked.len() == acted {
                        parts.push(if acted == 1 {
                            "I can't check that one automatically, so take a quick look.".into()
                        } else {
                            format!(
                                "I can't check these automatically, so take a quick look ({acted} steps)."
                            )
                        });
                    } else {
                        parts.push(format!(
                            "I checked {verified} of {acted} steps afterwards; the other {} can't be checked automatically.",
                            unchecked.len()
                        ));
                    }
                }
            }
            RunOutcome::Failed { step_id, error } => {
                let title = ctx
                    .steps
                    .iter()
                    .find(|r| &r.step.id == step_id)
                    .map(|r| r.step.title.as_str())
                    .unwrap_or("a step");
                parts.push(format!(
                    "I stopped at “{title}”: {}\n{} of {total} step(s) were done and checked.",
                    sentence(error),
                    verified
                ));
            }
            RunOutcome::Denied { reason, .. } => parts.push(format!(
                "I refused that step: {} Nothing after it ran.",
                sentence(reason)
            )),
            RunOutcome::Declined { .. } => {
                parts.push("You declined that step, so I stopped there.".into());
            }
            RunOutcome::Cancelled => parts.push(format!(
                "Stopped. {verified} of {total} step(s) were done and checked."
            )),
        }
    }
    if parts.is_empty() {
        parts.push("Done.".into());
    }
    parts.join("\n\n")
}
