//! Runs a validated plan: permission check, confirmation, execution, verification, retries.

use std::time::Duration;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::context::{StepStatus, WorkflowContext};
use crate::permissions::{Policy, Risk, Verdict};
use crate::plan::{ExecutionPlan, PlanAction, PlanStep, Verification};
use crate::services::{ServiceError, Services};

/// Asks the owner. Implemented by the app (a confirmation dialog).
#[async_trait]
pub trait Confirmer: Send + Sync {
    async fn confirm(&self, step: &PlanStep, risk: Risk, reason: &str) -> bool;
}

/// What happened, for the timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecEvent {
    StepStarted {
        id: String,
        title: String,
    },
    StepFinished {
        id: String,
        status: StepStatus,
        summary: String,
    },
}

pub trait EventSink: Send + Sync {
    fn event(&self, event: ExecEvent);
}

#[derive(Debug, Clone, Copy)]
pub struct ExecConfig {
    pub step_timeout: Duration,
    pub verify_timeout: Duration,
    pub verify_poll: Duration,
    pub max_attempts: u32,
    pub retry_delay: Duration,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            step_timeout: Duration::from_secs(60),
            verify_timeout: Duration::from_secs(6),
            verify_poll: Duration::from_millis(250),
            max_attempts: 3,
            retry_delay: Duration::from_millis(400),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Failed {
        step_id: String,
        error: String,
    },
    /// The policy refused an action; nothing after it ran.
    Denied {
        step_id: String,
        reason: String,
    },
    /// The owner said no.
    Declined {
        step_id: String,
    },
    Cancelled,
}

fn trim(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n).collect::<String>())
    }
}

async fn perform(svc: &dyn Services, action: &PlanAction) -> Result<String, ServiceError> {
    match action {
        PlanAction::OpenApp { name } => svc.open_app(name).await,
        PlanAction::OpenUrl { url } => svc.open_url(url).await,
        PlanAction::FocusWindow { title } => svc.focus_window(title).await,
        PlanAction::FindFiles {
            query,
            root,
            max_results,
        } => {
            let max = (*max_results).unwrap_or(25).clamp(1, 200) as usize;
            let found = svc.find_files(query, root.as_deref(), max).await?;
            Ok(if found.is_empty() {
                "No files found".into()
            } else {
                format!("{} found:\n{}", found.len(), found.join("\n"))
            })
        }
        PlanAction::ListDir { path } => {
            let items = svc.list_dir(path).await?;
            Ok(format!("{} item(s):\n{}", items.len(), items.join("\n")))
        }
        PlanAction::ReadTextFile { path, max_bytes } => {
            let max = (*max_bytes).unwrap_or(20_000).clamp(1, 200_000) as usize;
            svc.read_text(path, max).await
        }
        PlanAction::CreateDir { path } => svc.create_dir(path).await,
        PlanAction::MovePath { from, to } => svc.move_path(from, to).await,
        PlanAction::WriteTextFile {
            path,
            content,
            overwrite,
        } => svc.write_text(path, content, *overwrite).await,
        PlanAction::MoveToTrash { paths } => svc.move_to_trash(paths).await,
        PlanAction::UiClick { target } => svc.ui_click(target).await,
        PlanAction::UiSetText { target, text } => svc.ui_set_text(target, text).await,
        PlanAction::TypeText { text } => svc.type_text(text).await,
        PlanAction::PressKeys { keys } => svc.press_keys(keys).await,
        PlanAction::WaitFor {
            condition,
            timeout_secs,
        } => {
            let limit = Duration::from_secs(u64::from((*timeout_secs).unwrap_or(10)));
            wait_until(svc, condition, limit, Duration::from_millis(250)).await?;
            Ok("Condition met".into())
        }
    }
}

