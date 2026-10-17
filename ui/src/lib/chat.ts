// Pure reducer: turns relay events into the items a conversation renders (and persists via conv_save).
// No React, no I/O: unit tested in chat.test.ts and task.test.ts.
//
// Two shapes of run:
// - Plan runs (hybrid): route / context_shared / plan_proposed / step_* events build one TaskItem,
//   the structured task timeline. The final assistant_message is the outcome in words.
// - Agent runs (cloud_agent): tool_started / tool_finished and text deltas build a ToolsItem
//   ("Worked for 12s · 7 steps") plus the assistant's answer.
//
// v3: a route with `intent: "chat"` is a plain question. It never creates a task card; the reply is a
// normal assistant message carrying the `used` chips. Web tasks grow their step list with `step_added`.
import type {
  ErrorKind,
  Intent,
  PlanSource,
  PlanStep,
  Risk,
  RouteStrategy,
  RunOutcome,
  SharedCategory,
  StepRisk,
  UiEvent,
  UsedKind,
  Via,
} from "./schemas";
import { asksUser } from "./task";

export type StepStatus = "running" | "ok" | "failed" | "interrupted";

export type ToolStep =
  | {
      kind: "tool";
      id: string;
      name: string;
      summary: string;
      result: string | null;
      status: StepStatus;
      started_at: number;
      finished_at: number | null;
    }
  | { kind: "note"; id: string; text: string }
  | {
      kind: "confirm";
      id: string;
      confirm_id: number;
      title: string;
      detail: string;
      risk: Risk;
      state: "pending" | "approved" | "declined";
    };

export type EngineTag = { kind: "local" | "cloud"; name: string };
/** What did the work in a run (`used` event): Local AI, Cloud AI, native tools or an MCP server. */
export type UsedTag = { kind: UsedKind; label: string; detail?: string | null };

/** Plan step lifecycle. "verified" = effect inspected afterwards; "done" = ran, nothing to verify against. */
export type TaskStepState = "pending" | "running" | "verified" | "done" | "failed" | "skipped" | "stopped";

export type TaskStep = {
  id: string;
  title: string;
  tool: string;
  risk: StepRisk;
  status: TaskStepState;
  summary: string;
  started_at: number | null;
  finished_at: number | null;
  attempts: number;
  /** How the step runs: Relay's own tools or an MCP server (v3; absent in older data). */
  via?: Via | null;
  server?: string | null;
  /** True only when the step touches real sensitive information (passwords, ID or card numbers...). */
  sensitive_data?: boolean;
};

export type TaskConfirm = {
  confirm_id: number;
  title: string;
  detail: string;
  risk: Risk;
  state: "pending" | "approved" | "declined";
  step_id: string | null;
  at: number;
};

export type TaskRoute = { strategy: RouteStrategy; provider: string | null; reason: string };
export type TaskShare = { provider: string; categories: SharedCategory[]; bytes: number };

export type TaskItem = {
  kind: "task";
  id: string;
  run_id: number;
  route: TaskRoute | null;
  /** Earlier routes of this run (a fallback re-routes), oldest first. */
  previous_routes: TaskRoute[];
  engine: EngineTag | null;
  shared: TaskShare[];
  plan: { source: PlanSource; objective: string; notes: string | null } | null;
  steps: TaskStep[];
  confirms: TaskConfirm[];
  started_at: number;
  finished_at: number | null;
  outcome: RunOutcome | null;
  /** v3 fields; optional so chats saved by older versions still load. */
  intent?: Intent | null;
  used?: UsedTag[];
  /** The run ended by asking the user to do something (e.g. sign in, then say continue). */
  awaiting_user?: boolean;
};

