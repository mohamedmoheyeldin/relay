// Task timeline fixtures built by replaying contract events through the real reducer.
// Used by the component gallery and the tests, so both always match the live behaviour.
import { applyEvent, emptyChat, type ChatEvent, type TaskItem } from "./chat";
import type { PlanStep, StepFinishStatus } from "./schemas";

export type FixtureName = "running" | "verified" | "unverified" | "failed" | "refused" | "declined" | "stopped" | "awaiting" | "local" | "native" | "web" | "sensitive";

const STEPS: PlanStep[] = [
  { id: "a", title: "List files in Downloads", tool: "list_files", risk: "safe", via: "native" },
  { id: "b", title: "Find installers older than 30 days", tool: "find_files", risk: "safe", via: "native" },
  { id: "c", title: "Move 23 files to the Recycle Bin", tool: "recycle_files", risk: "destructive", via: "native" },
];

/** A web task: 12 browser steps through the Playwright MCP server. */
export const WEB_STEPS: PlanStep[] = Array.from({ length: 12 }, (_, i) => ({
  id: `w${i + 1}`,
  title: i === 0 ? "Open dice.com" : i === 11 ? "Open Easy Apply for the top match" : `Browser action ${i + 1}`,
  tool: i === 0 ? "browser_navigate" : "browser_click",
  risk: "safe",
  via: "mcp",
  server: "playwright",
}));

function build(events: ChatEvent[], t0 = Date.now() - 20_000): TaskItem {
  let s = emptyChat();
  events.forEach((e, i) => (s = applyEvent(s, e, t0 + i * 1500)));
  const t = s.items.find((i): i is TaskItem => i.kind === "task");
  if (!t) throw new Error("fixture produced no task");
  return t;
}

function head(run: number, strategy: "cloud_plan" | "local_plan" | "native" = "cloud_plan"): ChatEvent[] {
  const ev: ChatEvent[] = [{ type: "run_started", run_id: run, conv_id: "c" }];
  if (strategy === "cloud_plan") {
    ev.push({ type: "route", run_id: run, strategy, provider: "claude", reason: "Claude Code makes the plan; Relay does the steps on this PC" });
    ev.push({ type: "used", run_id: run, kind: "cloud_ai", label: "Claude Code" });
    ev.push({
      type: "context_shared",
      run_id: run,
      provider: "claude",
      categories: [
        { category: "task", items: 1 },
        { category: "window_titles", items: 6 },
        { category: "file_names", items: 41 },
      ],
      bytes: 12_480,
    });
  } else if (strategy === "local_plan") {
    ev.push({ type: "route", run_id: run, strategy, provider: "local", reason: "Relay Local AI makes the plan on this PC; nothing leaves it" });
    ev.push({ type: "used", run_id: run, kind: "local_ai", label: "Qwen3 14B" });
  } else {
    ev.push({ type: "route", run_id: run, strategy, provider: null, reason: "Relay can do this itself: open an app and type" });
  }
  ev.push({ type: "used", run_id: run, kind: "native", label: "Windows" });
  ev.push({
    type: "plan_proposed",
    run_id: run,
    source: strategy === "cloud_plan" ? "claude" : strategy === "local_plan" ? "local" : "native",
    objective: "Tidy up the Downloads folder",
    notes: "Files go to the Recycle Bin, so they can be restored.",
    steps: STEPS,
  });
  return ev;
}

function step(run: number, id: string, status: StepFinishStatus, summary: string): ChatEvent[] {
  return [
    { type: "step_started", run_id: run, step_id: id },
    { type: "step_finished", run_id: run, step_id: id, status, summary },
  ];
}

