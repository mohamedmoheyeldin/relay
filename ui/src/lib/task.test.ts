import { describe, expect, it } from "vitest";
import { applyEvent, emptyChat, normalizeLoaded, type ChatEvent, type ChatState, type TaskItem } from "./chat";
import { taskFixture } from "./fixtures";
import { canTransition, currentStepIndex, ledgerSummary, providerLabel, routeBadge, STEP_TRANSITIONS, verdictOf, verdictText } from "./task";

function run(events: ChatEvent[], start: ChatState = emptyChat(), t0 = 1000): ChatState {
  let s = start;
  events.forEach((e, i) => (s = applyEvent(s, e, t0 + i * 1000)));
  return s;
}
const task = (s: ChatState) => s.items.find((i): i is TaskItem => i.kind === "task")!;

const plan: ChatEvent = {
  type: "plan_proposed",
  run_id: 1,
  source: "codex",
  objective: "Do it",
  notes: null,
  steps: [
    { id: "a", title: "A", tool: "t", risk: "safe" },
    { id: "b", title: "B", tool: "t", risk: "destructive" },
  ],
};

describe("task timeline reducer", () => {
  it("builds one task item from route, ledger, plan and step events", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      { type: "route", run_id: 1, strategy: "cloud_plan", provider: "codex", reason: "Hybrid: Codex plans it, Relay runs it here" },
      { type: "context_shared", run_id: 1, provider: "codex", categories: [{ category: "window_titles", items: 3 }], bytes: 900 },
      plan,
      { type: "step_started", run_id: 1, step_id: "a" },
    ]);
    expect(s.items.map((i) => i.kind)).toEqual(["task"]);
    const t = task(s);
    expect(t.route?.reason).toBe("Hybrid: Codex plans it, Relay runs it here");
    expect(t.shared).toHaveLength(1);
    expect(t.plan?.source).toBe("codex");
    expect(t.steps.map((x) => x.status)).toEqual(["running", "pending"]);
    expect(t.steps[0]).toMatchObject({ attempts: 1, started_at: 5000 });
    expect(verdictOf(t).kind).toBe("running");
  });

  it("records verified, done, failed and skipped results with summaries", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      plan,
      { type: "step_started", run_id: 1, step_id: "a" },
      { type: "step_finished", run_id: 1, step_id: "a", status: "verified", summary: "checked" },
      { type: "step_started", run_id: 1, step_id: "b" },
      { type: "step_finished", run_id: 1, step_id: "b", status: "failed", summary: "Refused: outside allowed folders" },
      { type: "run_finished", run_id: 1, outcome: "failed" },
    ]);
    const t = task(s);
    expect(t.steps.map((x) => [x.status, x.summary])).toEqual([
      ["verified", "checked"],
      ["failed", "Refused: outside allowed folders"],
    ]);
    const v = verdictOf(t);
    expect(v).toMatchObject({ kind: "failed", at: 2 });
    expect(verdictText(v)).toBe("Blocked at step 2"); // a refusal reads differently from a failed check
    expect(s.run).toBeNull();
  });

  it("keeps step progress when the plan is proposed again, and records re-routes", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      { type: "route", run_id: 1, strategy: "cloud_plan", provider: "claude", reason: "cloud" },
      plan,
      { type: "step_started", run_id: 1, step_id: "a" },
      { type: "step_finished", run_id: 1, step_id: "a", status: "done", summary: "" },
      { type: "route", run_id: 1, strategy: "local_plan", provider: "local", reason: "fallback" },
      { ...plan, source: "local" } as ChatEvent,
    ]);
    const t = task(s);
    expect(t.previous_routes.map((r) => r.provider)).toEqual(["claude"]);
    expect(t.route?.provider).toBe("local");
    expect(t.plan?.source).toBe("local");
    expect(t.steps[0]!.status).toBe("done");
  });

  it("attaches approvals to the running step and resolves them", () => {
    let s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      plan,
      { type: "step_started", run_id: 1, step_id: "b" },
      { type: "confirm", id: 9, run_id: 1, title: "Delete?", detail: "x", risk: "destructive" },
    ]);
    expect(task(s).confirms).toEqual([expect.objectContaining({ confirm_id: 9, step_id: "b", state: "pending" })]);
    // No tool group is created for plan runs.
    expect(s.items.some((i) => i.kind === "tools")).toBe(false);
    s = applyEvent(s, { type: "confirm_resolved", id: 9, approved: false }, 9000);
    expect(task(s).confirms[0]!.state).toBe("declined");
  });

  it("a declined step makes the verdict 'declined', not 'failed'", () => {
    const t = taskFixture("declined");
    expect(verdictOf(t).kind).toBe("declined");
    expect(verdictText(verdictOf(t))).toBe("Not done: you said no");
  });

  it("stopping marks the running step stopped and pending approvals declined", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      plan,
      { type: "step_started", run_id: 1, step_id: "a" },
      { type: "confirm", id: 2, run_id: 1, title: "Go?", detail: "", risk: "reversible" },
      { type: "run_finished", run_id: 1, outcome: "cancelled" },
    ]);
    const t = task(s);
    expect(t.steps.map((x) => x.status)).toEqual(["stopped", "pending"]);
    expect(t.confirms[0]!.state).toBe("declined");
    expect(verdictOf(t).kind).toBe("stopped");
    // Plan runs show the stop in the timeline, not as an extra "Stopped." notice.
    expect(s.items.some((i) => i.kind === "notice")).toBe(false);
  });

  it("places the task card before text the run already streamed", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      { type: "assistant_delta", run_id: 1, text: "Thinking" },
      { type: "route", run_id: 1, strategy: "native", provider: null, reason: "direct" },
    ]);
    expect(s.items.map((i) => i.kind)).toEqual(["task", "assistant"]);
  });

  it("normalizeLoaded repairs saved task items", () => {
    const t = { ...taskFixture("running"), finished_at: null, outcome: null };
    const [loaded] = normalizeLoaded([{ ...t, steps: [...t.steps, { ...t.steps[0]!, id: "z", status: "weird" }] }]) as TaskItem[];
    expect(loaded!.steps.find((x) => x.id === "b")!.status).toBe("stopped");
    expect(loaded!.steps.find((x) => x.id === "z")!.status).toBe("done");
    expect(loaded!.finished_at).toBe(t.started_at);
  });
});