export type UserItem = { kind: "user"; id: string; text: string; attachments: string[]; at: number };
export type AssistantItem = {
  kind: "assistant";
  id: string;
  run_id: number | null;
  text: string;
  streaming: boolean;
  engine: EngineTag | null;
  /** What produced this message (chips under chat answers). */
  used?: UsedTag[];
  /** Context a chat answer sent to a cloud provider (chat answers have no task card to show it). */
  shared?: TaskShare[];
  /** The message asks the user to act before Relay can go on. */
  asks_user?: boolean;
};
export type ToolsItem = {
  kind: "tools";
  id: string;
  run_id: number;
  steps: ToolStep[];
  started_at: number;
  finished_at: number | null;
  outcome: RunOutcome | null;
};
export type NoticeItem = { kind: "notice"; id: string; text: string };
export type ErrorItem = { kind: "error"; id: string; error: ErrorKind; message: string; run_id: number | null };

export type Item = UserItem | AssistantItem | ToolsItem | TaskItem | NoticeItem | ErrorItem;

export type RunCursor = {
  run_id: number;
  started_at: number;
  openTextId: string | null;
  groupId: string | null;
  taskId: string | null;
  engine: EngineTag | null;
  intent: Intent | null;
  used: UsedTag[];
  shared: TaskShare[];
};

export type ChatState = { items: Item[]; run: RunCursor | null; seq: number };

export const emptyChat = (): ChatState => ({ items: [], run: null, seq: 0 });

/** Events the per-conversation reducer understands (global ones are handled by the store). */
export type ChatEvent = Extract<
  UiEvent,
  {
    type:
      | "run_started"
      | "route"
      | "used"
      | "step_added"
      | "context_shared"
      | "plan_proposed"
      | "step_started"
      | "step_finished"
      | "assistant_delta"
      | "assistant_message"
      | "tool_started"
      | "tool_finished"
      | "confirm"
      | "confirm_resolved"
      | "run_finished"
      | "error"
      | "engine"
      | "notice";
  }
>;

function nextId(s: ChatState, now: number): [string, number] {
  const seq = s.seq + 1;
  return [`${now.toString(36)}-${seq}`, seq];
}

export function addUser(s: ChatState, text: string, attachments: string[], now: number): ChatState {
  const [id, seq] = nextId(s, now);
  const item: UserItem = { kind: "user", id, text, attachments, at: now };
  return { ...s, seq, items: [...s.items, item] };
}

function newCursor(runId: number, now: number): RunCursor {
  return { run_id: runId, started_at: now, openTextId: null, groupId: null, taskId: null, engine: null, intent: null, used: [], shared: [] };
}

function addUsed(list: UsedTag[] | undefined, tag: UsedTag): UsedTag[] {
  const cur = list ?? [];
  const i = cur.findIndex((t) => t.kind === tag.kind && t.label === tag.label);
  if (i < 0) return [...cur, tag];
  return cur.map((t, j) => (j === i ? { ...t, detail: tag.detail ?? t.detail ?? null } : t));
}

/** Plan step fields from the contract, kept on the timeline step. */
function stepInfo(p: PlanStep): Pick<TaskStep, "title" | "tool" | "risk" | "via" | "server" | "sensitive_data"> {
  return {
    title: p.title,
    tool: p.tool,
    risk: p.risk,
    via: p.via ?? null,
    server: p.server ?? null,
    sensitive_data: p.sensitive_data === true,
  };
}

function pendingStep(p: PlanStep): TaskStep {
  return { id: p.id, ...stepInfo(p), status: "pending", summary: "", started_at: null, finished_at: null, attempts: 0 };
}

function ensureRun(s: ChatState, runId: number, now: number): ChatState {
  if (s.run && s.run.run_id === runId) return s;
  return { ...s, run: newCursor(runId, now) };
}

function closeOpenText(s: ChatState): ChatState {
  const run = s.run;
  if (!run || !run.openTextId) return s;
  const id = run.openTextId;
  return {
    ...s,
    run: { ...run, openTextId: null },
    items: s.items.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, streaming: false } : it)),
  };
}

/** Moves the run's assistant text that sits after its tool group (or anywhere, before the group exists)
 *  into the group as narration notes, and returns the state plus the group id. */
