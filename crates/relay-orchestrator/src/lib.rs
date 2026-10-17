//! The hybrid AI orchestrator: routes a task, turns AI output into validated plans, and executes
//! them through permissioned native services, verifying every effect.
//!
//! Layers (each testable on its own):
//! * [`plan`]: the closed set of typed actions and the plan schema (AI output is untrusted).
//! * [`permissions`]: allowlists, folder boundaries, confirmations and sharing policy.
//! * [`router`]: native / local / cloud strategy selection.
//! * [`context`]: structured workflow state and the minimal, redacted context for cloud prompts.
//! * [`services`]: the native execution layer interface.
//! * [`executor`]: permission check, confirmation, execution, verification, retry.

pub mod context;
pub mod executor;
pub mod permissions;
pub mod plan;
pub mod router;
pub mod services;

pub use context::{DesktopFacts, ScopedContext, StepRecord, StepStatus, WorkflowContext};
pub use executor::{execute_plan, Confirmer, EventSink, ExecConfig, ExecEvent, RunOutcome};
pub use permissions::{ConfirmLevel, Policy, PrivacyMode, Risk, SharePolicy, Verdict};
pub use plan::{ExecutionPlan, PlanAction, PlanError, PlanStep, UiTarget, Verification};
pub use router::{route, Abilities, ProviderKind, Route, Strategy};
pub use services::{ServiceError, ServiceResult, Services};