describe("verdicts", () => {
  it("distinguishes verified, ran-but-not-verified and failed", () => {
    expect(verdictText(verdictOf(taskFixture("verified")))).toBe("Done · all 3 steps checked");
    expect(verdictText(verdictOf(taskFixture("unverified")))).toBe("Done · 2 of 3 steps checked");
    expect(verdictOf(taskFixture("failed"))).toMatchObject({ kind: "failed", at: 2 });
    expect(verdictOf(taskFixture("refused"))).toMatchObject({ kind: "failed", at: 3 });
    expect(verdictText(verdictOf(taskFixture("refused")))).toBe("Blocked at step 3");
    const none = { ...taskFixture("unverified"), steps: taskFixture("unverified").steps.map((s) => ({ ...s, status: "done" as const })) };
    expect(verdictText(verdictOf(none))).toBe("Done, but none of the 3 steps could be checked");
    // Never jargon like "step(s)" or "0 verified".
    for (const f of ["verified", "unverified", "failed", "refused", "declined", "stopped", "running", "web"] as const) expect(verdictText(verdictOf(taskFixture(f)))).not.toMatch(/\(s\)|\b0 verified|verified/);
  });
  it("a run without a plan that completes is 'answered'", () => {
    expect(verdictOf({ steps: [], outcome: "completed", confirms: [] }).kind).toBe("answered");
  });
});

describe("step state machine", () => {
  it("allows the documented transitions only", () => {
    expect(canTransition("pending", "running")).toBe(true);
    expect(canTransition("running", "verified")).toBe(true);
    expect(canTransition("failed", "running")).toBe(true); // retry
    expect(canTransition("verified", "failed")).toBe(false);
    expect(canTransition("done", "running")).toBe(false);
    for (const end of ["verified", "done", "skipped", "stopped"] as const) expect(STEP_TRANSITIONS[end]).toHaveLength(0);
  });
  it("finds the current step", () => {
    expect(currentStepIndex(taskFixture("running").steps)).toBe(1);
    expect(currentStepIndex(taskFixture("verified").steps)).toBe(-1);
  });
});

describe("labels", () => {
  it("maps strategies to route badges and providers to names", () => {
    expect(routeBadge("native")).toBe("native");
    expect(routeBadge("local_plan")).toBe("local");
    expect(routeBadge("cloud_agent")).toBe("cloud");
    expect(providerLabel("codex")).toBe("Codex");
    expect(providerLabel("claude")).toBe("Claude Code");
    expect(providerLabel(null)).toBe("Relay");
  });
  it("summarises the privacy ledger", () => {
    expect(ledgerSummary([{ items: 1 }, { items: 47 }], 12_480)).toBe("2 categories · 48 items · 12 KB");
    expect(ledgerSummary([{ items: 1 }], 300)).toBe("1 category · 1 item · 300 B");
  });
});
