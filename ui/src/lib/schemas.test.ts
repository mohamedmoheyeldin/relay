import { describe, expect, it, vi } from "vitest";
import { HybridSchema, UiEventSchema, WorkflowSchema, applyHybridPatch, type Hybrid } from "./schemas";
import { checkEvent, checkResult, onContractViolation } from "./backend";

const valid: unknown[] = [
  { type: "run_started", run_id: 1, conv_id: "c" },
  { type: "route", run_id: 1, strategy: "cloud_plan", provider: "codex", reason: "Hybrid" },
  { type: "route", run_id: 1, strategy: "native", provider: null, reason: "direct" },
  { type: "engine", run_id: 1, kind: "local", name: "qwen3:14b" },
  { type: "context_shared", run_id: 1, provider: "claude", categories: [{ category: "file_names", items: 4 }], bytes: 120 },
  { type: "plan_proposed", run_id: 1, source: "local", objective: "x", steps: [{ id: "a", title: "A", tool: "t", risk: "sensitive" }], notes: null },
  { type: "step_started", run_id: 1, step_id: "a" },
  { type: "step_finished", run_id: 1, step_id: "a", status: "done", summary: "" },
  { type: "assistant_delta", run_id: 1, text: "hi" },
  { type: "assistant_message", run_id: 1, text: "hi" },
  { type: "tool_started", run_id: 1, tool_id: "t", name: "n", summary: "s" },
  { type: "tool_finished", run_id: 1, tool_id: "t", ok: false, summary: "s" },
  { type: "confirm", id: 3, run_id: 1, title: "t", detail: "d", risk: "destructive" },
  { type: "confirm_resolved", id: 3, approved: true },
  { type: "notice", text: "n" },
  { type: "error", run_id: null, kind: "not_logged_in", message: "m" },
  { type: "run_finished", run_id: 1, outcome: "timed_out" },
  { type: "status", provider: "p", ready: true, detail: "d", busy: false, kill_hotkey: "k", local_model: null, route_mode: "hybrid_auto" },
  { type: "progress", id: "local-ai", label: "l", percent: null, done: false, error: null },
  // v3 additions
  { type: "route", run_id: 1, strategy: "cloud_plan", provider: "claude", reason: "q", intent: "chat" },
  { type: "used", run_id: 1, kind: "local_ai", label: "Qwen3 14B" },
  { type: "used", run_id: 1, kind: "mcp", label: "Playwright browser", detail: "server playwright" },
  { type: "step_added", run_id: 1, step: { id: "w1", title: "Open", tool: "browser_navigate", risk: "safe", via: "mcp", server: "playwright" } },
  { type: "plan_proposed", run_id: 1, source: "claude", objective: "x", steps: [{ id: "a", title: "A", tool: "t", risk: "sensitive", via: "native", sensitive_data: true }], notes: null },
];

const malformed: [string, unknown][] = [
  ["unknown type", { type: "explode", run_id: 1 }],
  ["no type", { run_id: 1 }],
  ["missing field", { type: "run_started", run_id: 1 }],
  ["wrong field type", { type: "run_started", run_id: "1", conv_id: "c" }],
  ["bad strategy", { type: "route", run_id: 1, strategy: "teleport", provider: null, reason: "" }],
  ["bad step status", { type: "step_finished", run_id: 1, step_id: "a", status: "probably", summary: "" }],
  ["bad step risk", { type: "plan_proposed", run_id: 1, source: "local", objective: "x", steps: [{ id: "a", title: "A", tool: "t", risk: "yolo" }], notes: null }],
  ["bad plan source", { type: "plan_proposed", run_id: 1, source: "gpt", objective: "x", steps: [], notes: null }],
  ["categories not an array", { type: "context_shared", run_id: 1, provider: "claude", categories: {}, bytes: 1 }],
  ["confirm risk", { type: "confirm", id: 1, run_id: 1, title: "", detail: "", risk: "safe" }],
  ["outcome", { type: "run_finished", run_id: 1, outcome: "exploded" }],
  ["bad used kind", { type: "used", run_id: 1, kind: "magic", label: "x" }],
  ["bad intent", { type: "route", run_id: 1, strategy: "native", provider: null, reason: "", intent: "dance" }],
  ["step_added without step", { type: "step_added", run_id: 1 }],
  ["null payload", null],
  ["string payload", "hello"],
];

