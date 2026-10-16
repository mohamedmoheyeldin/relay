//! Executes tool calls: validation, safety gate, desktop actions, audit.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use relay_os::capture::{self, changed_pixels};
use relay_os::driver::Driver;
use relay_os::scaling::{Budget, DisplayGeometry};
use relay_os::types::{Desktop, DisplayInfo, OsError};
use relay_protocol::{Risk, RunId};
use serde_json::{json, Value};

use crate::actions::{parse_action, Action, ToolError};
use crate::audit::AuditLog;
use crate::safety::{
    classify_builtin, control_name_is_risky, truncate, ApprovalMode, ApprovalRequest, Approver,
    KillSwitch, Verdict, STOPPED_MESSAGE,
};

/// One piece of tool output.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Image { png_base64: String },
}

/// The result of a tool call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    pub content: Vec<Content>,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(text.into())],
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(text.into())],
            is_error: true,
        }
    }

    /// All text parts joined; convenient for tests and logs.
    pub fn joined_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text(t) => Some(t.as_str()),
                Content::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn has_image(&self) -> bool {
        self.content
            .iter()
            .any(|c| matches!(c, Content::Image { .. }))
    }
}

/// Who is calling.
#[derive(Debug, Clone, Copy)]
pub struct CallCtx {
    pub run_id: RunId,
}

/// Tunables for how the host behaves.
#[derive(Debug, Clone, Copy)]
pub struct HostOptions {
    pub budget: Budget,
    pub approval_mode: ApprovalMode,
    /// Compare screens before/after actions and report whether anything changed.
    pub change_verdicts: bool,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            budget: Budget::default(),
            approval_mode: ApprovalMode::default(),
            change_verdicts: true,
        }
    }
}

const HALT_TEXT: &str = "Not executed: an earlier computer action in this turn failed.";
const MAX_BATCH: usize = 25;
const VERDICT_SETTLE: Duration = Duration::from_millis(150);
const VERDICT_POLL: Duration = Duration::from_millis(100);
const VERDICT_PATIENCE: Duration = Duration::from_millis(750);
const PIXEL_DIFF: u8 = 24;
/// Typing a few characters on a large display changes only a few dozen pixels.
const MIN_CHANGED_PIXELS: usize = 24;

/// Shared services behind the MCP tools.
pub struct ToolHost {
    desktop: Arc<dyn Desktop>,
    driver: Arc<Driver>,
    kill: KillSwitch,
    approver: Arc<dyn Approver>,
    audit: Option<Arc<AuditLog>>,
    budget: Budget,
    mode: Mutex<ApprovalMode>,
    pinned_display: Mutex<Option<u32>>,
    change_verdicts: bool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn os_error(e: OsError) -> ToolError {
    match e {
        OsError::Stopped => ToolError::new(STOPPED_MESSAGE),
        other => ToolError::new(other.to_string()),
    }
}

impl ToolHost {
    pub fn new(
        desktop: Arc<dyn Desktop>,
        driver: Arc<Driver>,
        kill: KillSwitch,
        approver: Arc<dyn Approver>,
        audit: Option<Arc<AuditLog>>,
        options: HostOptions,
    ) -> Self {
        Self {
            desktop,
            driver,
            kill,
            approver,
            audit,
            budget: options.budget,
            mode: Mutex::new(options.approval_mode),
            pinned_display: Mutex::new(None),
            change_verdicts: options.change_verdicts,
        }
    }

    pub fn kill_switch(&self) -> &KillSwitch {
        &self.kill
    }

    pub fn set_approval_mode(&self, mode: ApprovalMode) {
        *lock(&self.mode) = mode;
    }

    pub fn approval_mode(&self) -> ApprovalMode {
        *lock(&self.mode)
    }

    /// The active display and its screenshot geometry. Pins the first choice so coordinates
    /// keep their meaning; re-pins if the pinned display disappears.
    fn geometry(&self) -> Result<(DisplayInfo, DisplayGeometry), ToolError> {
        let displays = self.desktop.displays().map_err(os_error)?;
        let mut pinned = lock(&self.pinned_display);
        let chosen = pinned
            .and_then(|id| displays.iter().find(|d| d.id == id))
            .or_else(|| displays.iter().find(|d| d.primary))
            .or_else(|| displays.first())
            .cloned()
            .ok_or_else(|| ToolError::new("no displays found"))?;
        *pinned = Some(chosen.id);
        let geometry = DisplayGeometry::new(chosen.rect, &self.budget).map_err(os_error)?;
        Ok((chosen, geometry))
    }