/// Poll a condition until it holds or the time runs out.
async fn wait_until(
    svc: &dyn Services,
    condition: &Verification,
    limit: Duration,
    poll: Duration,
) -> Result<(), ServiceError> {
    let started = tokio::time::Instant::now();
    loop {
        if svc.check(condition).await? {
            return Ok(());
        }
        if started.elapsed() >= limit {
            return Err(ServiceError::Failed(
                "the expected state did not appear in time".into(),
            ));
        }
        tokio::time::sleep(poll).await;
    }
}

/// Execute `plan`, updating `ctx`. Stops at the first failure, refusal or cancellation.
// One argument per collaborator keeps each independently mockable in tests.
#[allow(clippy::too_many_arguments)]
pub async fn execute_plan(
    svc: &dyn Services,
    policy: &Policy,
    ctx: &mut WorkflowContext,
    plan: &ExecutionPlan,
    confirmer: &dyn Confirmer,
    sink: &dyn EventSink,
    cancel: &CancellationToken,
    cfg: ExecConfig,
) -> RunOutcome {
    ctx.adopt(plan);
    let mut outcome = RunOutcome::Completed;
    for step in &plan.steps {
        if cancel.is_cancelled() {
            outcome = RunOutcome::Cancelled;
            break;
        }
        sink.event(ExecEvent::StepStarted {
            id: step.id.clone(),
            title: step.title.clone(),
        });
        if let Some(r) = ctx.record_mut(&step.id) {
            r.status = StepStatus::Running;
        }

        match policy.check(&step.action) {
            Verdict::Deny(reason) => {
                finish(
                    ctx,
                    sink,
                    &step.id,
                    StepStatus::Failed,
                    format!("Refused: {reason}"),
                );
                ctx.errors.push(reason.clone());
                outcome = RunOutcome::Denied {
                    step_id: step.id.clone(),
                    reason,
                };
                break;
            }
            Verdict::Confirm { risk, reason } => {
                if !confirmer.confirm(step, risk, &reason).await {
                    finish(
                        ctx,
                        sink,
                        &step.id,
                        StepStatus::Failed,
                        "You declined this step".into(),
                    );
                    outcome = RunOutcome::Declined {
                        step_id: step.id.clone(),
                    };
                    break;
                }
                ctx.approvals
                    .push(format!("{}: {}", step.id, step.action.describe()));
            }
            Verdict::Allow => {}
        }

        // Run, retrying transient failures.
        let mut result: Result<String, ServiceError> = Err(ServiceError::Failed("not run".into()));
        let mut attempts = 0;
        while attempts < cfg.max_attempts.max(1) {
            attempts += 1;
            result = tokio::select! {
                _ = cancel.cancelled() => {
                    outcome = RunOutcome::Cancelled;
                    break;
                }
                r = tokio::time::timeout(cfg.step_timeout, perform(svc, &step.action)) => match r {
                    Ok(r) => r,
                    Err(_) => Err(ServiceError::Failed("the step took too long".into())),
                },
            };
            match &result {
                Err(e) if e.is_transient() && attempts < cfg.max_attempts => {
                    info!(
                        "step {} attempt {attempts} failed transiently: {e}",
                        step.id
                    );
                    tokio::time::sleep(cfg.retry_delay).await;
                }
                _ => break,
            }
        }
        if let Some(r) = ctx.record_mut(&step.id) {
            r.attempts = attempts;
        }
        if outcome == RunOutcome::Cancelled {
            finish(ctx, sink, &step.id, StepStatus::Skipped, "Stopped".into());
            break;
        }
        match result {
            Err(e) => {
                warn!("step {} failed: {e}", step.id);
                finish(ctx, sink, &step.id, StepStatus::Failed, e.to_string());
                ctx.errors.push(format!("{}: {e}", step.title));
                outcome = RunOutcome::Failed {
                    step_id: step.id.clone(),
                    error: e.to_string(),
                };
                break;
            }
            Ok(summary) => {
                let summary = trim(&summary, 2_000);
                // Verify against the live state; a plan's own claim is not enough.
                match &step.verify {
                    Some(check) => {
                        match wait_until(svc, check, cfg.verify_timeout, cfg.verify_poll).await {
                            Ok(()) => finish(ctx, sink, &step.id, StepStatus::Verified, summary),
                            Err(e) => {
                                let msg =
                                    format!("It ran, but the result could not be confirmed ({e})");
                                finish(ctx, sink, &step.id, StepStatus::Failed, msg.clone());
                                ctx.errors.push(format!("{}: {msg}", step.title));
                                outcome = RunOutcome::Failed {
                                    step_id: step.id.clone(),
                                    error: msg,
                                };
                                break;
                            }
                        }
                    }
                    None => finish(ctx, sink, &step.id, StepStatus::Done, summary),
                }
            }
        }
    }
    // Anything that never ran is skipped, not silently pending.
    for r in &mut ctx.steps {
        if r.status == StepStatus::Pending {
            r.status = StepStatus::Skipped;
        }
    }
    outcome
}

