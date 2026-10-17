// Task timeline fixtures built by replaying contract events through the real reducer.
// Used by the component gallery and the tests, so both always match the live behaviour.
import { applyEvent, emptyChat, type ChatEvent, type TaskItem } from "./chat";
import type { PlanStep, StepFinishStatus } from "./schemas";

export type FixtureName = "running" | "verified" | "unverified" | "failed" | "refused" | "declined" | "stopped" | "awaiting" | "local" | "native";

const STEPS: PlanStep[] = [
  { id: "a", title: "List files in Downloads", tool: "list_files", risk: "safe" },
  { id: "b", title: "Find installers older than 30 days", tool: "find_files", risk: "safe" },
  { id: "c", title: "Move 23 files to the Recycle Bin", tool: "recycle_files", risk: "destructive" },
];

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
    ev.push({ type: "route", run_id: run, strategy, provider: "claude", reason: "Hybrid: Claude Code plans it, Relay runs it here" });
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
    ev.push({ type: "route", run_id: run, strategy, provider: "local", reason: "Private: Relay Local AI plans it on this PC" });
  } else {
    ev.push({ type: "route", run_id: run, strategy, provider: null, reason: "Relay can do this directly: open an app and type" });
  }
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
  }
}