function intoGroup(s: ChatState, runId: number, now: number): [ChatState, string] {
  let st = closeOpenText(s);
  const run = st.run!;
  const groupIdx = run.groupId ? st.items.findIndex((i) => i.id === run.groupId) : -1;
  const notes: ToolStep[] = [];
  let firstRemoved = -1;
  const kept: Item[] = [];
  st.items.forEach((it, idx) => {
    if (it.kind === "assistant" && it.run_id === runId && idx > groupIdx) {
      if (firstRemoved < 0) firstRemoved = kept.length;
      if (it.text.trim()) notes.push({ kind: "note", id: it.id, text: it.text });
      return;
    }
    kept.push(it);
  });
  if (groupIdx >= 0 && run.groupId) {
    const gid = run.groupId;
    return [
      {
        ...st,
        items: kept.map((it) => (it.id === gid && it.kind === "tools" ? { ...it, steps: [...it.steps, ...notes] } : it)),
      },
      gid,
    ];
  }
  const [id, seq] = nextId(st, now);
  const group: ToolsItem = {
    kind: "tools",
    id,
    run_id: runId,
    steps: notes,
    started_at: run.started_at,
    finished_at: null,
    outcome: null,
  };
  const at = firstRemoved >= 0 ? firstRemoved : kept.length;
  const items = [...kept.slice(0, at), group, ...kept.slice(at)];
  st = { ...st, seq, items, run: { ...run, groupId: id } };
  return [st, id];
}

function mapGroup(s: ChatState, gid: string, f: (g: ToolsItem) => ToolsItem): ChatState {
  return { ...s, items: s.items.map((it) => (it.id === gid && it.kind === "tools" ? f(it) : it)) };
}

/** Returns the run's task item, creating it (after the last user message of the run) if needed. */
function ensureTask(s: ChatState, runId: number, now: number): [ChatState, string] {
  const st = ensureRun(s, runId, now);
  const run = st.run!;
  if (run.taskId && st.items.some((i) => i.id === run.taskId)) return [st, run.taskId];
  const existing = st.items.find((i): i is TaskItem => i.kind === "task" && i.run_id === runId);
  if (existing) return [{ ...st, run: { ...run, taskId: existing.id } }, existing.id];
  const [id, seq] = nextId(st, now);
  const task: TaskItem = {
    kind: "task",
    id,
    run_id: runId,
    route: null,
    previous_routes: [],
    engine: run.engine,
    shared: run.shared,
    plan: null,
    steps: [],
    confirms: [],
    started_at: run.started_at,
    finished_at: null,
    outcome: null,
    intent: run.intent,
    used: run.used,
  };
  // Place it before any text this run already streamed, so the timeline reads top-down.
  const firstText = st.items.findIndex((i) => (i.kind === "assistant" || i.kind === "tools") && i.run_id === runId);
  const items = firstText >= 0 ? [...st.items.slice(0, firstText), task, ...st.items.slice(firstText)] : [...st.items, task];
  return [{ ...st, seq, items, run: { ...run, taskId: id } }, id];
}

function mapTask(s: ChatState, tid: string, f: (t: TaskItem) => TaskItem): ChatState {
  return { ...s, items: s.items.map((it) => (it.id === tid && it.kind === "task" ? f(it) : it)) };
}

function taskOfRun(s: ChatState, runId: number): TaskItem | undefined {
  if (s.run?.run_id === runId && s.run.taskId) {
    const t = s.items.find((i) => i.id === s.run!.taskId);
    if (t && t.kind === "task") return t;
  }
  return s.items.find((i): i is TaskItem => i.kind === "task" && i.run_id === runId);
}

