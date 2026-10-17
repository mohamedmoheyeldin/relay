// Hybrid routing, cloud providers and the task (workflow) history (docs/ui-contract.md v2).
import * as z from "zod/mini";
import { StepRiskSchema, UsedKindSchema } from "./events";

export const ProviderIdSchema = z.enum(["claude", "codex"]);
export const PrivacyModeSchema = z.enum(["local_only", "hybrid_auto", "cloud_preferred"]);
export const ConfirmLevelSchema = z.enum(["off", "destructive", "always"]);

export const ProviderCapsSchema = z.object({
  structured_output: z.boolean(),
  session_resume: z.boolean(),
  mcp_tools: z.boolean(),
  streaming_deltas: z.boolean(),
  reasoning_levels: z.array(z.string()),
  models: z.array(z.string()),
});

export const ProviderInfoSchema = z.object({
  id: ProviderIdSchema,
  label: z.string(),
  installed: z.boolean(),
  logged_in: z.boolean(),
  version: z.nullable(z.string()),
  auth_method: z.nullable(z.string()),
  detail: z.string(),
  caps: ProviderCapsSchema,
  model: z.nullable(z.string()),
  reasoning: z.nullable(z.string()),
});

export const ShareSchema = z.object({
  window_titles: z.boolean(),
  ui_tree: z.boolean(),
  file_names: z.boolean(),
  file_contents: z.boolean(),
  screenshots: z.boolean(),
});

export const HybridToolSchema = z.object({ id: z.string(), label: z.string(), risk: StepRiskSchema });

/** v3: browser control through an MCP server (Playwright). */
export const BrowserInfoSchema = z.object({ enabled: z.boolean(), available: z.boolean(), server: z.string(), detail: z.string() });

export const HybridSchema = z.object({
  privacy_mode: PrivacyModeSchema,
  default_provider: ProviderIdSchema,
  providers: z.array(ProviderInfoSchema),
  confirm_level: ConfirmLevelSchema,
  share: ShareSchema,
  allowed_roots: z.array(z.string()),
  protected_roots: z.array(z.string()),
  disabled_tools: z.array(z.string()),
  tools: z.array(HybridToolSchema),
  browser: z.optional(BrowserInfoSchema),
});

export const WorkflowOutcomeSchema = z.enum(["completed", "failed", "cancelled", "declined", "denied", "timed_out", "unavailable"]);

export const WorkflowSummarySchema = z.object({
  id: z.string(),
  conv_id: z.nullable(z.string()),
  objective: z.string(),
  strategy: z.string(),
  provider: z.nullable(z.string()),
  outcome: WorkflowOutcomeSchema,
  steps_total: z.number(),
  steps_verified: z.number(),
  created_at: z.number(),
  updated_at: z.number(),
});

export const WorkflowStepSchema = z.object({
  id: z.string(),
  title: z.string(),
  tool: z.string(),
  risk: z.string(),
  status: z.string(),
  summary: z.string(),
  attempts: z.number(),
  via: z.optional(z.nullable(z.string())),
  server: z.optional(z.nullable(z.string())),
  sensitive_data: z.optional(z.boolean()),
});

/** What did the work (same shape as the `used` event). Optional: older histories lack it. */
export const UsedTagSchema = z.object({ kind: UsedKindSchema, label: z.string(), detail: z.optional(z.nullable(z.string())) });

export const WorkflowSchema = z.extend(WorkflowSummarySchema, {
  used: z.optional(z.array(UsedTagSchema)),
  intent: z.optional(z.nullable(z.string())),
  plan_source: z.string(),
  steps: z.array(WorkflowStepSchema),
  shared: z.array(z.object({ category: z.string(), items: z.number() })),
  shared_bytes: z.number(),
  approvals: z.array(z.string()),
  errors: z.array(z.string()),
  notes: z.nullable(z.string()),
});

export type ProviderId = z.infer<typeof ProviderIdSchema>;
export type PrivacyMode = z.infer<typeof PrivacyModeSchema>;
export type ConfirmLevel = z.infer<typeof ConfirmLevelSchema>;
export type ProviderInfo = z.infer<typeof ProviderInfoSchema>;
export type Share = z.infer<typeof ShareSchema>;
export type HybridTool = z.infer<typeof HybridToolSchema>;
export type Hybrid = z.infer<typeof HybridSchema>;
export type WorkflowOutcome = z.infer<typeof WorkflowOutcomeSchema>;
export type WorkflowSummary = z.infer<typeof WorkflowSummarySchema>;
export type WorkflowStep = z.infer<typeof WorkflowStepSchema>;
export type Workflow = z.infer<typeof WorkflowSchema>;
export type BrowserInfo = z.infer<typeof BrowserInfoSchema>;
export type UsedTag = z.infer<typeof UsedTagSchema>;

/** `set_hybrid` argument: every field optional (camelCase, converted by Tauri). */
export type HybridPatch = {
  privacyMode?: PrivacyMode;
  defaultProvider?: ProviderId;
  confirmLevel?: ConfirmLevel;
  share?: Partial<Share>;
  allowedRoots?: string[];
  disabledTools?: string[];
  provider?: { id: ProviderId; model?: string | null; reasoning?: string | null };
  /** v3: switch browser control (the Playwright MCP server) on or off. */
  browserEnabled?: boolean;
};

/** Applies a set_hybrid patch locally (optimistic update and mock backend share this). */
export function applyHybridPatch(h: Hybrid, p: HybridPatch): Hybrid {
  return {
    ...h,
    privacy_mode: p.privacyMode ?? h.privacy_mode,
    default_provider: p.defaultProvider ?? h.default_provider,
    confirm_level: p.confirmLevel ?? h.confirm_level,
    share: p.share ? { ...h.share, ...p.share } : h.share,
    allowed_roots: p.allowedRoots ?? h.allowed_roots,
    disabled_tools: p.disabledTools ?? h.disabled_tools,
    ...(h.browser ? { browser: p.browserEnabled !== undefined && h.browser.available ? { ...h.browser, enabled: p.browserEnabled } : h.browser } : {}),
    providers: p.provider
      ? h.providers.map((x) =>
          x.id === p.provider!.id
            ? {
                ...x,
                model: p.provider!.model !== undefined ? p.provider!.model : x.model,
                reasoning: p.provider!.reasoning !== undefined ? p.provider!.reasoning : x.reasoning,
              }
            : x,
        )
      : h.providers,
  };
}