fn finish(
    ctx: &mut WorkflowContext,
    sink: &dyn EventSink,
    id: &str,
    status: StepStatus,
    summary: String,
) {
    if let Some(r) = ctx.record_mut(id) {
        r.status = status;
        r.summary = summary.clone();
    }
    sink.event(ExecEvent::StepFinished {
        id: id.to_string(),
        status,
        summary,
    });
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    use crate::context::DesktopFacts;
    use crate::plan::UiTarget;

    /// Scriptable services that record every call.
    #[derive(Default)]
    pub struct Mock {
        pub calls: Mutex<Vec<String>>,
        /// Remaining transient failures to produce for `open_app`.
        pub transient_open: Mutex<u32>,
        pub fail_tool: Mutex<Option<(String, ServiceError)>>,
        /// Conditions currently true (matched by title / path).
        pub truths: Mutex<Vec<String>>,
        pub slow: Mutex<bool>,
    }

    impl Mock {
        fn log(&self, s: impl Into<String>) {
            if let Ok(mut c) = self.calls.lock() {
                c.push(s.into());
            }
        }
        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().map(|c| c.clone()).unwrap_or_default()
        }
        pub fn make_true(&self, what: &str) {
            if let Ok(mut t) = self.truths.lock() {
                t.push(what.to_string());
            }
        }
        fn gate(&self, tool: &str) -> ServiceResult<()> {
            if let Some((t, e)) = self.fail_tool.lock().ok().and_then(|f| f.clone()) {
                if t == tool {
                    return Err(e);
                }
            }
            Ok(())
        }
    }

    use crate::services::ServiceResult;

    #[async_trait]
    impl Services for Mock {
        async fn open_app(&self, name: &str) -> ServiceResult<String> {
            self.log(format!("open_app {name}"));
            if *self.slow.lock().unwrap() {
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            {
                let mut n = self.transient_open.lock().unwrap();
                if *n > 0 {
                    *n -= 1;
                    return Err(ServiceError::Transient("not ready".into()));
                }
            }
            self.gate("open_app")?;
            Ok(format!("Opened {name}"))
        }
        async fn open_url(&self, url: &str) -> ServiceResult<String> {
            self.log(format!("open_url {url}"));
            Ok("ok".into())
        }
        async fn focus_window(&self, title: &str) -> ServiceResult<String> {
            self.log(format!("focus {title}"));
            Ok("ok".into())
        }
        async fn find_files(
            &self,
            q: &str,
            _r: Option<&str>,
            _m: usize,
        ) -> ServiceResult<Vec<String>> {
            self.log(format!("find {q}"));
            Ok(vec![r"C:\Users\me\Downloads\a.zip".into()])
        }
        async fn list_dir(&self, p: &str) -> ServiceResult<Vec<String>> {
            self.log(format!("ls {p}"));
            Ok(vec!["a".into(), "b".into()])
        }
        async fn read_text(&self, p: &str, _m: usize) -> ServiceResult<String> {
            self.log(format!("read {p}"));
            Ok("contents".into())
        }
        async fn create_dir(&self, p: &str) -> ServiceResult<String> {
            self.log(format!("mkdir {p}"));
            Ok("ok".into())
        }
        async fn move_path(&self, f: &str, t: &str) -> ServiceResult<String> {
            self.log(format!("mv {f} {t}"));
            Ok("ok".into())
        }
        async fn write_text(&self, p: &str, _c: &str, _o: bool) -> ServiceResult<String> {
            self.log(format!("write {p}"));
            Ok("ok".into())
        }
        async fn move_to_trash(&self, paths: &[String]) -> ServiceResult<String> {
            self.log(format!("trash {}", paths.join(",")));
            self.gate("move_to_trash")?;
            Ok("ok".into())
        }
        async fn ui_click(&self, t: &UiTarget) -> ServiceResult<String> {
            self.log(format!("click {:?}", t.name));
            self.gate("ui_click")?;
            Ok("ok".into())
        }
        async fn ui_set_text(&self, t: &UiTarget, text: &str) -> ServiceResult<String> {
            self.log(format!("set {:?} {text}", t.name));
            Ok("ok".into())
        }
        async fn type_text(&self, text: &str) -> ServiceResult<String> {
            self.log(format!("type {text}"));
            Ok("ok".into())
        }
        async fn press_keys(&self, keys: &str) -> ServiceResult<String> {
            self.log(format!("keys {keys}"));
            Ok("ok".into())
        }
        async fn check(&self, c: &Verification) -> ServiceResult<bool> {
            let key = match c {
                Verification::WindowExists { title } | Verification::WindowActive { title } => {
                    title.clone()
                }
                Verification::FileExists { path } | Verification::FileMissing { path } => {
                    path.clone()
                }
                Verification::UiElementExists { target } => target.name.clone().unwrap_or_default(),
                Verification::UiTextContains { text, .. } => text.clone(),
            };
            let truthy = self.truths.lock().unwrap().contains(&key);
            Ok(match c {
                Verification::FileMissing { .. } => !truthy,
                _ => truthy,
            })
        }
        async fn snapshot(&self) -> ServiceResult<DesktopFacts> {
            Ok(DesktopFacts::default())
        }
    }

    pub struct Auto(pub bool);

    #[async_trait]
    impl Confirmer for Auto {
        async fn confirm(&self, _s: &PlanStep, _r: Risk, _why: &str) -> bool {
            self.0
        }
    }

    #[derive(Default)]
    pub struct Events(pub Mutex<Vec<ExecEvent>>);

    impl EventSink for Events {
        fn event(&self, event: ExecEvent) {
            if let Ok(mut e) = self.0.lock() {
                e.push(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;
    use std::path::PathBuf;

    fn policy() -> Policy {
        Policy::new(vec![PathBuf::from(r"C:\Users\me\Downloads")], vec![])
    }

    fn step(id: &str, action: PlanAction, verify: Option<Verification>) -> PlanStep {
        PlanStep {
            id: id.into(),
            title: format!("step {id}"),
            action,
            verify,
        }
    }

    fn plan(steps: Vec<PlanStep>) -> ExecutionPlan {
        ExecutionPlan {
            objective: "o".into(),
            steps,
            notes: None,
        }
    }

    fn cfg() -> ExecConfig {
        ExecConfig {
            step_timeout: Duration::from_millis(300),
            verify_timeout: Duration::from_millis(200),
            verify_poll: Duration::from_millis(20),
            max_attempts: 3,
            retry_delay: Duration::from_millis(5),
        }
    }

    async fn run(
        svc: &Mock,
        p: &Policy,
        plan: &ExecutionPlan,
        ok: bool,
    ) -> (RunOutcome, WorkflowContext, Events) {
        let mut ctx = WorkflowContext::new("w", "o");
        let ev = Events::default();
        let out = execute_plan(
            svc,
            p,
            &mut ctx,
            plan,
            &Auto(ok),
            &ev,
            &CancellationToken::new(),
            cfg(),
        )
        .await;
        (out, ctx, ev)
    }

    #[tokio::test]
    async fn a_verified_plan_completes_in_order() {
        let svc = Mock::default();
        svc.make_true("Notepad");
        let pl = plan(vec![
            step(
                "s1",
                PlanAction::OpenApp {
                    name: "Notepad".into(),
                },
                Some(Verification::WindowExists {
                    title: "Notepad".into(),
                }),
            ),
            step("s2", PlanAction::TypeText { text: "hi".into() }, None),
        ]);
        let (out, ctx, ev) = run(&svc, &policy(), &pl, true).await;
        assert_eq!(out, RunOutcome::Completed);
        assert_eq!(svc.calls(), vec!["open_app Notepad", "type hi"]);
        assert_eq!(ctx.steps[0].status, StepStatus::Verified);
        assert_eq!(ctx.steps[1].status, StepStatus::Done);
        assert!(ctx.all_succeeded());
        let events = ev.0.lock().unwrap().clone();
        assert!(matches!(events[0], ExecEvent::StepStarted { .. }));
        assert_eq!(events.len(), 4);
    }

    #[tokio::test]
    async fn success_without_the_expected_effect_is_a_failure() {
        let svc = Mock::default(); // Notepad never "appears"
        let pl = plan(vec![
            step(
                "s1",
                PlanAction::OpenApp {
                    name: "Notepad".into(),
                },
                Some(Verification::WindowExists {
                    title: "Notepad".into(),
                }),
            ),
            step("s2", PlanAction::TypeText { text: "hi".into() }, None),
        ]);
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert!(matches!(out, RunOutcome::Failed { ref step_id, .. } if step_id == "s1"));
        assert_eq!(ctx.steps[0].status, StepStatus::Failed);
        assert!(ctx.steps[0].summary.contains("could not be confirmed"));
        assert_eq!(ctx.steps[1].status, StepStatus::Skipped);
        assert_eq!(
            svc.calls(),
            vec!["open_app Notepad"],
            "later steps never ran"
        );
    }

    #[tokio::test]
    async fn transient_failures_are_retried() {
        let svc = Mock::default();
        *svc.transient_open.lock().unwrap() = 2;
        let pl = plan(vec![step(
            "s1",
            PlanAction::OpenApp { name: "X".into() },
            None,
        )]);
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert_eq!(out, RunOutcome::Completed);
        assert_eq!(ctx.steps[0].attempts, 3);
        assert_eq!(svc.calls().len(), 3);

        let svc = Mock::default();
        *svc.transient_open.lock().unwrap() = 9;
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert!(matches!(out, RunOutcome::Failed { .. }));
        assert_eq!(ctx.steps[0].attempts, 3);
    }

    #[tokio::test]
    async fn permanent_failures_are_not_retried() {
        let svc = Mock::default();
        *svc.fail_tool.lock().unwrap() = Some((
            "ui_click".into(),
            ServiceError::Failed("no such button".into()),
        ));
        let pl = plan(vec![step(
            "s1",
            PlanAction::UiClick {
                target: crate::plan::UiTarget {
                    name: Some("Save".into()),
                    ..Default::default()
                },
            },
            None,
        )]);
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert!(matches!(out, RunOutcome::Failed { ref error, .. } if error == "no such button"));
        assert_eq!(ctx.steps[0].attempts, 1);
        assert_eq!(ctx.errors.len(), 1);
    }

    #[tokio::test]
    async fn refused_actions_never_reach_the_services() {
        let svc = Mock::default();
        let pl = plan(vec![
            step(
                "s1",
                PlanAction::MoveToTrash {
                    paths: vec![r"C:\Windows\System32\x.dll".into()],
                },
                None,
            ),
            step(
                "s2",
                PlanAction::TypeText {
                    text: "after".into(),
                },
                None,
            ),
        ]);
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert!(matches!(out, RunOutcome::Denied { ref step_id, .. } if step_id == "s1"));
        assert!(svc.calls().is_empty());
        assert!(ctx.steps[0].summary.starts_with("Refused"));
        assert_eq!(ctx.steps[1].status, StepStatus::Skipped);
    }

    #[tokio::test]
    async fn destructive_steps_wait_for_a_yes_and_stop_on_a_no() {
        let svc = Mock::default();
        let pl = plan(vec![step(
            "s1",
            PlanAction::MoveToTrash {
                paths: vec![r"C:\Users\me\Downloads\a.zip".into()],
            },
            None,
        )]);
        let (out, _, _) = run(&svc, &policy(), &pl, false).await;
        assert_eq!(
            out,
            RunOutcome::Declined {
                step_id: "s1".into()
            }
        );
        assert!(svc.calls().is_empty(), "declined means not executed");

        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert_eq!(out, RunOutcome::Completed);
        assert_eq!(svc.calls(), vec![r"trash C:\Users\me\Downloads\a.zip"]);
        assert_eq!(ctx.approvals.len(), 1);
    }

    #[tokio::test]
    async fn cancelling_stops_a_running_step() {
        let svc = Mock::default();
        *svc.slow.lock().unwrap() = true;
        let pl = plan(vec![
            step("s1", PlanAction::OpenApp { name: "X".into() }, None),
            step("s2", PlanAction::TypeText { text: "a".into() }, None),
        ]);
        let mut ctx = WorkflowContext::new("w", "o");
        let cancel = CancellationToken::new();
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            c2.cancel();
        });
        let mut slow_cfg = cfg();
        slow_cfg.step_timeout = Duration::from_secs(10);
        let ev = Events::default();
        let out = execute_plan(
            &svc,
            &policy(),
            &mut ctx,
            &pl,
            &Auto(true),
            &ev,
            &cancel,
            slow_cfg,
        )
        .await;
        assert_eq!(out, RunOutcome::Cancelled);
        assert_eq!(ctx.steps[1].status, StepStatus::Skipped);
        assert_eq!(svc.calls().len(), 1);
    }

    #[tokio::test]
    async fn a_step_that_hangs_times_out() {
        let svc = Mock::default();
        *svc.slow.lock().unwrap() = true;
        let pl = plan(vec![step(
            "s1",
            PlanAction::OpenApp { name: "X".into() },
            None,
        )]);
        let (out, _, _) = run(&svc, &policy(), &pl, true).await;
        assert!(matches!(out, RunOutcome::Failed { ref error, .. } if error.contains("too long")));
    }

    #[tokio::test]
    async fn wait_for_polls_until_the_state_appears() {
        let svc = std::sync::Arc::new(Mock::default());
        let s2 = svc.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            s2.make_true("Dialog");
        });
        let pl = plan(vec![step(
            "s1",
            PlanAction::WaitFor {
                condition: Verification::WindowExists {
                    title: "Dialog".into(),
                },
                timeout_secs: Some(2),
            },
            None,
        )]);
        let mut ctx = WorkflowContext::new("w", "o");
        let out = execute_plan(
            &*svc,
            &policy(),
            &mut ctx,
            &pl,
            &Auto(true),
            &Events::default(),
            &CancellationToken::new(),
            cfg(),
        )
        .await;
        assert_eq!(out, RunOutcome::Completed);
    }

    #[tokio::test]
    async fn read_only_results_are_kept_in_the_summary() {
        let svc = Mock::default();
        let pl = plan(vec![step(
            "s1",
            PlanAction::FindFiles {
                query: "zip".into(),
                root: None,
                max_results: None,
            },
            None,
        )]);
        let (out, ctx, _) = run(&svc, &policy(), &pl, true).await;
        assert_eq!(out, RunOutcome::Completed);
        assert!(ctx.steps[0].summary.contains("a.zip"));
    }
}
