//! Owner-facing wording for how a run ended.

use crate::context::{StepStatus, WorkflowContext};
use crate::executor::RunOutcome;
use crate::plan::ExecutionPlan;

/// The owner-facing summary of how a run ended, without overstating what was checked.
pub fn final_message(plan: &ExecutionPlan, ctx: &WorkflowContext, outcome: &RunOutcome) -> String {
    let mut parts = Vec::new();
    if let Some(answer) = plan.answer.as_deref().filter(|a| !a.trim().is_empty()) {
        parts.push(answer.trim().to_string());
    }
    // Results of read-only steps are the point of the task (file lists, text read).
    for r in &ctx.steps {
        let reads = matches!(
            r.step.action.tool(),
            "find_files" | "list_dir" | "read_text_file"
        );
        if reads
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
    let done = ctx
        .steps
        .iter()
        .filter(|r| r.status == StepStatus::Done)
        .count();
    if total > 0 {
        let line = match outcome {
            RunOutcome::Completed if verified == total => {
                format!("Verified: all {total} step(s) were checked after running.")
            }
            RunOutcome::Completed => format!(
                "Finished {total} step(s): {verified} verified, {done} ran with nothing to check against."
            ),
            RunOutcome::Failed { step_id, error } => {
                let title = ctx
                    .steps
                    .iter()
                    .find(|r| &r.step.id == step_id)
                    .map(|r| r.step.title.as_str())
                    .unwrap_or("a step");
                format!("Stopped at “{title}”: {error}. {verified} of {total} step(s) were completed and verified.")
            }
            RunOutcome::Denied { reason, .. } => format!("A step was refused: {reason}. Nothing after it ran."),
            RunOutcome::Declined { .. } => "You declined a step, so I stopped there.".to_string(),
            RunOutcome::Cancelled => format!("Stopped. {verified} of {total} step(s) were completed and verified."),
        };
        parts.push(line);
    }
    if parts.is_empty() {
        parts.push("Done.".into());
    }
    parts.join("\n\n")
}
