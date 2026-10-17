// Task timeline logic: verdicts, labels and the step state machine. Pure; tested in task.test.ts.
// Wording rule: short, human and specific. "Checked" means Relay looked at the result afterwards;
// a step that ran but could not be looked at is "done, not checked", never success.
import type { TaskItem, TaskStep, TaskStepState, UsedTag } from "./chat";
import type { PlanSource, RouteStrategy, StepRisk, UsedKind } from "./schemas";

export type Verdict =
  | { kind: "running"; verified: number; finished: number; total: number }
  | { kind: "verified"; verified: number; total: number }
  | { kind: "unverified"; verified: number; ran: number; total: number }
  | { kind: "failed"; at: number; step: TaskStep | null; verified: number; total: number }
  | { kind: "stopped"; verified: number; total: number }
  | { kind: "declined"; verified: number; total: number }
  | { kind: "timed_out"; verified: number; total: number }
  | { kind: "waiting"; verified: number; total: number }
  | { kind: "answered" };

const FINISHED: readonly TaskStepState[] = ["verified", "done", "failed", "skipped", "stopped"];

/** What the run achieved. Only "verified" means every step's effect was checked afterwards. */
export function verdictOf(t: Pick<TaskItem, "steps" | "outcome" | "confirms"> & { awaiting_user?: boolean }): Verdict {
  const total = t.steps.length;
  const verified = t.steps.filter((s) => s.status === "verified").length;
  if (t.outcome === null) return { kind: "running", verified, finished: t.steps.filter((s) => FINISHED.includes(s.status)).length, total };
  const failedIdx = t.steps.findIndex((s) => s.status === "failed");
  if (failedIdx >= 0) return { kind: "failed", at: failedIdx + 1, step: t.steps[failedIdx] ?? null, verified, total };
  if (t.outcome === "cancelled") return { kind: "stopped", verified, total };
  if (t.outcome === "timed_out") return { kind: "timed_out", verified, total };
  const declined = t.confirms.some((c) => c.state === "declined");
  if (t.outcome === "failed") {
    if (declined) return { kind: "declined", verified, total };
    return { kind: "failed", at: 0, step: null, verified, total };
  }
  if (t.awaiting_user) return { kind: "waiting", verified, total };
  if (total === 0) return { kind: "answered" };
  if (declined && t.steps.some((s) => s.status === "skipped")) return { kind: "declined", verified, total };
  if (verified === total) return { kind: "verified", verified, total };
  const ran = t.steps.filter((s) => s.status === "verified" || s.status === "done").length;
  return { kind: "unverified", verified, ran, total };
}

const steps = (n: number) => `${n} ${n === 1 ? "step" : "steps"}`;

export function verdictText(v: Verdict): string {
  switch (v.kind) {
    case "running":
      return v.total ? `${v.finished} of ${steps(v.total)} done` : "Working";
    case "verified":
      return v.total === 1 ? "Done and checked" : `Done · all ${v.total} steps checked`;
    case "unverified":
      if (v.verified) return `Done · ${v.verified} of ${v.total} steps checked`;
      return v.ran === 1 && v.total === 1 ? "Done, but not checked" : `Done, but none of the ${steps(v.total)} could be checked`;
    case "failed":
      if (v.step && isRefusal(v.step.summary)) return `Blocked at step ${v.at}`;
      return v.at ? `Step ${v.at} didn't work` : "Didn't finish";
    case "stopped":
      return "Stopped";
    case "declined":
      return "Not done: you said no";
    case "timed_out":
      return "Took too long, so Relay stopped";
    case "waiting":
      return "Waiting for you";
    case "answered":
      return "Answered";
  }
}

/** One plain sentence under the verdict when it needs explaining (null when it speaks for itself). */
export function verdictHint(v: Verdict): string | null {
  switch (v.kind) {
    case "unverified": {
      const n = v.ran - v.verified;
      if (n <= 0) return null;
      if (!v.verified) return n === 1 ? "There was nothing on screen to check the result against." : "There was nothing on screen to check the results against.";
      return n === 1 ? "The other step ran, but there was nothing on screen to check it against." : `The other ${n} steps ran, but there was nothing on screen to check them against.`;
    }
    case "waiting":
      return "Do what the message below asks, then reply to continue.";
    case "declined":
      return "Nothing was changed.";
    default:
      return null;
  }
}