export function applyEvent(s: ChatState, ev: ChatEvent, now: number): ChatState {
  switch (ev.type) {
    case "run_started":
      return { ...s, run: newCursor(ev.run_id, now) };

    case "engine": {
      const st = ensureRun(s, ev.run_id, now);
      const engine = { kind: ev.kind, name: ev.name };
      const next = { ...st, run: { ...st.run!, engine } };
      const t = taskOfRun(next, ev.run_id);
      return t ? mapTask(next, t.id, (x) => ({ ...x, engine })) : next;
    }

    case "route": {
      const intent = ev.intent ?? null;
      if (intent === "chat") {
        // A question: no task card. Drop an empty card this run may already have (nothing to show).
        const st = ensureRun(s, ev.run_id, now);
        const run = { ...st.run!, intent };
        const t = taskOfRun(st, ev.run_id);
        if (t && !t.plan && t.steps.length === 0 && t.confirms.length === 0) {
          return { ...st, items: st.items.filter((i) => i.id !== t.id), run: { ...run, taskId: null, shared: [...run.shared, ...t.shared] } };
        }
        if (!t) return { ...st, run };
      }
      const st0 = ensureRun(s, ev.run_id, now);
      const [st, tid] = ensureTask({ ...st0, run: { ...st0.run!, intent } }, ev.run_id, now);
      const route: TaskRoute = { strategy: ev.strategy, provider: ev.provider, reason: ev.reason };
      return mapTask(st, tid, (t) => ({
        ...t,
        previous_routes: t.route ? [...t.previous_routes, t.route] : t.previous_routes,
        route,
        intent: intent ?? t.intent ?? null,
      }));
    }

    case "used": {
      const st = ensureRun(s, ev.run_id, now);
      const tag: UsedTag = { kind: ev.kind, label: ev.label, detail: ev.detail ?? null };
      const used = addUsed(st.run!.used, tag);
      return {
        ...st,
        run: { ...st.run!, used },
        items: st.items.map((it) =>
          (it.kind === "task" || it.kind === "assistant") && it.run_id === ev.run_id ? { ...it, used: addUsed(it.used, tag) } : it,
        ),
      };
    }

    case "context_shared": {
      const share: TaskShare = { provider: ev.provider, categories: ev.categories, bytes: ev.bytes };
      if (s.run?.run_id === ev.run_id && s.run.intent === "chat" && !taskOfRun(s, ev.run_id)) {
        // Chat answers keep their privacy ledger on the message itself.
        return {
          ...s,
          run: { ...s.run, shared: [...s.run.shared, share] },
          items: s.items.map((it) => (it.kind === "assistant" && it.run_id === ev.run_id ? { ...it, shared: [...(it.shared ?? []), share] } : it)),
        };
      }
      const [st, tid] = ensureTask(s, ev.run_id, now);
      return mapTask(st, tid, (t) => ({ ...t, shared: [...t.shared, share] }));
    }

    case "plan_proposed": {
      const [st, tid] = ensureTask(s, ev.run_id, now);
      return mapTask(st, tid, (t) => {
        const proposed = ev.steps.map((p): TaskStep => {
          const prev = t.steps.find((x) => x.id === p.id);
          return {
            ...pendingStep(p),
            status: prev && prev.status !== "pending" ? prev.status : "pending",
            summary: prev?.summary ?? "",
            started_at: prev?.started_at ?? null,
            finished_at: prev?.finished_at ?? null,
            attempts: prev?.attempts ?? 0,
          };
        });
        // Web tasks may have added steps before the (short) plan arrives: keep them.
        const extra = t.steps.filter((x) => !ev.steps.some((p) => p.id === x.id));
        return { ...t, plan: { source: ev.source, objective: ev.objective, notes: ev.notes }, steps: [...proposed, ...extra] };
      });
    }

    case "step_added": {
      const [st, tid] = ensureTask(s, ev.run_id, now);
      return mapTask(st, tid, (t) => {
        const known = t.steps.some((x) => x.id === ev.step.id);
        return {
          ...t,
          steps: known ? t.steps.map((x) => (x.id === ev.step.id ? { ...x, ...stepInfo(ev.step) } : x)) : [...t.steps, pendingStep(ev.step)],
        };
      });
    }

    case "step_started": {
      const [st, tid] = ensureTask(s, ev.run_id, now);
      return mapTask(st, tid, (t) => {
        const known = t.steps.some((x) => x.id === ev.step_id);
        const steps: TaskStep[] = known
          ? t.steps.map((x) =>
              x.id === ev.step_id
                ? { ...x, status: "running", summary: "", started_at: now, finished_at: null, attempts: x.attempts + 1 }
                : x,
            )
          : [
              ...t.steps,
              {
                id: ev.step_id,
                title: ev.step_id,
                tool: "",
                risk: "safe",
                status: "running",
                summary: "",
                started_at: now,
                finished_at: null,
                attempts: 1,
              },
            ];
        return { ...t, steps };
      });
    }

    case "step_finished": {
      const [st, tid] = ensureTask(s, ev.run_id, now);
      return mapTask(st, tid, (t) => ({
        ...t,
        steps: t.steps.map((x) =>
          x.id === ev.step_id
            ? { ...x, status: ev.status, summary: ev.summary, finished_at: now, started_at: x.started_at ?? now }
            : x,
        ),
      }));
    }

    case "assistant_delta": {
      if (!ev.text) return s;
      const st = ensureRun(s, ev.run_id, now);
      const run = st.run!;
      if (run.openTextId) {
        const id = run.openTextId;
        return {
          ...st,
          items: st.items.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, text: it.text + ev.text } : it)),
        };
      }
      const [id, seq] = nextId(st, now);
      const item: AssistantItem = {
        kind: "assistant",
        id,
        run_id: ev.run_id,
        text: ev.text,
        streaming: true,
        engine: run.engine,
        used: run.used,
        ...(run.shared.length ? { shared: run.shared } : {}),
      };
      return { ...st, seq, items: [...st.items, item], run: { ...run, openTextId: id } };
    }

    case "assistant_message": {
      const st = ensureRun(s, ev.run_id, now);
      const run = st.run!;
      // A web task that ends by asking the user to act (sign in, then say continue) is shown prominently.
      // The backend says so explicitly (`needs_user`); older events fall back to reading the text.
      const asks = ev.needs_user ?? (run.intent === "web" && asksUser(ev.text));
      const mark = (x: ChatState): ChatState => {
        if (!asks) return x;
        const t = taskOfRun(x, ev.run_id);
        return t ? mapTask(x, t.id, (tt) => ({ ...tt, awaiting_user: true })) : x;
      };
      if (run.openTextId) {
        const id = run.openTextId;
        return mark({
          ...st,
          run: { ...run, openTextId: null },
          items: st.items.map((it) =>
            it.id === id && it.kind === "assistant" ? { ...it, text: ev.text, streaming: false, ...(asks ? { asks_user: true } : {}) } : it,
          ),
        });
      }
      if (!ev.text.trim()) return st;
      const [id, seq] = nextId(st, now);
      const item: AssistantItem = {
        kind: "assistant",
        id,
        run_id: ev.run_id,
        text: ev.text,
        streaming: false,
        engine: run.engine,
        used: run.used,
        ...(run.shared.length ? { shared: run.shared } : {}),
        ...(asks ? { asks_user: true } : {}),
      };
      return mark({ ...st, seq, items: [...st.items, item] });
    }

    case "tool_started": {
      const [st, gid] = intoGroup(ensureRun(s, ev.run_id, now), ev.run_id, now);
      const step: ToolStep = {
        kind: "tool",
        id: ev.tool_id,
        name: ev.name,
        summary: ev.summary,
        result: null,
        status: "running",
        started_at: now,
        finished_at: null,
      };
      return mapGroup(st, gid, (g) => ({ ...g, steps: [...g.steps, step] }));
    }

    case "tool_finished": {
      const gid = s.run?.groupId;
      const finish = (g: ToolsItem): ToolsItem => ({
        ...g,
        steps: g.steps.map((st) =>
          st.kind === "tool" && st.id === ev.tool_id
            ? { ...st, status: ev.ok ? "ok" : "failed", result: ev.summary || null, finished_at: now }
            : st,
        ),
      });
      if (gid) return mapGroup(s, gid, finish);
      // Unknown group (e.g. a tool from an earlier run): search all groups.
      return {
        ...s,
        items: s.items.map((it) => (it.kind === "tools" && it.run_id === ev.run_id ? finish(it) : it)),
      };
    }

    case "confirm": {
      // Plan runs keep approvals in the task timeline, next to the step that asked.
      const task = taskOfRun(s, ev.run_id);
      if (task && task.plan) {
        const st = ensureRun(s, ev.run_id, now);
        const running = task.steps.find((x) => x.status === "running") ?? task.steps.find((x) => x.status === "pending");
        const c: TaskConfirm = {
          confirm_id: ev.id,
          title: ev.title,
          detail: ev.detail,
          risk: ev.risk,
          state: "pending",
          step_id: running?.id ?? null,
          at: now,
        };
        return mapTask(st, task.id, (t) => ({ ...t, confirms: [...t.confirms.filter((x) => x.confirm_id !== ev.id), c] }));
      }
      const [st, gid] = intoGroup(ensureRun(s, ev.run_id, now), ev.run_id, now);
      const step: ToolStep = {
        kind: "confirm",
        id: `confirm-${ev.id}`,
        confirm_id: ev.id,
        title: ev.title,
        detail: ev.detail,
        risk: ev.risk,
        state: "pending",
      };
      return mapGroup(st, gid, (g) => ({ ...g, steps: [...g.steps, step] }));
    }

    case "confirm_resolved": {
      let hit = false;
      const state = ev.approved ? ("approved" as const) : ("declined" as const);
      const items = s.items.map((it) => {
        if (it.kind === "task" && it.confirms.some((c) => c.confirm_id === ev.id)) {
          hit = true;
          return { ...it, confirms: it.confirms.map((c) => (c.confirm_id === ev.id ? { ...c, state } : c)) };
        }
        if (it.kind !== "tools") return it;
        if (!it.steps.some((st) => st.kind === "confirm" && st.confirm_id === ev.id)) return it;
        hit = true;
        return {
          ...it,
          steps: it.steps.map((st) => (st.kind === "confirm" && st.confirm_id === ev.id ? { ...st, state } : st)),
        };
      });
      return hit ? { ...s, items } : s;
    }

    case "notice": {
      const st = closeOpenText(s);
      const [id, seq] = nextId(st, now);
      return { ...st, seq, items: [...st.items, { kind: "notice", id, text: ev.text }] };
    }

    case "error": {
      const st = closeOpenText(s);
      const [id, seq] = nextId(st, now);
      const item: ErrorItem = { kind: "error", id, error: ev.kind, message: ev.message, run_id: ev.run_id };
      return { ...st, seq, items: [...st.items, item] };
    }

    case "run_finished":
      return finishRun(s, ev.run_id, ev.outcome, now);
  }
}

