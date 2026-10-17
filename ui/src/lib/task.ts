// Task timeline logic: verdicts, labels and the step state machine. Pure; tested in task.test.ts.
import type { TaskItem, TaskStep, TaskStepState } from "./chat";
import type { PlanSource, RouteStrategy, StepRisk } from "./schemas";

export type Verdict =
  | { kind: "running"; verified: number; total: number }
  | { kind: "verified"; verified: number; total: number }
  | { kind: "unverified"; verified: number; ran: number; total: number }
  | { kind: "failed"; at: number; step: TaskStep | null; verified: number; total: number }
  | { kind: "stopped"; verified: number; total: number }
  | { kind: "declined"; verified: number; total: number }
  | { kind: "timed_out"; verified: number; total: number }
  | { kind: "answered" };

/** What the run achieved. Only "verified" means every step's effect was checked afterwards. */
export function verdictOf(t: Pick<TaskItem, "steps" | "outcome" | "confirms">): Verdict {
  const total = t.steps.length;
  const verified = t.steps.filter((s) => s.status === "verified").length;
  if (t.outcome === null) return { kind: "running", verified, total };
  const failedIdx = t.steps.findIndex((s) => s.status === "failed");
  if (failedIdx >= 0) return { kind: "failed", at: failedIdx + 1, step: t.steps[failedIdx] ?? null, verified, total };
  if (t.outcome === "cancelled") return { kind: "stopped", verified, total };
  if (t.outcome === "timed_out") return { kind: "timed_out", verified, total };
  const declined = t.confirms.some((c) => c.state === "declined");
  if (t.outcome === "failed") {
    if (declined) return { kind: "declined", verified, total };
    return { kind: "failed", at: 0, step: null, verified, total };
  }
  if (total === 0) return { kind: "answered" };
  if (declined && t.steps.some((s) => s.status === "skipped")) return { kind: "declined", verified, total };
  if (verified === total) return { kind: "verified", verified, total };
  const ran = t.steps.filter((s) => s.status === "verified" || s.status === "done").length;
  return { kind: "unverified", verified, ran, total };
}

export function verdictText(v: Verdict): string {
  switch (v.kind) {
    case "running":
      return v.total ? `${v.verified} of ${v.total} steps verified so far` : "Working";
    case "verified":
      return `Verified · ${v.verified} of ${v.total} ${v.total === 1 ? "step" : "steps"}`;
    case "unverified":
      return v.verified
        ? `Ran, ${v.verified} of ${v.total} verified`
        : `Ran but not verified · ${v.ran} of ${v.total} ${v.total === 1 ? "step" : "steps"}`;
    case "failed":
      if (v.step && isRefusal(v.step.summary)) return `Refused at step ${v.at}`;
      return v.at ? `Failed at step ${v.at}` : "Failed";
    case "stopped":
      return "Stopped";
    case "declined":
      return "Not done: you declined a step";
    case "timed_out":
      return "Timed out";
    case "answered":
      return "Answered";
  }
}

/** Rust refuses steps before they run (disabled tool, path outside allowed folders). */
export function isRefusal(summary: string): boolean {
  return /^refused\b/i.test(summary.trim());
}

export type StepStateMeta = { label: string; description: string; tone: "neutral" | "running" | "success" | "danger" | "muted" };

export const STEP_STATE: Record<TaskStepState, StepStateMeta> = {
  pending: { label: "Pending", description: "Not started yet.", tone: "muted" },
  running: { label: "Running", description: "Relay is carrying out this step now.", tone: "running" },
  verified: { label: "Verified", description: "Ran, and Relay checked afterwards that it had the intended effect.", tone: "success" },
  done: {
    label: "Done",
    description: "Ran without errors. There was nothing Relay could check afterwards, so it is not verified.",
    tone: "neutral",
  },
  failed: { label: "Failed", description: "Did not succeed, or was refused before running.", tone: "danger" },
  skipped: { label: "Skipped", description: "Not run.", tone: "muted" },
  stopped: { label: "Stopped", description: "Interrupted when the task was stopped.", tone: "muted" },
};

/** Allowed step transitions; anything else from the backend is accepted but flagged in tests. */
export const STEP_TRANSITIONS: Record<TaskStepState, readonly TaskStepState[]> = {
  pending: ["running", "skipped", "failed", "verified", "done"],
  running: ["verified", "done", "failed", "skipped", "stopped"],
  verified: [],
  done: [],
  failed: ["running"], // a retry starts the step again
  skipped: [],
  stopped: [],
};

export function canTransition(from: TaskStepState, to: TaskStepState): boolean {
  return STEP_TRANSITIONS[from].includes(to);
}

export const RISK_META: Record<StepRisk, { label: string; description: string }> = {
  safe: { label: "Safe", description: "Reads or opens things; changes nothing that matters." },
  sensitive: { label: "Sensitive", description: "Touches personal data or changes something you can undo." },
  destructive: { label: "Destructive", description: "Deletes, sends or changes something that may not be undoable." },
};

export type RouteBadge = "native" | "local" | "cloud" | "unavailable";

export function routeBadge(strategy: RouteStrategy): RouteBadge {
  switch (strategy) {
    case "native":
      return "native";
    case "local_plan":
      return "local";
    case "cloud_plan":
    case "cloud_agent":
      return "cloud";
    case "unavailable":
      return "unavailable";
  }
}

export const ROUTE_BADGE_LABEL: Record<RouteBadge, string> = {
  native: "On this PC",
  local: "Relay Local AI",
  cloud: "Cloud",
  unavailable: "Unavailable",
};

const PROVIDER_LABEL: Record<string, string> = { claude: "Claude Code", codex: "Codex", local: "Relay Local AI", native: "Relay" };

export function providerLabel(id: string | null | undefined): string {
  if (!id) return "Relay";
  return PROVIDER_LABEL[id] ?? id;
}

export function planSourceLabel(source: PlanSource): string {
  return source === "native" ? "Relay (built in)" : providerLabel(source);
}

const CATEGORY_LABEL: Record<string, string> = {
  window_titles: "Window titles",
  ui_tree: "UI controls",
  file_names: "File names",
  file_contents: "File contents",
  screenshots: "Screenshots",
  task: "Your request",
  history: "Earlier messages",
};

export function categoryLabel(c: string): string {
  return CATEGORY_LABEL[c] ?? c.replace(/_/g, " ").replace(/^./, (x) => x.toUpperCase());
}

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "unknown size";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

/** "3 categories · 41 items · 12 KB" */
export function ledgerSummary(categories: { items: number }[], bytes: number): string {
  const items = categories.reduce((n, c) => n + c.items, 0);
  return `${categories.length} ${categories.length === 1 ? "category" : "categories"} · ${items} ${items === 1 ? "item" : "items"} · ${formatSize(bytes)}`;
}

/** Index of the step that is running now, or the first pending one. */
export function currentStepIndex(steps: TaskStep[]): number {
  const r = steps.findIndex((s) => s.status === "running");
  return r >= 0 ? r : steps.findIndex((s) => s.status === "pending");
}