    /// Dispatch one MCP tool call.
    pub async fn call(&self, ctx: &CallCtx, name: &str, args: &Value) -> ToolOutput {
        let started = Instant::now();
        let output = match name {
            "approve" => self.permission_prompt(ctx, args).await,
            "ask_user" => self.ask_user(ctx, args).await,
            "computer_batch" => self.batch(ctx, args).await,
            _ => match parse_action(name, args) {
                Ok(action) => match self.run_action(ctx, &action).await {
                    Ok(content) => ToolOutput {
                        content,
                        is_error: false,
                    },
                    Err(e) => ToolOutput::error(e.0),
                },
                Err(e) => ToolOutput::error(e.0),
            },
        };
        if let Some(audit) = &self.audit {
            audit
                .record(
                    ctx.run_id,
                    name,
                    args,
                    &truncate(&output.joined_text(), 500),
                    output.is_error,
                    started.elapsed(),
                )
                .await;
        }
        output
    }

    async fn confirm(
        &self,
        ctx: &CallCtx,
        title: String,
        detail: String,
        risk: Risk,
    ) -> Result<(), ToolError> {
        let decision = self
            .approver
            .ask(ApprovalRequest {
                run_id: ctx.run_id,
                title,
                detail,
                risk,
            })
            .await;
        if decision.is_approved() {
            Ok(())
        } else {
            Err(ToolError::new(decision.refusal_message()))
        }
    }

    /// Decide whether the owner must approve this action first.
    async fn gate(&self, action: &Action) -> Option<(String, Risk)> {
        let mode = self.approval_mode();
        if mode == ApprovalMode::Off || !action.is_act() {
            return None;
        }
        if mode == ApprovalMode::Always {
            return Some((action.describe(), Risk::Reversible));
        }
        if action.flagged_destructive() || action.is_dangerous_key() {
            return Some((action.describe(), Risk::Destructive));
        }
        if matches!(action, Action::Click { .. }) {
            if let Some(name) = self.control_under(action).await {
                if control_name_is_risky(&name) {
                    return Some((
                        format!("{} on \"{}\"", action.describe(), name),
                        Risk::Destructive,
                    ));
                }
            }
        }
        None
    }

    async fn control_under(&self, action: &Action) -> Option<String> {
        let (_, geometry) = self.geometry().ok()?;
        let (x, y) = match action.target() {
            Some((x, y)) => geometry.to_physical(x, y).ok()?,
            None => self.desktop.cursor_position().ok()?,
        };
        let desktop = self.desktop.clone();
        tokio::task::spawn_blocking(move || desktop.control_name_at(x, y).ok().flatten())
            .await
            .ok()
            .flatten()
    }

    async fn run_action(&self, ctx: &CallCtx, action: &Action) -> Result<Vec<Content>, ToolError> {
        if action.is_act() && self.kill.is_tripped() {
            return Err(ToolError::new(STOPPED_MESSAGE));
        }
        if let Some((title, risk)) = self.gate(action).await {
            self.confirm(
                ctx,
                title,
                format!("Relay wants to: {}", action.describe()),
                risk,
            )
            .await?;
            if self.kill.is_tripped() {
                return Err(ToolError::new(STOPPED_MESSAGE));
            }
        }
        let (display, geometry) = self.geometry()?;
        let env = ExecEnv {
            desktop: self.desktop.clone(),
            driver: self.driver.clone(),
            budget: self.budget,
            display,
            geometry,
            verdicts: self.change_verdicts,
        };
        let action = action.clone();
        let outcome = tokio::task::spawn_blocking(move || env.execute(&action))
            .await
            .map_err(|e| ToolError::new(format!("internal error: {e}")))??;
        if let Exec::Pin(id) = outcome.pin {
            *lock(&self.pinned_display) = Some(id);
        }
        Ok(outcome.content)
    }

