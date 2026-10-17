import { describe, expect, it } from "vitest";
import { matchesFilter, routeReason, workflowToTask } from "./workflow";
import { verdictOf } from "./task";
import type { Workflow } from "./schemas";

const base: Workflow = {
  id: "w1",
  conv_id: "c1",
  objective: "Empty the Recycle Bin",
  strategy: "cloud_plan",
  provider: "codex",
  outcome: "declined",
  steps_total: 2,
  steps_verified: 1,
  created_at: 1000,
  updated_at: 5000,
  plan_source: "codex",
  steps: [
    { id: "a", title: "Check", tool: "list_files", risk: "safe", status: "verified", summary: "212 items", attempts: 1 },
    { id: "b", title: "Empty", tool: "empty_recycle_bin", risk: "destructive", status: "skipped", summary: "Declined", attempts: 0 },
  ],
  shared: [{ category: "file_names", items: 212 }],
  shared_bytes: 4000,
  approvals: ["Declined: Empty the Recycle Bin"],
  errors: [],
  notes: null,
};

describe("workflow history", () => {
  it("maps a stored workflow onto the timeline shape", () => {
    const t = workflowToTask(base);
    expect(t.route).toMatchObject({ strategy: "cloud_plan", provider: "codex", reason: "Hybrid: Codex planned it, Relay ran it here" });
    expect(t.shared[0]).toMatchObject({ provider: "codex", bytes: 4000 });
    expect(t.steps.map((s) => s.status)).toEqual(["verified", "skipped"]);
    expect(t.confirms[0]).toMatchObject({ state: "declined", title: "Empty the Recycle Bin" });
    expect(verdictOf(t).kind).toBe("declined");
  });
  it("tolerates unknown strings from the backend", () => {
    const t = workflowToTask({ ...base, strategy: "quantum", plan_source: "?", steps: [{ ...base.steps[0]!, status: "??", risk: "??" }] });
    expect(t.route?.strategy).toBe("cloud_plan");
    expect(t.plan?.source).toBe("native");
    expect(t.steps[0]).toMatchObject({ status: "done", risk: "safe" });
  });
  it("filters by outcome", () => {
    const ok = { ...base, outcome: "completed" as const, steps_verified: 2 };
    expect(matchesFilter(ok, "verified")).toBe(true);
    expect(matchesFilter({ ...ok, steps_verified: 1 }, "verified")).toBe(false);
    expect(matchesFilter({ ...base, outcome: "denied" }, "failed")).toBe(true);
    expect(matchesFilter(base, "stopped")).toBe(true);
    expect(matchesFilter(base, "all")).toBe(true);
  });
  it("describes routes in plain words", () => {
    expect(routeReason("native", null)).toMatch(/directly/);
    expect(routeReason("local_plan", "local")).toMatch(/Relay Local AI/);
    expect(routeReason("cloud_agent", "claude")).toMatch(/Claude Code drove/);
  });
});
