// Task history helpers: map a stored Workflow onto the timeline's TaskItem shape, outcome labels
// and filters. Pure; tested in workflow.test.ts.
import type { TaskConfirm, TaskItem, TaskStepState } from "./chat";
import type { PlanSource, RouteStrategy, StepRisk, Workflow, WorkflowOutcome, WorkflowSummary } from "./schemas";
import { providerLabel } from "./task";

const STRATEGIES: readonly RouteStrategy[] = ["native", "local_plan", "cloud_plan", "cloud_agent", "unavailable"];
const SOURCES: readonly PlanSource[] = ["native", "local", "claude", "codex"];
const RISKS: readonly StepRisk[] = ["safe", "sensitive", "destructive"];
const STATES: readonly TaskStepState[] = ["pending", "running", "verified", "done", "failed", "skipped", "stopped"];

export function strategyOf(s: string): RouteStrategy {
  return (STRATEGIES as readonly string[]).includes(s) ? (s as RouteStrategy) : "cloud_plan";
}

export function routeReason(strategy: string, provider: string | null): string {
  switch (strategyOf(strategy)) {
    case "native":
      return "Relay did this directly on this PC";
    case "local_plan":
      return "Private: Relay Local AI planned it on this PC";
    case "cloud_plan":
      return `Hybrid: ${providerLabel(provider)} planned it, Relay ran it here`;
    case "cloud_agent":
      return `Agent mode: ${providerLabel(provider)} drove the desktop`;
    case "unavailable":
      return "No engine was available for this task";
  }
}

export const OUTCOME_META: Record<WorkflowOutcome, { label: string; tone: "success" | "danger" | "warning" | "neutral" }> = {
  completed: { label: "Completed", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
  cancelled: { label: "Stopped", tone: "neutral" },
  declined: { label: "Declined", tone: "warning" },
  denied: { label: "Refused", tone: "danger" },
  timed_out: { label: "Timed out", tone: "warning" },
  unavailable: { label: "Unavailable", tone: "neutral" },
};

export type OutcomeFilter = "all" | "verified" | "completed" | "failed" | "stopped";

export function matchesFilter(w: WorkflowSummary, f: OutcomeFilter): boolean {
  switch (f) {
    case "all":
      return true;
    case "verified":
      return w.outcome === "completed" && w.steps_total > 0 && w.steps_verified === w.steps_total;
    case "completed":
      return w.outcome === "completed";
    case "failed":
      return w.outcome === "failed" || w.outcome === "denied" || w.outcome === "unavailable";
    case "stopped":
      return w.outcome === "cancelled" || w.outcome === "declined" || w.outcome === "timed_out";
  }
}

/** Builds a read-only TaskItem from a stored workflow so the timeline component can show it. */
export function workflowToTask(w: Workflow): TaskItem {
  const outcome = w.outcome === "completed" ? "completed" : w.outcome === "cancelled" ? "cancelled" : w.outcome === "timed_out" ? "timed_out" : "failed";
  const confirms: TaskConfirm[] = w.approvals.map((a, i) => {
    const declined = /^(declined|denied)\b/i.test(a);
    return {
      confirm_id: -(i + 1),
      title: a.replace(/^(approved|declined|denied)\s*:\s*/i, ""),
      detail: "",
      risk: "destructive",
      state: declined ? "declined" : "approved",
      step_id: null,
      at: w.created_at,
    };
  });
  return {
    kind: "task",
    id: `wf-${w.id}`,
    run_id: -1,
    route: { strategy: strategyOf(w.strategy), provider: w.provider, reason: routeReason(w.strategy, w.provider) },
    previous_routes: [],
    engine: null,
    shared: w.shared.length ? [{ provider: w.provider ?? "cloud", categories: w.shared, bytes: w.shared_bytes }] : [],
    plan: {
      source: (SOURCES as readonly string[]).includes(w.plan_source) ? (w.plan_source as PlanSource) : "native",
      objective: w.objective,
      notes: w.notes,
    },
    steps: w.steps.map((s) => ({
      id: s.id,
      title: s.title,
      tool: s.tool,
      risk: (RISKS as readonly string[]).includes(s.risk) ? (s.risk as StepRisk) : "safe",
      status: (STATES as readonly string[]).includes(s.status) ? (s.status as TaskStepState) : "done",
      summary: s.summary,
      started_at: null,
      finished_at: null,
      attempts: s.attempts,
    })),
    confirms,
    started_at: w.created_at,
    finished_at: w.updated_at,
    outcome,
  };
}

export function formatWhen(t: number, now = Date.now()): string {
  const d = new Date(t);
  const today = new Date(now);
  const sameDay = d.toDateString() === today.toDateString();
  const time = d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  if (sameDay) return `Today ${time}`;
  const y = new Date(now - 86_400_000);
  if (d.toDateString() === y.toDateString()) return `Yesterday ${time}`;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric", ...(d.getFullYear() !== today.getFullYear() ? { year: "numeric" } : {}) });
}