/** True when an assistant message asks the user to do something before Relay can go on. */
export function asksUser(text: string): boolean {
  const t = text.trim();
  if (!t) return false;
  if (/\?\s*$/.test(t)) return true;
  return /\b(then (say|type|reply|tell me|send)|say ["“']?continue|let me know when|once you(?:'ve| have)|please (sign|log) in|sign in (to|on|with)|log in (to|on|with)|your turn)\b/i.test(t);
}

/** Rust refuses steps before they run (disabled tool, path outside allowed folders). */
export function isRefusal(summary: string): boolean {
  return /^refused\b/i.test(summary.trim());
}

export type StepStateMeta = { label: string; description: string; tone: "neutral" | "running" | "success" | "danger" | "muted" };

export const STEP_STATE: Record<TaskStepState, StepStateMeta> = {
  pending: { label: "Up next", description: "Hasn't started yet.", tone: "muted" },
  running: { label: "Working", description: "Relay is doing this now.", tone: "running" },
  verified: { label: "Checked", description: "Done, and Relay checked the result on screen.", tone: "success" },
  done: {
    label: "Done, not checked",
    description: "Done, but there was nothing Relay could look at to confirm it worked.",
    tone: "neutral",
  },
  failed: { label: "Didn't work", description: "This step didn't work.", tone: "danger" },
  skipped: { label: "Skipped", description: "Relay didn't run this step.", tone: "muted" },
  stopped: { label: "Stopped", description: "Stopped before it finished.", tone: "muted" },
};

/** Label of a step's state, with "Blocked" for steps Relay refused before running them. */
export function stepStateLabel(step: Pick<TaskStep, "status" | "summary">): string {
  return step.status === "failed" && isRefusal(step.summary) ? "Blocked" : STEP_STATE[step.status].label;
}

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

// ---- step badges ------------------------------------------------------------------------------
// `risk: "sensitive"` alone is an internal policy class and is not shown. Users see:
//   "Changes or deletes data" for destructive steps, and "Sensitive info" only when the step really
//   touches sensitive information (`sensitive_data: true`).

export type StepBadge = { key: "destructive" | "sensitive_data"; label: string; description: string; tone: "danger" | "warning" };

export const DESTRUCTIVE_BADGE: StepBadge = {
  key: "destructive",
  label: "Changes or deletes data",
  description: "This changes or deletes something that may be hard to undo.",
  tone: "danger",
};
export const SENSITIVE_BADGE: StepBadge = {
  key: "sensitive_data",
  label: "Sensitive info",
  description: "This touches sensitive information, such as a password, an ID, a bank or card number, or a key file.",
  tone: "warning",
};

export function stepBadges(step: { risk: StepRisk | string; sensitive_data?: boolean }): StepBadge[] {
  const out: StepBadge[] = [];
  if (step.risk === "destructive") out.push(DESTRUCTIVE_BADGE);
  if (step.sensitive_data === true) out.push(SENSITIVE_BADGE);
  return out;
}

// ---- "what did the work" tags -------------------------------------------------------------------

export type UsedTone = "success" | "info" | "neutral" | "violet";
export const USED_META: Record<UsedKind, { name: string; tone: UsedTone; description: string }> = {
  local_ai: { name: "Local AI", tone: "success", description: "An AI model running on this PC. Nothing left this PC for it." },
  cloud_ai: { name: "Cloud AI", tone: "info", description: "A cloud AI provider. Only what your privacy settings allow is sent." },
  native: { name: "Native tools", tone: "neutral", description: "Relay's own tools, run directly on this PC." },
  mcp: { name: "MCP", tone: "violet", description: "A connected tool server (MCP), such as the browser." },
};

const SERVER_LABEL: Record<string, string> = { playwright: "Playwright browser", browser: "Browser" };

/** Friendly name of an MCP server ("playwright" -> "Playwright browser"). */
export function mcpServerLabel(server: string | null | undefined): string {
  if (!server) return "Connected tool";
  const k = server.trim().toLowerCase();
  return SERVER_LABEL[k] ?? server.replace(/[-_]+/g, " ").replace(/^./, (x) => x.toUpperCase());
}

/** Chip text: "Local AI · Qwen3 14B", "Cloud AI · Claude Code", "Native tools · Windows", "Playwright browser". */
export function usedChipText(t: UsedTag): string {
  if (t.kind === "mcp") return t.label || "Connected tool";
  const name = USED_META[t.kind].name;
  return t.label && t.label.toLowerCase() !== name.toLowerCase() ? `${name} · ${t.label}` : name;
}

/** The task's `used` tags, or (older backends and history) tags derived from its route and steps. */
export function usedTagsOf(t: Pick<TaskItem, "route" | "engine" | "steps"> & { used?: UsedTag[] }): UsedTag[] {
  if (t.used && t.used.length) return t.used;
  const out: UsedTag[] = [];
  const add = (tag: UsedTag) => {
    if (!out.some((x) => x.kind === tag.kind && x.label === tag.label)) out.push(tag);
  };
  const strategy = t.route?.strategy;
  if (strategy === "local_plan") add({ kind: "local_ai", label: t.engine?.kind === "local" ? t.engine.name : "Relay Local AI" });
  else if (strategy === "cloud_plan" || strategy === "cloud_agent") add({ kind: "cloud_ai", label: providerLabel(t.route?.provider) });
  else if (t.engine) add(t.engine.kind === "local" ? { kind: "local_ai", label: t.engine.name } : { kind: "cloud_ai", label: t.engine.name });
  const ran = t.steps.filter((s) => s.status !== "pending" && s.status !== "skipped");
  if (ran.some((s) => s.via !== "mcp") || strategy === "native") add({ kind: "native", label: "Windows" });
  for (const s of ran) if (s.via === "mcp") add({ kind: "mcp", label: mcpServerLabel(s.server) });
  return out;
}

/** Long step lists (web tasks) collapse to the step that matters now plus failed ones. */
export const COLLAPSE_STEPS_AT = 6;

/** Indexes of the steps to show when the list is collapsed. */
export function collapsedStepIndexes(list: Pick<TaskStep, "status">[]): number[] {
  if (list.length <= COLLAPSE_STEPS_AT) return list.map((_, i) => i);
  let focus = list.findIndex((s) => s.status === "running");
  if (focus < 0) {
    for (let i = list.length - 1; i >= 0; i--)
      if (list[i]!.status !== "pending" && list[i]!.status !== "skipped") {
        focus = i;
        break;
      }
  }
  if (focus < 0) focus = 0;
  const keep = new Set<number>([focus]);
  list.forEach((s, i) => {
    if (s.status === "failed") keep.add(i);
  });
  return [...keep].sort((a, b) => a - b);
}

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

// Kept short: the badge sits next to the route reason in the task header.
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
