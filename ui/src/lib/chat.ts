// Pure reducer: turns relay events into the items a conversation renders (and persists via conv_save).
// No React, no I/O: unit tested in chat.test.ts.
import type { ErrorKind, Risk, RunOutcome, UiEvent } from "./types";

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

export type UserItem = { kind: "user"; id: string; text: string; attachments: string[]; at: number };
export type AssistantItem = {
  kind: "assistant";
  id: string;
  run_id: number | null;
  text: string;
  streaming: boolean;
  engine: EngineTag | null;
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

export type Item = UserItem | AssistantItem | ToolsItem | NoticeItem | ErrorItem;

export type RunCursor = {
  run_id: number;
  started_at: number;
  openTextId: string | null;
  groupId: string | null;
  engine: EngineTag | null;
};

export type ChatState = { items: Item[]; run: RunCursor | null; seq: number };

export const emptyChat = (): ChatState => ({ items: [], run: null, seq: 0 });

/** Events the per-conversation reducer understands (global ones are handled by the store). */
export type ChatEvent = Extract<
  UiEvent,
  {
    type:
      | "run_started"
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

function ensureRun(s: ChatState, runId: number, now: number): ChatState {
  if (s.run && s.run.run_id === runId) return s;
  return { ...s, run: { run_id: runId, started_at: now, openTextId: null, groupId: null, engine: null } };
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

export function applyEvent(s: ChatState, ev: ChatEvent, now: number): ChatState {
  switch (ev.type) {
    case "run_started":
      return { ...s, run: { run_id: ev.run_id, started_at: now, openTextId: null, groupId: null, engine: null } };

    case "engine": {
      const st = ensureRun(s, ev.run_id, now);
      return { ...st, run: { ...st.run!, engine: { kind: ev.kind, name: ev.name } } };
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
      const item: AssistantItem = { kind: "assistant", id, run_id: ev.run_id, text: ev.text, streaming: true, engine: run.engine };
      return { ...st, seq, items: [...st.items, item], run: { ...run, openTextId: id } };
    }

    case "assistant_message": {
      const st = ensureRun(s, ev.run_id, now);
      const run = st.run!;
      if (run.openTextId) {
        const id = run.openTextId;
        return {
          ...st,
          run: { ...run, openTextId: null },
          items: st.items.map((it) =>
            it.id === id && it.kind === "assistant" ? { ...it, text: ev.text, streaming: false } : it,
          ),
        };
      }
      if (!ev.text.trim()) return st;
      const [id, seq] = nextId(st, now);
      const item: AssistantItem = { kind: "assistant", id, run_id: ev.run_id, text: ev.text, streaming: false, engine: run.engine };
      return { ...st, seq, items: [...st.items, item] };
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
      const items = s.items.map((it) => {
        if (it.kind !== "tools") return it;
        if (!it.steps.some((st) => st.kind === "confirm" && st.confirm_id === ev.id)) return it;
        hit = true;
        return {
          ...it,
          steps: it.steps.map((st) =>
            st.kind === "confirm" && st.confirm_id === ev.id
              ? { ...st, state: ev.approved ? ("approved" as const) : ("declined" as const) }
              : st,
          ),
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
      };
      const items = [...st.items];
      items[gIdx] = { ...group, steps };
      items.splice(gIdx + 1, 0, answer);
      st = { ...st, items };
    }
  }
  if (outcome === "cancelled") {
    const [id, seq] = nextId(st, now);
    st = { ...st, seq, items: [...st.items, { kind: "notice", id, text: "Stopped." }] };
  }
  return { ...st, run: null };
}

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
    if (it.kind === "user" || it.kind === "assistant") parts.push(it.text);
    else if (it.kind === "tools") for (const s of it.steps) if (s.kind === "note") parts.push(s.text);
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