    async fn batch(&self, ctx: &CallCtx, args: &Value) -> ToolOutput {
        let Some(items) = args.get("actions").and_then(Value::as_array) else {
            return ToolOutput::error("actions must be a list of {name, input} objects");
        };
        if items.is_empty() || items.len() > MAX_BATCH {
            return ToolOutput::error(format!("actions must hold 1..={MAX_BATCH} entries"));
        }
        let mut content = Vec::new();
        let mut failed = false;
        for (i, item) in items.iter().enumerate() {
            let label = format!("{}/{}", i + 1, items.len());
            if failed {
                content.push(Content::Text(format!("[{label}] {HALT_TEXT}")));
                continue;
            }
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                content.push(Content::Text(format!(
                    "[{label}] each entry needs a string name and an object input"
                )));
                failed = true;
                continue;
            };
            let input = item.get("input").cloned().unwrap_or_else(|| json!({}));
            if !input.is_object() {
                content.push(Content::Text(format!(
                    "[{label}] {name}: input must be an object"
                )));
                failed = true;
                continue;
            }
            if matches!(name, "computer_batch" | "approve" | "ask_user") {
                content.push(Content::Text(format!("[{label}] {name} cannot be batched")));
                failed = true;
                continue;
            }
            let result = match parse_action(name, &input) {
                Ok(action) => self.run_action(ctx, &action).await,
                Err(e) => Err(e),
            };
            match result {
                Ok(parts) => {
                    for part in parts {
                        match part {
                            Content::Text(t) => {
                                content.push(Content::Text(format!("[{label}] {t}")))
                            }
                            // Intermediate screenshots are dropped; only the last one is kept.
                            Content::Image { .. } if i + 1 < items.len() => {}
                            image @ Content::Image { .. } => content.push(image),
                        }
                    }
                }
                Err(e) => {
                    content.push(Content::Text(format!("[{label}] {name} failed: {}", e.0)));
                    failed = true;
                }
            }
        }
        if !failed {
            if let Ok(parts) = self.run_action(ctx, &Action::Screenshot).await {
                content.extend(parts);
            }
        }
        ToolOutput {
            content,
            is_error: failed,
        }
    }

    async fn ask_user(&self, ctx: &CallCtx, args: &Value) -> ToolOutput {
        let question = args
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if question.trim().is_empty() {
            return ToolOutput::error("question is required");
        }
        let risk = match args.get("risk").and_then(Value::as_str) {
            Some("destructive") => Risk::Destructive,
            _ => Risk::Reversible,
        };
        match self
            .confirm(ctx, "Relay is asking".into(), truncate(question, 600), risk)
            .await
        {
            Ok(()) => ToolOutput::text("The owner approved."),
            Err(e) => ToolOutput::error(e.0),
        }
    }

    /// The permission-prompt tool: the AI CLI asks here before using one of its own tools.
    async fn permission_prompt(&self, ctx: &CallCtx, args: &Value) -> ToolOutput {
        let tool = args
            .get("tool_name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let input = args.get("input").cloned().unwrap_or(Value::Null);
        let deny = |message: &str| {
            ToolOutput::text(json!({"behavior": "deny", "message": message}).to_string())
        };
        if self.kill.is_tripped() {
            return deny(STOPPED_MESSAGE);
        }
        let verdict = if self.approval_mode() == ApprovalMode::Off {
            Verdict::Allow
        } else {
            classify_builtin(tool, &input)
        };
        match verdict {
            Verdict::Allow => {
                ToolOutput::text(json!({"behavior": "allow", "updatedInput": input}).to_string())
            }
            Verdict::Ask {
                title,
                detail,
                risk,
            } => match self.confirm(ctx, title, detail, risk).await {
                Ok(()) if !self.kill.is_tripped() => ToolOutput::text(
                    json!({"behavior": "allow", "updatedInput": input}).to_string(),
                ),
                Ok(()) => deny(STOPPED_MESSAGE),
                Err(e) => deny(&e.0),
            },
        }
    }
}

/// Whether executing an action changes which display is active.
enum Exec {
    Nothing,
    Pin(u32),
}

struct Outcome {
    content: Vec<Content>,
    pin: Exec,
}

impl Outcome {
    fn text(t: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(t.into())],
            pin: Exec::Nothing,
        }
    }
}

