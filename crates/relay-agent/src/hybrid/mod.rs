//! The hybrid engine: routes a task, gets a plan from the local model or a cloud provider,
//! validates it, and runs it through permissioned native services, verifying every effect.
//!
//! AI output is only ever a proposal. Rust parses it into typed actions, checks permissions,
//! asks the owner where required, executes natively, and verifies the result afterwards.

mod chat;
mod flow;
pub mod prompts;
pub mod record;
mod web;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use relay_ai::{AiError, AiEvent, AiProvider, Finish, RunRequest, SessionMode, Timeouts};
use relay_mcp::{ApprovalRequest, Approver, Decision, KillSwitch};
use relay_orchestrator::context::{
    scope_for_cloud, DesktopFacts, ScopedContext, StepStatus, WorkflowContext,
};
use relay_orchestrator::executor::{
    execute_plan, Confirmer, EventSink as ExecSink, ExecConfig, ExecEvent, RunOutcome,
};
use relay_orchestrator::permissions::{ConfirmLevel, Policy, PrivacyMode, Risk, SharePolicy};
use relay_orchestrator::plan::{parse_plan, plan_schema, ExecutionPlan, PlanStep};
use relay_orchestrator::router::{route, Abilities, ProviderKind, Strategy};
use relay_orchestrator::Services;
use relay_protocol::{
    ErrorKind, PlanStepInfo, RunId, RunOutcome as UiOutcome, SharedInfo, UiEvent,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::bridge::{ConfirmBridge, EventSink};
use crate::browser::BrowserDriver;
use crate::context::ConvStore;
use crate::local::{ChatMessage, LocalProvider};
use prompts::{cloud_user_prompt, local_user_prompt, PLANNER_SYSTEM};
pub use record::{NullWorkflowStore, StepOut, UsedTag, WorkflowRecord, WorkflowStore};
pub use relay_orchestrator::report::final_message;

const MAX_REPLANS: u32 = 2;
const HISTORY_TURNS: usize = 6;
const PLAN_TIMEOUTS: Timeouts = Timeouts {
    total: Duration::from_secs(240),
    idle: Duration::from_secs(120),
};

/// Per-provider choices the owner made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderOpts {
    pub model: Option<String>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HybridSettings {
    pub privacy: PrivacyMode,
    pub default_provider: ProviderKind,
    pub confirm: ConfirmLevel,
    pub share: SharePolicy,
    pub allowed_roots: Vec<PathBuf>,
    pub protected_roots: Vec<PathBuf>,
    pub disabled_tools: HashSet<String>,
    pub provider_opts: HashMap<ProviderKind, ProviderOpts>,
}

impl HybridSettings {
    pub fn policy(&self) -> Policy {
        let mut p = Policy::new(self.allowed_roots.clone(), self.protected_roots.clone());
        p.privacy = self.privacy;
        p.confirm = self.confirm;
        p.share = self.share;
        p.disabled_tools = self.disabled_tools.clone();
        p
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SubmitError {
    #[error("Relay is already working on a task. Stop it first or wait for it to finish.")]
    Busy,
    #[error("There is nothing to do: the message is empty.")]
    Empty,
}

/// What a planner produced.
struct Planned {
    plan: ExecutionPlan,
    source: String,
    shared: Vec<SharedInfo>,
    bytes: usize,
}

pub struct HybridParts {
    pub services: Arc<dyn Services>,
    pub local: Arc<LocalProvider>,
    pub claude: Arc<dyn AiProvider>,
    pub codex: Arc<dyn AiProvider>,
    pub bridge: Arc<ConfirmBridge>,
    pub kill: KillSwitch,
    pub conv_store: Arc<dyn ConvStore>,
    pub workflows: Arc<dyn WorkflowStore>,
    pub work_dir: PathBuf,
    pub settings: HybridSettings,
    pub sink: EventSink,
    /// Browser control for website tasks.
    pub browser: Arc<dyn BrowserDriver>,
}

pub struct Hybrid {
    services: Arc<dyn Services>,
    local: Arc<LocalProvider>,
    claude: Arc<dyn AiProvider>,
    codex: Arc<dyn AiProvider>,
    bridge: Arc<ConfirmBridge>,
    kill: KillSwitch,
    conv_store: Arc<dyn ConvStore>,
    workflows: Arc<dyn WorkflowStore>,
    work_dir: PathBuf,
    sink: EventSink,
    settings: Mutex<HybridSettings>,
    active: Mutex<Option<CancellationToken>>,
    next_run: Mutex<RunId>,
    statuses: Mutex<HashMap<ProviderKind, (std::time::Instant, relay_ai::ProviderStatus)>>,
    exec_config: Mutex<ExecConfig>,
    browser: Arc<dyn BrowserDriver>,
    /// Labels already reported with a `used` event in the current run, and that run's id.
    used_labels: Mutex<(RunId, HashSet<String>)>,
}

/// What a cloud request is for; it decides the default model and effort when the owner chose none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// A whole plan for a task on this PC.
    Plan,
    /// One next step of a browser task (many per task, so it must be quick).
    Step,
    /// A written answer (quality matters most).
    Answer,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// The name of this PC's own tools, for tags.
fn native_label() -> &'static str {
    if cfg!(windows) {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "This PC"
    }
}

/// Most pages a task may open on its own; more than this is a runaway plan, not a task.
const MAX_OPENS_PER_PLAN: usize = 2;

/// Drop steps that repeat an effect already achieved in this task, and cap how many pages one plan
/// may open. Replanning after a failure used to re-run the whole plan and open the same tabs again.
fn prune_repeats(mut plan: ExecutionPlan, done: &HashSet<String>) -> ExecutionPlan {
    let mut opens = 0;
    let mut seen = HashSet::new();
    plan.steps.retain(|s| {
        let key = serde_json::to_string(&s.action).unwrap_or_default();
        if done.contains(&key) || !seen.insert(key) {
            return matches!(
                s.action,
                relay_orchestrator::PlanAction::WaitFor { .. }
                    | relay_orchestrator::PlanAction::ListDir { .. }
                    | relay_orchestrator::PlanAction::FindFiles { .. }
            );
        }
        if matches!(s.action, relay_orchestrator::PlanAction::OpenUrl { .. }) {
            opens += 1;
            return opens <= MAX_OPENS_PER_PLAN;
        }
        true
    });
    plan
}

/// "qwen3:14b" -> "Qwen3 14B" for tags.
fn pretty_model(name: &str) -> String {
    let (base, size) = name.split_once(':').unwrap_or((name, ""));
    let mut base = base.to_string();
    if let Some(first) = base.get(..1) {
        base = format!("{}{}", first.to_uppercase(), &base[1..]);
    }
    if size.is_empty() || size == "latest" {
        base
    } else {
        format!("{base} {}", size.to_uppercase())
    }
}

/// Errors worth one quiet retry: sign-in refresh races, overload, dropped connections.
fn is_transient(e: &AiError) -> bool {
    let t = e.to_string().to_lowercase();
    [
        "transient",
        "oauth",
        "refresh",
        "overloaded",
        "rate limit",
        "529",
        "503",
        "connection reset",
        "econnreset",
    ]
    .iter()
    .any(|w| t.contains(w))
}

fn risk_label(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::Sensitive => "sensitive",
        Risk::Destructive => "destructive",
    }
}

fn status_label(s: StepStatus) -> &'static str {
    match s {
        StepStatus::Pending => "pending",
        StepStatus::Running => "running",
        StepStatus::Verified => "verified",
        StepStatus::Done => "done",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}

pub fn step_infos(plan: &ExecutionPlan) -> Vec<PlanStepInfo> {
    plan.steps
        .iter()
        .map(|s| PlanStepInfo {
            id: s.id.clone(),
            title: s.title.clone(),
            tool: s.action.tool().to_string(),
            risk: risk_label(Policy::risk_of(&s.action)).to_string(),
            via: Some("native".into()),
            server: None,
            sensitive_data: relay_orchestrator::sensitivity::action_is_sensitive(&s.action),
        })
        .collect()
}

struct UiSink {
    run_id: RunId,
    sink: EventSink,
}

impl ExecSink for UiSink {
    fn event(&self, event: ExecEvent) {
        match event {
            ExecEvent::StepStarted { id, .. } => (self.sink)(UiEvent::StepStarted {
                run_id: self.run_id,
                step_id: id,
            }),
            ExecEvent::StepFinished {
                id,
                status,
                summary,
            } => (self.sink)(UiEvent::StepFinished {
                run_id: self.run_id,
                step_id: id,
                status: status_label(status).to_string(),
                summary,
            }),
        }
    }
}

struct BridgeConfirmer {
    run_id: RunId,
    bridge: Arc<ConfirmBridge>,
}

#[async_trait]
impl Confirmer for BridgeConfirmer {
    async fn confirm(&self, step: &PlanStep, risk: Risk, reason: &str) -> bool {
        let decision = self
            .bridge
            .ask(ApprovalRequest {
                run_id: self.run_id,
                title: step.action.describe(),
                detail: format!("{reason}\nStep: {}", step.title),
                risk: match risk {
                    Risk::Destructive => relay_protocol::Risk::Destructive,
                    _ => relay_protocol::Risk::Reversible,
                },
            })
            .await;
        decision == Decision::Approved
    }
}

impl Hybrid {
    pub fn new(parts: HybridParts) -> Arc<Self> {
        Arc::new(Self {
            services: parts.services,
            local: parts.local,
            claude: parts.claude,
            codex: parts.codex,
            bridge: parts.bridge,
            kill: parts.kill,
            conv_store: parts.conv_store,
            workflows: parts.workflows,
            work_dir: parts.work_dir,
            sink: parts.sink,
            settings: Mutex::new(parts.settings),
            active: Mutex::new(None),
            next_run: Mutex::new(1),
            statuses: Mutex::new(HashMap::new()),
            exec_config: Mutex::new(ExecConfig::default()),
            browser: parts.browser,
            used_labels: Mutex::new((0, HashSet::new())),
        })
    }

    fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
        m.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn settings(&self) -> HybridSettings {
        Self::lock(&self.settings).clone()
    }

    pub fn update_settings(&self, change: impl FnOnce(&mut HybridSettings)) {
        change(&mut Self::lock(&self.settings));
    }

    /// Timing knobs for step execution (tests shorten them).
    pub fn set_exec_config(&self, cfg: ExecConfig) {
        *Self::lock(&self.exec_config) = cfg;
    }

    pub fn is_busy(&self) -> bool {
        Self::lock(&self.active).is_some()
    }

    pub fn provider(&self, kind: ProviderKind) -> &Arc<dyn AiProvider> {
        match kind {
            ProviderKind::Claude => &self.claude,
            ProviderKind::Codex => &self.codex,
        }
    }

    pub fn local(&self) -> &Arc<LocalProvider> {
        &self.local
    }

    pub fn browser(&self) -> &Arc<dyn BrowserDriver> {
        &self.browser
    }

    /// Report, once per run, that a component did part of the work (drives the tags in the UI).
    fn used(&self, kind: &str, label: &str, detail: Option<String>) {
        let run_id = {
            let mut g = Self::lock(&self.used_labels);
            let key = format!("{kind}:{label}");
            if !g.1.insert(key) {
                return;
            }
            g.0
        };
        self.emit(UiEvent::Used {
            run_id,
            kind: kind.to_string(),
            label: label.to_string(),
            detail,
        });
    }

    /// Tags reported in the current run, for the history record.
    fn used_tags(&self) -> Vec<UsedTag> {
        let mut tags: Vec<UsedTag> = Self::lock(&self.used_labels)
            .1
            .iter()
            .filter_map(|k| k.split_once(':'))
            .map(|(kind, label)| UsedTag {
                kind: kind.to_string(),
                label: label.to_string(),
            })
            .collect();
        tags.sort_by(|a, b| (&a.kind, &a.label).cmp(&(&b.kind, &b.label)));
        tags
    }

    /// The local model to use, or why there is none.
    async fn local_model(&self) -> Result<String, String> {
        let model = self
            .local
            .overview()
            .await
            .active
            .map(|m| m.name)
            .ok_or_else(|| "the local AI has no model ready".to_string())?;
        self.used("local_ai", &pretty_model(&model), Some(model.clone()));
        Ok(model)
    }

    /// The model and effort for a cloud request: the owner's choice, else a sensible default for the purpose.
    /// Planning and browser steps default to a fast model at low effort; written answers to a stronger one.
    fn provider_choice(
        &self,
        kind: ProviderKind,
        purpose: Purpose,
    ) -> (Option<String>, Option<String>) {
        let opts = self
            .settings()
            .provider_opts
            .get(&kind)
            .cloned()
            .unwrap_or_default();
        let (model, effort) = match (kind, purpose) {
            (ProviderKind::Claude, Purpose::Step) => (Some("haiku"), Some("low")),
            (ProviderKind::Claude, Purpose::Plan) => (Some("sonnet"), Some("low")),
            (ProviderKind::Claude, Purpose::Answer) => (Some("sonnet"), Some("medium")),
            (ProviderKind::Codex, Purpose::Answer) => (None, Some("medium")),
            (ProviderKind::Codex, _) => (None, Some("low")),
        };
        (
            opts.model.or(model.map(str::to_string)),
            opts.reasoning.or(effort.map(str::to_string)),
        )
    }

    /// Stop the current task right now: cancel planning and execution, deny open prompts.
    pub fn stop(&self) {
        self.kill.trip();
        self.bridge.deny_all();
        if let Some(token) = Self::lock(&self.active).as_ref() {
            token.cancel();
        }
    }

    pub fn answer(&self, answer: relay_protocol::ConfirmAnswer) -> bool {
        self.bridge.answer(answer)
    }

    /// Sign-in and install state of a provider, cached for 30 seconds unless `force`.
    pub async fn provider_status(
        &self,
        kind: ProviderKind,
        force: bool,
    ) -> relay_ai::ProviderStatus {
        if !force {
            if let Some((at, st)) = Self::lock(&self.statuses).get(&kind) {
                if at.elapsed() < Duration::from_secs(30) {
                    return st.clone();
                }
            }
        }
        let status = self.provider(kind).status().await;
        Self::lock(&self.statuses).insert(kind, (std::time::Instant::now(), status.clone()));
        status
    }

    async fn abilities(&self) -> Abilities {
        let settings = self.settings();
        let (overview, claude, codex, browser) = tokio::join!(
            self.local.overview(),
            self.provider_status(ProviderKind::Claude, false),
            self.provider_status(ProviderKind::Codex, false),
            self.browser.status()
        );
        let local = overview.ability();
        let ready = |k: ProviderKind| match k {
            ProviderKind::Claude => claude.installed && claude.logged_in,
            ProviderKind::Codex => codex.installed && codex.logged_in,
        };
        let default = settings.default_provider;
        let other = match default {
            ProviderKind::Claude => ProviderKind::Codex,
            ProviderKind::Codex => ProviderKind::Claude,
        };
        let (provider, fallback) = match (ready(default), ready(other)) {
            (true, true) => (Some(default), Some(other)),
            (true, false) => (Some(default), None),
            (false, true) => (Some(other), None),
            (false, false) => (None, None),
        };
        Abilities {
            local_ready: local.ready,
            local_sees_screen: local.vision,
            provider,
            fallback,
            browser_ready: browser.available && !settings.disabled_tools.contains("browser"),
        }
    }

    fn emit(&self, event: UiEvent) {
        (self.sink)(event);
    }

    /// Start a task. Returns at once; progress arrives as events.
    pub fn submit_in(
        self: &Arc<Self>,
        conv_id: &str,
        text: &str,
        attachments: &[String],
    ) -> Result<RunId, SubmitError> {
        let mut task = text.trim().to_string();
        if task.is_empty() {
            return Err(SubmitError::Empty);
        }
        if !attachments.is_empty() {
            task.push_str("\n\nAttached files:");
            for p in attachments {
                task.push_str(&format!("\n- {p}"));
            }
        }
        let (run_id, cancel) = {
            let mut active = Self::lock(&self.active);
            if active.is_some() {
                return Err(SubmitError::Busy);
            }
            let mut next = Self::lock(&self.next_run);
            let id = *next;
            *next += 1;
            let token = CancellationToken::new();
            *active = Some(token.clone());
            (id, token)
        };
        self.kill.rearm();
        let conv = conv_id.to_string();
        self.emit(UiEvent::RunStarted {
            run_id,
            conv_id: conv.clone(),
        });
        let this = self.clone();
        tokio::spawn(async move {
            this.run_task(run_id, conv, task, cancel).await;
        });
        Ok(run_id)
    }

    // ---- planning ----

    async fn plan_locally(
        &self,
        task: &str,
        history: &[(String, String)],
        failure: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Planned, String> {
        let model = self.local_model().await?;
        let facts = self.services.snapshot().await.unwrap_or_default();
        let settings = self.settings();
        let allowed: Vec<String> = settings
            .allowed_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let mut desktop = String::new();
        if !facts.window_titles.is_empty() {
            desktop.push_str("Open windows:\n");
            for t in facts.window_titles.iter().take(20) {
                let active = facts.active_window.as_deref() == Some(t.as_str());
                desktop.push_str(&format!("- {t}{}\n", if active { " (active)" } else { "" }));
            }
        }
        if let Some(outline) = &facts.ui_outline {
            desktop.push_str(&format!("\nControls in the active window:\n{outline}\n"));
        }
        let client = self.local.client();
        let schema = plan_schema();
        let mut user = local_user_prompt(task, &desktop, &allowed, history, failure);
        let mut last_error = String::new();
        for attempt in 0..2 {
            let messages = vec![
                ChatMessage::new("system", PLANNER_SYSTEM),
                ChatMessage::new("user", user.clone()),
            ];
            let reply = tokio::select! {
                _ = cancel.cancelled() => return Err("stopped".into()),
                r = client.chat_json(&model, &messages, &schema, Duration::from_secs(180)) => r,
            };
            match reply {
                Err(e) => return Err(e.to_string()),
                Ok(text) => match parse_plan(&text) {
                    Ok(plan) => {
                        return Ok(Planned {
                            plan,
                            source: "local".into(),
                            shared: Vec::new(),
                            bytes: 0,
                        })
                    }
                    Err(e) => {
                        last_error = e.to_string();
                        if attempt == 0 {
                            user.push_str(&format!(
                                "\n\nYour previous reply was rejected: {e}. Reply with only the JSON object in the required shape."
                            ));
                        }
                    }
                },
            }
        }
        Err(last_error)
    }

    async fn ask_provider(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        prompt: String,
        with_schema: bool,
        cancel: &CancellationToken,
    ) -> Result<String, AiError> {
        self.ask_provider_with(
            kind,
            run_id,
            prompt,
            PLANNER_SYSTEM,
            with_schema.then(plan_schema),
            Purpose::Plan,
            cancel,
        )
        .await
    }

    /// One tool-less request to a cloud provider; returns its final text. A transient failure (for
    /// example the CLI refreshing its sign-in at the same moment as another process) is retried once.
    #[allow(clippy::too_many_arguments)]
    async fn ask_provider_with(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        prompt: String,
        system: &str,
        schema: Option<serde_json::Value>,
        purpose: Purpose,
        cancel: &CancellationToken,
    ) -> Result<String, AiError> {
        let mut last = None;
        for attempt in 0..2 {
            match self
                .ask_provider_once(
                    kind,
                    run_id,
                    prompt.clone(),
                    system,
                    schema.clone(),
                    purpose,
                    cancel,
                )
                .await
            {
                Ok(text) => {
                    self.mark_cloud_used(kind, purpose);
                    return Ok(text);
                }
                Err(e) if attempt == 0 && is_transient(&e) && !cancel.is_cancelled() => {
                    info!("{} hit a transient error, retrying: {e}", kind.label());
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    last = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or(AiError::Cancelled))
    }

    fn mark_cloud_used(&self, kind: ProviderKind, purpose: Purpose) {
        let (model, _) = self.provider_choice(kind, purpose);
        self.used("cloud_ai", kind.label(), model);
    }

    #[allow(clippy::too_many_arguments)]
    async fn ask_provider_once(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        prompt: String,
        system: &str,
        schema: Option<serde_json::Value>,
        purpose: Purpose,
        cancel: &CancellationToken,
    ) -> Result<String, AiError> {
        let (model, reasoning) = self.provider_choice(kind, purpose);
        let request = RunRequest {
            run_id,
            prompt,
            session: SessionMode::New(uuid::Uuid::new_v4().to_string()),
            work_dir: self.work_dir.clone(),
            mcp_servers: json!({}),
            permission_prompt_tool: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            system_prompt_append: Some(system.to_string()),
            model,
            reasoning,
            output_schema: schema,
            plan_only: true,
            timeouts: PLAN_TIMEOUTS,
        };
        let mut handle = self.provider(kind).run(request).await?;
        let mut cancelled = false;
        loop {
            tokio::select! {
                _ = cancel.cancelled(), if !cancelled => {
                    cancelled = true;
                    handle.cancel();
                }
                event = handle.events.recv() => match event {
                    None => return Err(AiError::Io("the provider ended unexpectedly".into())),
                    Some(AiEvent::Finished(Finish::Completed { result, .. })) => return Ok(result),
                    Some(AiEvent::Finished(Finish::Cancelled)) => return Err(AiError::Cancelled),
                    Some(AiEvent::Finished(Finish::TimedOut(k))) => return Err(AiError::Timeout(k)),
                    Some(AiEvent::Finished(Finish::Failed(e))) => return Err(e),
                    Some(AiEvent::Finished(Finish::Escalate { reason, .. })) => {
                        return Err(AiError::Cli { message: reason, exit_code: None })
                    }
                    Some(_) => {}
                }
            }
        }
    }

    async fn plan_in_cloud(
        &self,
        kind: ProviderKind,
        run_id: RunId,
        task: &str,
        history: &[(String, String)],
        failure: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Planned, AiError> {
        let settings = self.settings();
        let facts = if settings.share.window_titles
            || settings.share.ui_tree
            || settings.share.file_names
        {
            self.services.snapshot().await.unwrap_or_default()
        } else {
            DesktopFacts::default()
        };
        let scoped: ScopedContext = scope_for_cloud(&facts, &settings.share);
        let allowed: Vec<String> = settings
            .allowed_roots
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let prompt = cloud_user_prompt(task, &scoped, &allowed, history, failure);
        let shared: Vec<SharedInfo> = scoped
            .shared
            .iter()
            .map(|s| SharedInfo {
                category: s.category.clone(),
                items: s.items,
            })
            .collect();
        // The ledger is shown before the request leaves the PC.
        self.emit(UiEvent::ContextShared {
            run_id,
            provider: kind.as_str().to_string(),
            categories: shared.clone(),
            bytes: prompt.len(),
        });
        let mut text = match self
            .ask_provider(kind, run_id, prompt.clone(), true, cancel)
            .await
        {
            // A provider that rejects the schema is retried without it; the plan is still validated.
            Err(AiError::Cli { ref message, .. }) if message.to_lowercase().contains("schema") => {
                self.ask_provider(kind, run_id, prompt.clone(), false, cancel)
                    .await?
            }
            other => other?,
        };
        let mut parsed = parse_plan(&text);
        if let Err(e) = &parsed {
            let retry = format!("{prompt}\n\nYour previous reply was rejected: {e}. Reply with only the JSON object in the required shape.");
            text = self.ask_provider(kind, run_id, retry, true, cancel).await?;
            parsed = parse_plan(&text);
        }
        let plan = parsed.map_err(|e| AiError::Cli {
            message: e.to_string(),
            exit_code: None,
        })?;
        Ok(Planned {
            plan,
            source: kind.as_str().to_string(),
            shared,
            bytes: prompt.len(),
        })
    }

    // ---- the run ----

    async fn run_task(
        self: Arc<Self>,
        run_id: RunId,
        conv_id: String,
        task: String,
        cancel: CancellationToken,
    ) {
        let ctx = self.conv_store.load(&conv_id);
        let settings = self.settings();
        let abilities = self.abilities().await;
        let decision = route(&task, settings.privacy, abilities);
        *Self::lock(&self.used_labels) = (run_id, HashSet::new());
        let (strategy_name, provider_name) = match &decision.strategy {
            Strategy::Native(_) => ("native", None),
            Strategy::LocalPlan => ("local_plan", None),
            Strategy::CloudPlan(p) => ("cloud_plan", Some(p.as_str().to_string())),
            Strategy::CloudAgent(p) => ("cloud_agent", Some(p.as_str().to_string())),
            Strategy::ChatLocal => ("chat_local", None),
            Strategy::ChatCloud(p) => ("chat_cloud", Some(p.as_str().to_string())),
            Strategy::WebLocal => ("web_local", None),
            Strategy::WebCloud(p) => ("web_cloud", Some(p.as_str().to_string())),
            Strategy::Unavailable(_) => ("unavailable", None),
        };
        self.emit(UiEvent::Route {
            run_id,
            strategy: strategy_name.into(),
            provider: provider_name.clone(),
            reason: decision.reason.clone(),
            intent: Some(decision.intent.as_str().to_string()),
        });
        if matches!(
            decision.strategy,
            Strategy::ChatLocal
                | Strategy::ChatCloud(_)
                | Strategy::WebLocal
                | Strategy::WebCloud(_)
        ) {
            self.run_conversational(
                run_id,
                &conv_id,
                &task,
                &ctx.history,
                decision.strategy.clone(),
                &cancel,
            )
            .await;
            return;
        }

        let started = now_ms();
        let mut record = WorkflowRecord {
            id: uuid::Uuid::new_v4().simple().to_string(),
            conv_id: Some(conv_id.clone()),
            objective: task.chars().take(300).collect(),
            strategy: strategy_name.into(),
            provider: provider_name,
            outcome: "failed".into(),
            steps_total: 0,
            steps_verified: 0,
            created_at: started,
            updated_at: started,
            plan_source: String::new(),
            steps: Vec::new(),
            shared: Vec::new(),
            shared_bytes: 0,
            approvals: Vec::new(),
            errors: Vec::new(),
            notes: None,
            intent: None,
            used: Vec::new(),
        };

        let mut final_text = String::new();
        let mut ui_outcome = UiOutcome::Failed;
        let mut wf = WorkflowContext::new(record.id.clone(), task.clone());
        let mut last_plan: Option<ExecutionPlan> = None;
        let mut strategy = decision.strategy.clone();
        let mut failure: Option<String> = None;
        // Effects that already happened in this task (a replan must not open the same page again).
        let mut done_effects: HashSet<String> = HashSet::new();
        let policy = settings.policy();
        let confirmer = BridgeConfirmer {
            run_id,
            bridge: self.bridge.clone(),
        };
        let sink = UiSink {
            run_id,
            sink: self.sink.clone(),
        };
        let exec_cfg = *Self::lock(&self.exec_config);

        'attempts: for attempt in 0..=MAX_REPLANS {
            if cancel.is_cancelled() {
                ui_outcome = UiOutcome::Cancelled;
                record.outcome = "cancelled".into();
                break;
            }
            // ---- plan ----
            let planned: Result<Planned, String> = match &strategy {
                Strategy::Native(step) => Ok(Planned {
                    plan: ExecutionPlan {
                        objective: task.clone(),
                        steps: vec![(**step).clone()],
                        notes: None,
                        answer: None,
                    },
                    source: "native".into(),
                    shared: Vec::new(),
                    bytes: 0,
                }),
                Strategy::LocalPlan => {
                    self.plan_locally(&task, &ctx.history, failure.as_deref(), &cancel)
                        .await
                }
                Strategy::CloudPlan(kind) | Strategy::CloudAgent(kind) => {
                    match self
                        .plan_in_cloud(
                            *kind,
                            run_id,
                            &task,
                            &ctx.history,
                            failure.as_deref(),
                            &cancel,
                        )
                        .await
                    {
                        Ok(p) => Ok(p),
                        Err(AiError::Cancelled) => Err("stopped".into()),
                        Err(first) => {
                            warn!("{} could not plan: {first}", kind.label());
                            let (_, message) = crate::agent::error_to_ui(&first);
                            self.emit(UiEvent::Notice {
                                text: format!("{} could not plan this: {message}", kind.label()),
                            });
                            // Fall back: the other provider (if the owner allows cloud), else the local AI.
                            let other = match kind {
                                ProviderKind::Claude => ProviderKind::Codex,
                                ProviderKind::Codex => ProviderKind::Claude,
                            };
                            let other_ready = settings.privacy != PrivacyMode::LocalOnly
                                && abilities.fallback == Some(other);
                            if other_ready && !cancel.is_cancelled() {
                                self.emit(UiEvent::Notice {
                                    text: format!("Trying {} instead.", other.label()),
                                });
                                match self
                                    .plan_in_cloud(
                                        other,
                                        run_id,
                                        &task,
                                        &ctx.history,
                                        failure.as_deref(),
                                        &cancel,
                                    )
                                    .await
                                {
                                    Ok(p) => Ok(p),
                                    Err(e) => Err(e.to_string()),
                                }
                            } else if abilities.local_ready && !cancel.is_cancelled() {
                                self.emit(UiEvent::Notice {
                                    text: "Using the local AI instead.".into(),
                                });
                                self.plan_locally(&task, &ctx.history, failure.as_deref(), &cancel)
                                    .await
                            } else {
                                Err(message)
                            }
                        }
                    }
                }
                Strategy::Unavailable(why) => Err(why.clone()),
                // Answers and browser tasks never reach the plan-and-execute loop.
                Strategy::ChatLocal
                | Strategy::ChatCloud(_)
                | Strategy::WebLocal
                | Strategy::WebCloud(_) => Err("internal routing error".into()),
            };
            let planned = match planned {
                Ok(p) => p,
                Err(why) => {
                    if cancel.is_cancelled() {
                        ui_outcome = UiOutcome::Cancelled;
                        record.outcome = "cancelled".into();
                    } else {
                        record.outcome = if matches!(strategy, Strategy::Unavailable(_)) {
                            "unavailable"
                        } else {
                            "failed"
                        }
                        .into();
                        record.errors.push(why.clone());
                        self.emit(UiEvent::Error {
                            run_id: Some(run_id),
                            kind: ErrorKind::Cli,
                            message: why.clone(),
                        });
                        final_text = format!("I could not make a plan: {why}");
                    }
                    break 'attempts;
                }
            };
            record.plan_source = planned.source.clone();
            record.shared.extend(planned.shared.clone());
            record.shared_bytes += planned.bytes;
            record.notes = planned.plan.notes.clone();
            self.emit(UiEvent::PlanProposed {
                run_id,
                source: planned.source.clone(),
                objective: planned.plan.objective.clone(),
                steps: step_infos(&planned.plan),
                notes: planned.plan.notes.clone(),
            });
            let plan = prune_repeats(planned.plan, &done_effects);
            if plan.steps.is_empty() {
                final_text = final_message(
                    &plan,
                    &WorkflowContext::new("x", ""),
                    &RunOutcome::Completed,
                );
                record.outcome = "completed".into();
                ui_outcome = UiOutcome::Completed;
                last_plan = Some(plan);
                break;
            }

            // ---- execute ----
            self.used("native", native_label(), None);
            wf = WorkflowContext::new(record.id.clone(), task.clone());
            let env_cancel = cancel.clone();
            let outcome = execute_plan(
                self.services.as_ref(),
                &policy,
                &mut wf,
                &plan,
                &confirmer,
                &sink,
                &env_cancel,
                exec_cfg,
            )
            .await;
            record.steps = wf
                .steps
                .iter()
                .map(|r| StepOut {
                    id: r.step.id.clone(),
                    title: r.step.title.clone(),
                    tool: r.step.action.tool().to_string(),
                    risk: risk_label(Policy::risk_of(&r.step.action)).to_string(),
                    status: status_label(r.status).to_string(),
                    summary: r.summary.clone(),
                    attempts: r.attempts,
                })
                .collect();
            record.approvals = wf.approvals.clone();
            for r in &wf.steps {
                if matches!(r.status, StepStatus::Verified | StepStatus::Done) {
                    if let Ok(key) = serde_json::to_string(&r.step.action) {
                        done_effects.insert(key);
                    }
                }
            }
            record.errors.extend(wf.errors.clone());
            final_text = final_message(&plan, &wf, &outcome);
            last_plan = Some(plan);
            match &outcome {
                RunOutcome::Completed => {
                    record.outcome = "completed".into();
                    ui_outcome = UiOutcome::Completed;
                    break;
                }
                RunOutcome::Cancelled => {
                    record.outcome = "cancelled".into();
                    ui_outcome = UiOutcome::Cancelled;
                    break;
                }
                RunOutcome::Declined { .. } => {
                    record.outcome = "declined".into();
                    ui_outcome = UiOutcome::Completed;
                    break;
                }
                RunOutcome::Denied { .. } => {
                    record.outcome = "denied".into();
                    ui_outcome = UiOutcome::Failed;
                    break;
                }
                RunOutcome::Failed { error, .. } => {
                    record.outcome = "failed".into();
                    ui_outcome = UiOutcome::Failed;
                    if attempt < MAX_REPLANS && !matches!(strategy, Strategy::Native(_)) {
                        // Recover: ask a planner to continue from what actually happened.
                        failure = Some(format!("{}\nError: {error}", wf.progress_summary()));
                        self.emit(UiEvent::Notice {
                            text: "A step failed; asking for a corrected plan.".into(),
                        });
                        info!("replanning after failure: {error}");
                        // Escalate from the local AI to the cloud when allowed.
                        if matches!(strategy, Strategy::LocalPlan)
                            && settings.privacy != PrivacyMode::LocalOnly
                        {
                            if let Some(p) = abilities.provider {
                                strategy = Strategy::CloudPlan(p);
                            }
                        }
                        continue;
                    }
                    break;
                }
            }
        }

        // ---- finish ----
        self.bridge.deny_all();
        record.steps_total = record.steps.len();
        record.steps_verified = record
            .steps
            .iter()
            .filter(|s| s.status == "verified")
            .count();
        record.updated_at = now_ms();
        record.intent = Some(decision.intent.as_str().to_string());
        record.used = self.used_tags();
        self.workflows.save(&record);
        if !final_text.is_empty() {
            self.emit(UiEvent::AssistantMessage {
                run_id,
                text: final_text.clone(),
                needs_user: false,
            });
        }
        let mut history = ctx.history.clone();
        history.push((task.clone(), final_text));
        let excess = history.len().saturating_sub(HISTORY_TURNS);
        history.drain(..excess);
        self.conv_store
            .save(&conv_id, ctx.session_id.as_deref(), &history);
        let _ = (&last_plan, &wf);
        *Self::lock(&self.active) = None;
        self.emit(UiEvent::RunFinished {
            run_id,
            outcome: ui_outcome,
        });
    }
}