function finishRun(s: ChatState, runId: number, outcome: RunOutcome, now: number): ChatState {
  let st = closeOpenText(ensureRun(s, runId, now));
  const task = taskOfRun(st, runId);
  if (task) {
    st = mapTask(st, task.id, (t) => ({
      ...t,
      finished_at: now,
      outcome,
      steps: t.steps.map((x) => (x.status === "running" ? { ...x, status: "stopped" as const, finished_at: now } : x)),
      confirms: t.confirms.map((c) => (c.state === "pending" ? { ...c, state: "declined" as const } : c)),
    }));
  }
  const gid = st.run!.groupId;
  if (gid) {
    st = mapGroup(st, gid, (g) => ({
      ...g,
      finished_at: now,
      outcome,
      steps: g.steps.map((x) => {
        if (x.kind === "tool" && x.status === "running") return { ...x, status: "interrupted" as const, finished_at: now };
        if (x.kind === "confirm" && x.state === "pending") return { ...x, state: "declined" as const };
        return x;
      }),
    }));
    // If the run ended with no visible answer, surface its last narration as the answer.
    const gIdx = st.items.findIndex((i) => i.id === gid);
    const hasAnswer = st.items.some((i, idx) => idx > gIdx && i.kind === "assistant" && i.run_id === runId);
    const group = st.items[gIdx] as ToolsItem;
    const lastNote = [...group.steps].reverse().find((x) => x.kind === "note");
    if (!hasAnswer && lastNote && lastNote.kind === "note" && outcome === "completed") {
      const steps = group.steps.filter((x) => x !== lastNote);
      const answer: AssistantItem = {
        kind: "assistant",
        id: lastNote.id,
        run_id: runId,
        text: lastNote.text,
        streaming: false,
        engine: st.run!.engine,
        used: st.run!.used,
      };
      const items = [...st.items];
      items[gIdx] = { ...group, steps };
      items.splice(gIdx + 1, 0, answer);
      st = { ...st, items };
    }
  }
  if (outcome === "cancelled" && !task) {
    const [id, seq] = nextId(st, now);
    st = { ...st, seq, items: [...st.items, { kind: "notice", id, text: "Stopped." }] };
  }
  return { ...st, run: null };
}