/// Everything a blocking action needs, captured by value.
struct ExecEnv {
    desktop: Arc<dyn Desktop>,
    driver: Arc<Driver>,
    budget: Budget,
    display: DisplayInfo,
    geometry: DisplayGeometry,
    verdicts: bool,
}

impl ExecEnv {
    fn physical(&self, p: (i64, i64)) -> Result<(i32, i32), ToolError> {
        self.geometry.to_physical(p.0, p.1).map_err(os_error)
    }

    fn optional_physical(&self, p: Option<(i64, i64)>) -> Result<Option<(i32, i32)>, ToolError> {
        p.map(|p| self.physical(p)).transpose()
    }

    fn png(&self, shot: capture::Shot) -> Content {
        Content::Image {
            png_base64: base64::engine::general_purpose::STANDARD.encode(shot.png),
        }
    }

    /// Run `act`, then report whether the screen visibly changed.
    fn with_verdict(&self, act: impl FnOnce() -> Result<(), OsError>) -> Result<String, ToolError> {
        if !self.verdicts {
            act().map_err(os_error)?;
            return Ok("OK".into());
        }
        let before = self.desktop.capture(self.display.rect).ok();
        act().map_err(os_error)?;
        let Some(before) = before else {
            return Ok("OK".into());
        };
        std::thread::sleep(VERDICT_SETTLE);
        let started = Instant::now();
        loop {
            if let Ok(after) = self.desktop.capture(self.display.rect) {
                if changed_pixels(&before, &after, PIXEL_DIFF) >= MIN_CHANGED_PIXELS {
                    return Ok("OK · screen changed".into());
                }
            }
            if started.elapsed() >= VERDICT_PATIENCE {
                return Ok("OK · no visible change".into());
            }
            std::thread::sleep(VERDICT_POLL);
        }
    }