describe("event schemas", () => {
  it("accept every event in the contract", () => {
    for (const ev of valid) expect(UiEventSchema.safeParse(ev).success, JSON.stringify(ev)).toBe(true);
  });
  it.each(malformed)("reject malformed events: %s", (_name, ev) => {
    expect(UiEventSchema.safeParse(ev).success).toBe(false);
  });
  it("checkEvent drops malformed events (development) and reports a violation", () => {
    const seen = vi.fn();
    const off = onContractViolation(seen);
    const err = vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(checkEvent({ type: "run_started", run_id: 1 })).toBeNull();
    expect(seen).toHaveBeenCalledWith(expect.objectContaining({ where: "event", name: "run_started" }));
    expect(checkEvent(JSON.stringify(valid[0]))).toEqual(valid[0]); // JSON-encoded payloads are decoded
    off();
    err.mockRestore();
  });
});

const hybrid: Hybrid = {
  privacy_mode: "hybrid_auto",
  default_provider: "claude",
  providers: [
    {
      id: "claude",
      label: "Claude Code",
      installed: true,
      logged_in: true,
      version: "1",
      auth_method: null,
      detail: "",
      caps: { structured_output: true, session_resume: true, mcp_tools: true, streaming_deltas: true, reasoning_levels: [], models: [] },
      model: null,
      reasoning: null,
    },
  ],
  confirm_level: "destructive",
  share: { window_titles: true, ui_tree: false, file_names: true, file_contents: false, screenshots: false },
  allowed_roots: [],
  protected_roots: ["C:\\Windows"],
  disabled_tools: [],
  tools: [{ id: "open_app", label: "Open apps", risk: "safe" }],
};

describe("command schemas", () => {
  it("validate get_hybrid and reject a wrong privacy mode", () => {
    expect(HybridSchema.safeParse(hybrid).success).toBe(true);
    expect(HybridSchema.safeParse({ ...hybrid, privacy_mode: "anywhere" }).success).toBe(false);
    expect(HybridSchema.safeParse({ ...hybrid, providers: [{ ...hybrid.providers[0], id: "gemini" }] }).success).toBe(false);
  });
  it("checkResult throws in development so contract drift is visible", () => {
    const err = vi.spyOn(console, "error").mockImplementation(() => undefined);
    expect(() => checkResult("get_hybrid", HybridSchema, { ...hybrid, share: null })).toThrow(/unexpected get_hybrid/);
    expect(checkResult("get_hybrid", HybridSchema, hybrid)).toEqual(hybrid);
    err.mockRestore();
  });
  it("validates workflow_get", () => {
    const w = {
      id: "w",
      conv_id: null,
      objective: "o",
      strategy: "native",
      provider: null,
      outcome: "declined",
      steps_total: 1,
      steps_verified: 0,
      created_at: 1,
      updated_at: 2,
      plan_source: "native",
      steps: [{ id: "a", title: "A", tool: "t", risk: "safe", status: "skipped", summary: "", attempts: 0 }],
      shared: [],
      shared_bytes: 0,
      approvals: ["Declined: x"],
      errors: [],
      notes: null,
    };
    expect(WorkflowSchema.safeParse(w).success).toBe(true);
    expect(WorkflowSchema.safeParse({ ...w, outcome: "meh" }).success).toBe(false);
  });
  it("applies set_hybrid patches like the backend", () => {
    const next = applyHybridPatch(hybrid, { privacyMode: "local_only", share: { screenshots: true }, provider: { id: "claude", model: "opus" } });
    expect(next.privacy_mode).toBe("local_only");
    expect(next.share).toMatchObject({ screenshots: true, window_titles: true });
    expect(next.providers[0]!.model).toBe("opus");
    expect(next.providers[0]!.reasoning).toBeNull();
  });
  it("accepts the v3 browser block (optional) and applies browserEnabled only when available", () => {
    const withBrowser: Hybrid = { ...hybrid, browser: { enabled: false, available: true, server: "playwright", detail: "Ready" } };
    expect(HybridSchema.safeParse(withBrowser).success).toBe(true);
    expect(applyHybridPatch(withBrowser, { browserEnabled: true }).browser?.enabled).toBe(true);
    const unavailable: Hybrid = { ...hybrid, browser: { enabled: false, available: false, server: "playwright", detail: "Node.js was not found" } };
    expect(applyHybridPatch(unavailable, { browserEnabled: true }).browser?.enabled).toBe(false);
    expect("browser" in applyHybridPatch(hybrid, { browserEnabled: true })).toBe(false);
  });
});