const TASK_STATES: readonly TaskStepState[] = ["pending", "running", "verified", "done", "failed", "skipped", "stopped"];

/** Clean up items loaded from storage (a run may have been cut off by a restart). */
export function normalizeLoaded(raw: unknown[]): Item[] {
  const out: Item[] = [];
  for (const r of raw) {
    if (!r || typeof r !== "object") continue;
    const it = r as Item;
    switch (it.kind) {
      case "user":
        out.push({ ...it, attachments: Array.isArray(it.attachments) ? it.attachments : [] });
        break;
      case "assistant":
        out.push({ ...it, streaming: false, engine: it.engine ?? null });
        break;
      case "tools":
        out.push({
          ...it,
          finished_at: it.finished_at ?? it.started_at,
          steps: (it.steps ?? []).map((x) =>
            x.kind === "tool" && x.status === "running"
              ? { ...x, status: "interrupted" as const }
              : x.kind === "confirm" && x.state === "pending"
                ? { ...x, state: "declined" as const }
                : x,
          ),
        });
        break;
      case "task":
        out.push({
          ...it,
          previous_routes: Array.isArray(it.previous_routes) ? it.previous_routes : [],
          shared: Array.isArray(it.shared) ? it.shared : [],
          used: Array.isArray(it.used) ? it.used : [],
          confirms: (Array.isArray(it.confirms) ? it.confirms : []).map((c) =>
            c.state === "pending" ? { ...c, state: "declined" as const } : c,
          ),
          finished_at: it.finished_at ?? it.started_at,
          steps: (Array.isArray(it.steps) ? it.steps : []).map((x) => ({
            ...x,
            status: x.status === "running" ? "stopped" : TASK_STATES.includes(x.status) ? x.status : "done",
          })),
        });
        break;
      case "notice":
      case "error":
        out.push(it);
        break;
    }
  }
  return out;
}