export function taskFixture(name: FixtureName): TaskItem {
  const r = 7;
  switch (name) {
    case "running":
      return build([...head(r), ...step(r, "a", "verified", "1,284 files, 18.3 GB"), { type: "step_started", run_id: r, step_id: "b" }]);
    case "awaiting":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files"),
        ...step(r, "b", "done", "14 matches"),
        { type: "step_started", run_id: r, step_id: "c" },
        { type: "confirm", id: 3, run_id: r, title: "Move 23 files to the Recycle Bin?", detail: "C:\\Users\\you\\Downloads", risk: "destructive" },
      ]);
    case "verified":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files, 18.3 GB"),
        ...step(r, "b", "verified", "14 matches, re-listed"),
        ...step(r, "c", "verified", "23 files are in the Recycle Bin"),
        { type: "run_finished", run_id: r, outcome: "completed" },
      ]);
    case "unverified":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files"),
        ...step(r, "b", "done", "14 matches"),
        ...step(r, "c", "verified", "23 files are in the Recycle Bin"),
        { type: "run_finished", run_id: r, outcome: "completed" },
      ]);
    case "failed":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files"),
        ...step(r, "b", "failed", "Ran, but the check failed: no files matched"),
        { type: "step_finished", run_id: r, step_id: "c", status: "skipped", summary: "Not run because an earlier step failed" },
        { type: "run_finished", run_id: r, outcome: "failed" },
      ]);
    case "refused":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files"),
        ...step(r, "b", "done", "14 matches"),
        ...step(r, "c", "failed", "Refused: C:\\Windows is a protected folder"),
        { type: "run_finished", run_id: r, outcome: "failed" },
      ]);
    case "declined":
      return build([
        ...head(r),
        ...step(r, "a", "verified", "1,284 files"),
        ...step(r, "b", "done", "14 matches"),
        { type: "step_started", run_id: r, step_id: "c" },
        { type: "confirm", id: 4, run_id: r, title: "Move 23 files to the Recycle Bin?", detail: "", risk: "destructive" },
        { type: "confirm_resolved", id: 4, approved: false },
        { type: "step_finished", run_id: r, step_id: "c", status: "skipped", summary: "Declined by you; not run" },
        { type: "run_finished", run_id: r, outcome: "failed" },
      ]);
    case "stopped":
      return build([...head(r), ...step(r, "a", "verified", "1,284 files"), { type: "step_started", run_id: r, step_id: "b" }, { type: "run_finished", run_id: r, outcome: "cancelled" }]);
    case "local":
      return build([...head(r, "local_plan"), ...step(r, "a", "verified", "1,284 files"), ...step(r, "b", "done", "14 matches"), ...step(r, "c", "verified", "Moved"), { type: "run_finished", run_id: r, outcome: "completed" }]);
    case "native":
      return build([...head(r, "native"), ...step(r, "a", "verified", "OK"), ...step(r, "b", "verified", "OK"), ...step(r, "c", "verified", "OK"), { type: "run_finished", run_id: r, outcome: "completed" }]);
    case "web": {
      const ev: ChatEvent[] = [
        { type: "run_started", run_id: r, conv_id: "c" },
        { type: "route", run_id: r, strategy: "cloud_plan", provider: "claude", reason: "Claude Code works out each browser step; Relay does them", intent: "web" },
        { type: "used", run_id: r, kind: "cloud_ai", label: "Claude Code" },
        { type: "used", run_id: r, kind: "mcp", label: "Playwright browser" },
        { type: "plan_proposed", run_id: r, source: "claude", objective: "Find remote Rust jobs on Dice", notes: null, steps: [] },
      ];
      for (const s of WEB_STEPS) ev.push({ type: "step_added", run_id: r, step: s }, ...step(r, s.id, s.id === "w12" ? "done" : "verified", "OK"));
      ev.push({ type: "assistant_message", run_id: r, text: "Sign in to Dice in the Relay browser window, then say continue." });
      ev.push({ type: "run_finished", run_id: r, outcome: "completed" });
      return build(ev);
    }
    case "sensitive":
      return build([
        { type: "run_started", run_id: r, conv_id: "c" },
        { type: "route", run_id: r, strategy: "native", provider: null, reason: "Relay can do this itself" },
        {
          type: "plan_proposed",
          run_id: r,
          source: "native",
          objective: "Fill in the form",
          notes: null,
          steps: [
            { id: "s1", title: "Type the note", tool: "type_text", risk: "sensitive" },
            { id: "s2", title: "Paste the saved password", tool: "type_text", risk: "sensitive", sensitive_data: true },
            { id: "s3", title: "Delete the draft", tool: "delete_files", risk: "destructive" },
          ],
        },
      ]);
  }
}