    fn execute(&self, action: &Action) -> Result<Outcome, ToolError> {
        let d = &self.driver;
        match action {
            Action::Screenshot => {
                let shot = capture::screenshot(&*self.desktop, &self.display, &self.budget)
                    .map_err(os_error)?;
                let note = format!(
                    "Screenshot {}x{} of display {}. All coordinates are pixels of this image, origin top-left.",
                    shot.width, shot.height, self.display.id
                );
                Ok(Outcome {
                    content: vec![Content::Text(note), self.png(shot)],
                    pin: Exec::Nothing,
                })
            }
            Action::Zoom { region } => {
                let shot = capture::zoom(&*self.desktop, &self.geometry, *region, &self.budget)
                    .map_err(os_error)?;
                Ok(Outcome {
                    content: vec![
                        Content::Text(format!(
                            "Zoomed view of {region:?}; coordinates in the full screenshot are unchanged."
                        )),
                        self.png(shot),
                    ],
                    pin: Exec::Nothing,
                })
            }
            Action::CursorPosition => {
                let (px, py) = self.desktop.cursor_position().map_err(os_error)?;
                if self.geometry.contains_physical(px, py) {
                    let (x, y) = self.geometry.to_screenshot(px, py);
                    Ok(Outcome::text(format!("({x}, {y})")))
                } else {
                    Ok(Outcome::text(
                        "The pointer is on another display than the active one.",
                    ))
                }
            }
            Action::Wait { duration } => {
                d.wait(*duration).map_err(os_error)?;
                Ok(Outcome::text(format!(
                    "Waited {:.1} s.",
                    duration.as_secs_f64()
                )))
            }
            Action::MouseMove { at } => {
                let (x, y) = self.physical(*at)?;
                d.move_to(x, y).map_err(os_error)?;
                Ok(Outcome::text("OK"))
            }
            Action::Click {
                at,
                button,
                count,
                mods,
                ..
            } => {
                let target = self.optional_physical(*at)?;
                let verdict = self.with_verdict(|| d.click(target, *button, *count, mods))?;
                Ok(Outcome::text(verdict))
            }
            Action::Drag { from, to, mods, .. } => {
                let (a, b) = (self.physical(*from)?, self.physical(*to)?);
                let verdict = self.with_verdict(|| d.drag(a, b, mods))?;
                Ok(Outcome::text(verdict))
            }
            Action::MouseDown => {
                d.mouse_down(relay_os::MouseButton::Left)
                    .map_err(os_error)?;
                Ok(Outcome::text("OK"))
            }
            Action::MouseUp => {
                d.mouse_up(relay_os::MouseButton::Left).map_err(os_error)?;
                Ok(Outcome::text("OK"))
            }
            Action::Scroll {
                at,
                direction,
                amount,
                mods,
            } => {
                let target = self.optional_physical(*at)?;
                let verdict = self.with_verdict(|| d.scroll(target, *direction, *amount, mods))?;
                Ok(Outcome::text(verdict))
            }
            Action::Type { text, .. } => {
                let verdict = self.with_verdict(|| d.type_text(text))?;
                Ok(Outcome::text(verdict))
            }
            Action::Key { chord, repeat, .. } => {
                let verdict = self.with_verdict(|| d.key(chord, *repeat))?;
                Ok(Outcome::text(verdict))
            }
            Action::HoldKey {
                chord, duration, ..
            } => {
                d.hold_key(chord, *duration).map_err(os_error)?;
                Ok(Outcome::text("OK"))
            }
            Action::ListWindows => {
                let windows = self.desktop.windows().map_err(os_error)?;
                if windows.is_empty() {
                    return Ok(Outcome::text("No windows."));
                }
                let lines: Vec<String> = windows
                    .iter()
                    .map(|w| {
                        let tl = self.geometry.to_screenshot(w.rect.x, w.rect.y);
                        let br = self.geometry.to_screenshot(
                            w.rect.x + w.rect.w as i32 - 1,
                            w.rect.y + w.rect.h as i32 - 1,
                        );
                        format!(
                            "{}\"{}\" [{}, {}, {}, {}]",
                            if w.foreground { "* " } else { "  " },
                            truncate(&w.title, 100),
                            tl.0,
                            tl.1,
                            br.0,
                            br.1
                        )
                    })
                    .collect();
                Ok(Outcome::text(format!(
                    "Windows (* = in front; boxes are screenshot pixels of the active display, clamped to it):\n{}",
                    lines.join("\n")
                )))
            }
            Action::FocusWindow { title } => {
                let focused = self.desktop.focus_window(title).map_err(os_error)?;
                let mut pin = Exec::Nothing;
                if let Ok(windows) = self.desktop.windows() {
                    if let Some(w) = windows.iter().find(|w| w.title == focused) {
                        let cx = w.rect.x + (w.rect.w / 2) as i32;
                        let cy = w.rect.y + (w.rect.h / 2) as i32;
                        if let Ok(displays) = self.desktop.displays() {
                            if let Some(display) =
                                displays.iter().find(|dp| dp.rect.contains(cx, cy))
                            {
                                pin = Exec::Pin(display.id);
                            }
                        }
                    }
                }
                Ok(Outcome {
                    content: vec![Content::Text(format!(
                        "OK · \"{}\" is in front. Take a new screenshot before acting.",
                        truncate(&focused, 100)
                    ))],
                    pin,
                })
            }
            Action::ListDisplays => {
                let displays = self.desktop.displays().map_err(os_error)?;
                let lines: Vec<String> = displays
                    .iter()
                    .map(|dp| {
                        format!(
                            "{}display {}: {}x{} at ({}, {}){}",
                            if dp.id == self.display.id { "* " } else { "  " },
                            dp.id,
                            dp.rect.w,
                            dp.rect.h,
                            dp.rect.x,
                            dp.rect.y,
                            if dp.primary { " (primary)" } else { "" }
                        )
                    })
                    .collect();
                Ok(Outcome::text(format!("* = active\n{}", lines.join("\n"))))
            }
            Action::SelectDisplay { id } => {
                let displays = self.desktop.displays().map_err(os_error)?;
                if displays.iter().any(|dp| dp.id == *id) {
                    Ok(Outcome {
                        content: vec![Content::Text(format!(
                            "Display {id} is now active. Take a new screenshot."
                        ))],
                        pin: Exec::Pin(*id),
                    })
                } else {
                    let ids: Vec<String> = displays.iter().map(|dp| dp.id.to_string()).collect();
                    Err(ToolError::new(format!(
                        "no display {id}; available: {}",
                        ids.join(", ")
                    )))
                }
            }
        }
    }
}
