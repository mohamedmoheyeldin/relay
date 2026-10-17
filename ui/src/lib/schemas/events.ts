// Zod schemas for the `relay-event` payloads (docs/ui-contract.md, "Events").
// Shared by the Tauri transport and the mock backend; tests check that malformed events are rejected.
import * as z from "zod/mini";

const runId = z.number();

export const RiskSchema = z.enum(["reversible", "destructive"]);
export const StepRiskSchema = z.enum(["safe", "sensitive", "destructive"]);
export const ErrorKindSchema = z.enum(["cli_not_installed", "not_logged_in", "timeout", "cancelled", "cli", "internal"]);
export const RunOutcomeSchema = z.enum(["completed", "cancelled", "timed_out", "failed"]);
export const RouteStrategySchema = z.enum(["native", "local_plan", "cloud_plan", "cloud_agent", "unavailable"]);
export const PlanSourceSchema = z.enum(["native", "local", "claude", "codex"]);
export const StepFinishStatusSchema = z.enum(["verified", "done", "failed", "skipped"]);

export const StatusPayloadSchema = z.object({
  provider: z.string(),
  ready: z.boolean(),
  detail: z.string(),
  busy: z.boolean(),
  kill_hotkey: z.string(),
  local_model: z.nullable(z.string()),
  route_mode: z.string(),
});

export const SharedCategorySchema = z.object({ category: z.string(), items: z.number() });

/** v3: how a step runs (Relay's own tools, or an MCP server such as the browser). */
export const ViaSchema = z.enum(["native", "mcp"]);
/** v3: what did the work in a run (drives the coloured chips). */
export const UsedKindSchema = z.enum(["local_ai", "cloud_ai", "native", "mcp"]);
/** v3: what kind of request the router decided this is. */
export const IntentSchema = z.enum(["chat", "action", "web"]);

export const PlanStepSchema = z.object({
  id: z.string(),
  title: z.string(),
  tool: z.string(),
  risk: StepRiskSchema,
  via: z.optional(ViaSchema),
  server: z.optional(z.nullable(z.string())),
  sensitive_data: z.optional(z.boolean()),
});

export const ProgressSchema = z.object({
  id: z.string(),
  label: z.string(),
  percent: z.nullable(z.number()),
  done: z.boolean(),
  error: z.nullable(z.string()),
});

export const UiEventSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("run_started"), run_id: runId, conv_id: z.string() }),
  z.object({
    type: z.literal("route"),
    run_id: runId,
    strategy: RouteStrategySchema,
    provider: z.nullable(z.string()),
    reason: z.string(),
    intent: z.optional(IntentSchema),
  }),
  z.object({
    type: z.literal("used"),
    run_id: runId,
    kind: UsedKindSchema,
    label: z.string(),
    detail: z.optional(z.nullable(z.string())),
  }),
  z.object({ type: z.literal("step_added"), run_id: runId, step: PlanStepSchema }),
  z.object({ type: z.literal("engine"), run_id: runId, kind: z.enum(["local", "cloud"]), name: z.string() }),
  z.object({
    type: z.literal("context_shared"),
    run_id: runId,
    provider: z.string(),
    categories: z.array(SharedCategorySchema),
    bytes: z.number(),
  }),
  z.object({
    type: z.literal("plan_proposed"),
    run_id: runId,
    source: PlanSourceSchema,
    objective: z.string(),
    steps: z.array(PlanStepSchema),
    notes: z.nullable(z.string()),
  }),
  z.object({ type: z.literal("step_started"), run_id: runId, step_id: z.string() }),
  z.object({
    type: z.literal("step_finished"),
    run_id: runId,
    step_id: z.string(),
    status: StepFinishStatusSchema,
    summary: z.string(),
  }),
  z.object({ type: z.literal("assistant_delta"), run_id: runId, text: z.string() }),
  z.object({ type: z.literal("assistant_message"), run_id: runId, text: z.string(), needs_user: z.optional(z.boolean()) }),
  z.object({ type: z.literal("tool_started"), run_id: runId, tool_id: z.string(), name: z.string(), summary: z.string() }),
  z.object({ type: z.literal("tool_finished"), run_id: runId, tool_id: z.string(), ok: z.boolean(), summary: z.string() }),
  z.object({
    type: z.literal("confirm"),
    id: z.number(),
    run_id: runId,
    title: z.string(),
    detail: z.string(),
    risk: RiskSchema,
  }),
  z.object({ type: z.literal("confirm_resolved"), id: z.number(), approved: z.boolean() }),
  z.object({ type: z.literal("notice"), text: z.string() }),
  z.object({ type: z.literal("error"), run_id: z.nullable(runId), kind: ErrorKindSchema, message: z.string() }),
  z.object({ type: z.literal("run_finished"), run_id: runId, outcome: RunOutcomeSchema }),
  z.extend(StatusPayloadSchema, { type: z.literal("status") }),
  z.extend(ProgressSchema, { type: z.literal("progress") }),
]);

export type UiEvent = z.infer<typeof UiEventSchema>;
export type UiEventType = UiEvent["type"];
export type Risk = z.infer<typeof RiskSchema>;
export type StepRisk = z.infer<typeof StepRiskSchema>;
export type ErrorKind = z.infer<typeof ErrorKindSchema>;
export type RunOutcome = z.infer<typeof RunOutcomeSchema>;
export type RouteStrategy = z.infer<typeof RouteStrategySchema>;
export type PlanSource = z.infer<typeof PlanSourceSchema>;
export type StepFinishStatus = z.infer<typeof StepFinishStatusSchema>;
export type StatusPayload = z.infer<typeof StatusPayloadSchema>;
export type PlanStep = z.infer<typeof PlanStepSchema>;
export type SharedCategory = z.infer<typeof SharedCategorySchema>;
export type Progress = z.infer<typeof ProgressSchema>;
export type Via = z.infer<typeof ViaSchema>;
export type UsedKind = z.infer<typeof UsedKindSchema>;
export type Intent = z.infer<typeof IntentSchema>;

export const EVENT_TYPES: readonly UiEventType[] = [
  "run_started",
  "route",
  "used",
  "step_added",
  "engine",
  "context_shared",
  "plan_proposed",
  "step_started",
  "step_finished",
  "assistant_delta",
  "assistant_message",
  "tool_started",
  "tool_finished",
  "confirm",
  "confirm_resolved",
  "notice",
  "error",
  "run_finished",
  "status",
  "progress",
];