/** Text the backend indexes for search. */
export function searchTextOf(items: Item[]): string {
  const parts: string[] = [];
  for (const it of items) {
    if (it.kind === "user" || it.kind === "assistant") {
      parts.push(it.text);
    } else if (it.kind === "tools") {
      for (const s of it.steps) if (s.kind === "note") parts.push(s.text);
    } else if (it.kind === "task" && it.plan) {
      parts.push(it.plan.objective);
    }
  }
  return parts.join("\n").slice(0, 50_000);
}

/** Title from the first user message: first non-empty line, ~50 chars, cut on a word boundary. */
export function titleFrom(text: string, max = 50): string {
  const line = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .find((l) => l.length > 0);
  if (!line) return "New chat";
  const clean = line.replace(/\s+/g, " ");
  if (clean.length <= max) return clean;
  const cut = clean.slice(0, max);
  const sp = cut.lastIndexOf(" ");
  return (sp > max * 0.6 ? cut.slice(0, sp) : cut).replace(/[\s,.;:–-]+$/, "") + "…";
}

export function toolSteps(g: ToolsItem): number {
  return g.steps.filter((s) => s.kind === "tool").length;
}

export function lastUser(items: Item[]): UserItem | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i]!;
    if (it.kind === "user") return it;
  }
  return null;
}

/** Items to keep when retrying: everything up to (excluding) the last user message. */
export function dropLastTurn(items: Item[]): Item[] {
  for (let i = items.length - 1; i >= 0; i--) if (items[i]!.kind === "user") return items.slice(0, i);
  return items;
}
