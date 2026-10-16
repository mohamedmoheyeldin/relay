import { describe, expect, it } from "vitest";
import {
  addUser,
  applyEvent,
  dropLastTurn,
  emptyChat,
  normalizeLoaded,
  searchTextOf,
  titleFrom,
  type ChatEvent,
  type ChatState,
  type ToolsItem,
} from "./chat";

function run(events: ChatEvent[], start: ChatState = emptyChat(), t0 = 1000): ChatState {
  let s = start;
  events.forEach((e, i) => (s = applyEvent(s, e, t0 + i * 1000)));
  return s;
}

describe("chat reducer", () => {
  it("streams deltas into one assistant message and closes it on run_finished", () => {
    let s = addUser(emptyChat(), "hi", [], 1);
    s = run(
      [
        { type: "run_started", run_id: 1, conv_id: "c" },
        { type: "assistant_delta", run_id: 1, text: "Hel" },
        { type: "assistant_delta", run_id: 1, text: "lo" },
      ],
      s,
    );
    expect(s.items).toHaveLength(2);
    expect(s.items[1]).toMatchObject({ kind: "assistant", text: "Hello", streaming: true });
    s = applyEvent(s, { type: "run_finished", run_id: 1, outcome: "completed" }, 9000);
    expect(s.items[1]).toMatchObject({ text: "Hello", streaming: false });
    expect(s.run).toBeNull();
  });

  it("assistant_message replaces the open streamed text", () => {
    const s = run([
      { type: "run_started", run_id: 1, conv_id: "c" },
      { type: "assistant_delta", run_id: 1, text: "Draf" },
      { type: "assistant_message", run_id: 1, text: "Final text" },
    ]);
    expect(s.items).toHaveLength(1);
    expect(s.items[0]).toMatchObject({ kind: "assistant", text: "Final text", streaming: false });
    expect(s.run?.openTextId).toBeNull();
  });

  it("groups all tool steps of a run into one group and folds narration into it", () => {
    const s = run([
      { type: "run_started", run_id: 7, conv_id: "c" },
      { type: "assistant_delta", run_id: 7, text: "Let me look." },
      { type: "tool_started", run_id: 7, tool_id: "a", name: "screenshot", summary: "Take a screenshot" },
      { type: "tool_finished", run_id: 7, tool_id: "a", ok: true, summary: "1568x882" },
      { type: "assistant_delta", run_id: 7, text: "Now clicking." },
      { type: "tool_started", run_id: 7, tool_id: "b", name: "left_click", summary: "Click OK" },
      { type: "tool_finished", run_id: 7, tool_id: "b", ok: false, summary: "missed" },
      { type: "assistant_delta", run_id: 7, text: "All done." },
      { type: "run_finished", run_id: 7, outcome: "completed" },
    ]);
    expect(s.items.map((i) => i.kind)).toEqual(["tools", "assistant"]);
    const g = s.items[0] as ToolsItem;
    expect(g.steps.map((x) => x.kind)).toEqual(["note", "tool", "note", "tool"]);
    expect(g.steps[1]).toMatchObject({ status: "ok", result: "1568x882" });
    expect(g.steps[3]).toMatchObject({ status: "failed" });
    expect(g.started_at).toBe(1000);
    expect(g.finished_at).toBe(9000);
    expect(g.outcome).toBe("completed");
    expect(s.items[1]).toMatchObject({ kind: "assistant", text: "All done.", streaming: false });
  });

  it("promotes the last narration when a run ends without a visible answer", () => {
    const s = run([
      { type: "run_started", run_id: 2, conv_id: "c" },
      { type: "assistant_message", run_id: 2, text: "Opening Notepad." },
      { type: "tool_started", run_id: 2, tool_id: "x", name: "open", summary: "Open Notepad" },
      { type: "tool_finished", run_id: 2, tool_id: "x", ok: true, summary: "ok" },
      { type: "run_finished", run_id: 2, outcome: "completed" },
    ]);
    expect(s.items.map((i) => i.kind)).toEqual(["tools", "assistant"]);
    expect((s.items[0] as ToolsItem).steps.map((x) => x.kind)).toEqual(["tool"]);
    expect(s.items[1]).toMatchObject({ text: "Opening Notepad." });
  });

  it("marks running steps interrupted and adds a notice when cancelled", () => {
    const s = run([
      { type: "run_started", run_id: 3, conv_id: "c" },
      { type: "tool_started", run_id: 3, tool_id: "t", name: "wait", summary: "Wait" },
      { type: "run_finished", run_id: 3, outcome: "cancelled" },
    ]);
    const g = s.items[0] as ToolsItem;
    expect(g.steps[0]).toMatchObject({ status: "interrupted" });
    expect(g.outcome).toBe("cancelled");
    expect(s.items.at(-1)).toMatchObject({ kind: "notice", text: "Stopped." });
  });

  it("records confirm steps and resolves them by id", () => {
    let s = run([
      { type: "run_started", run_id: 4, conv_id: "c" },
      { type: "confirm", id: 11, run_id: 4, title: "Delete files?", detail: "3 files", risk: "destructive" },
    ]);
    let g = s.items[0] as ToolsItem;
    expect(g.steps[0]).toMatchObject({ kind: "confirm", confirm_id: 11, state: "pending" });
    s = applyEvent(s, { type: "confirm_resolved", id: 11, approved: true }, 5000);
    g = s.items[0] as ToolsItem;
    expect(g.steps[0]).toMatchObject({ state: "approved" });
    // Unknown confirm ids leave the state untouched (same object).
    expect(applyEvent(s, { type: "confirm_resolved", id: 99, approved: false }, 6000)).toBe(s);
  });

  it("errors and notices close the open text and append items", () => {
    const s = run([
      { type: "run_started", run_id: 5, conv_id: "c" },
      { type: "assistant_delta", run_id: 5, text: "Trying" },
      { type: "notice", text: "Handed over to Claude." },
      { type: "assistant_delta", run_id: 5, text: "Again" },
      { type: "error", run_id: 5, kind: "not_logged_in", message: "nope" },
      { type: "run_finished", run_id: 5, outcome: "failed" },
    ]);
    expect(s.items.map((i) => i.kind)).toEqual(["assistant", "notice", "assistant", "error"]);
    expect(s.items[0]).toMatchObject({ text: "Trying", streaming: false });
    expect(s.items[2]).toMatchObject({ text: "Again", streaming: false });
    expect(s.items[3]).toMatchObject({ error: "not_logged_in", message: "nope" });
  });

  it("tags assistant text with the engine announced for the run", () => {
    const s = run([
      { type: "run_started", run_id: 6, conv_id: "c" },
      { type: "engine", run_id: 6, kind: "local", name: "qwen3:14b" },
      { type: "assistant_delta", run_id: 6, text: "Hi" },
    ]);
    expect(s.items[0]).toMatchObject({ engine: { kind: "local", name: "qwen3:14b" } });
  });

  it("keeps unchanged items referentially equal while streaming (memo friendly)", () => {
    let s = addUser(emptyChat(), "q", [], 1);
    s = run([{ type: "run_started", run_id: 1, conv_id: "c" }, { type: "assistant_delta", run_id: 1, text: "a" }], s);
    const user = s.items[0];
    const next = applyEvent(s, { type: "assistant_delta", run_id: 1, text: "b" }, 5000);
    expect(next.items[0]).toBe(user);
    expect(next.items[1]).not.toBe(s.items[1]);
  });

  it("generates unique ids", () => {
    const s = run([
      { type: "notice", text: "a" },
      { type: "notice", text: "b" },
      { type: "notice", text: "c" },
    ]);
    expect(new Set(s.items.map((i) => i.id)).size).toBe(3);
  });
});

describe("helpers", () => {
  it("titleFrom takes the first line, trims and cuts on a word", () => {
    expect(titleFrom("\n  Clean up my   Downloads folder\nplease")).toBe("Clean up my Downloads folder");
    expect(titleFrom("")).toBe("New chat");
    const long = titleFrom("Find every large video file on my computer and move them to the external drive");
    expect(long.length).toBeLessThanOrEqual(51);
    expect(long.endsWith("…")).toBe(true);
    expect(long).toBe("Find every large video file on my computer and…");
  });

  it("searchTextOf includes user, assistant and narration text", () => {
    let s = addUser(emptyChat(), "hello", [], 1);
    s = run(
      [
        { type: "run_started", run_id: 1, conv_id: "c" },
        { type: "assistant_delta", run_id: 1, text: "note" },
        { type: "tool_started", run_id: 1, tool_id: "t", name: "x", summary: "y" },
        { type: "assistant_message", run_id: 1, text: "answer" },
      ],
      s,
    );
    expect(searchTextOf(s.items)).toBe("hello\nnote\nanswer");
  });

  it("normalizeLoaded repairs runs cut off by a restart and drops junk", () => {
    const items = normalizeLoaded([
      { kind: "user", id: "1", text: "x", at: 1 },
      { kind: "assistant", id: "2", run_id: 1, text: "y", streaming: true },
      { kind: "tools", id: "3", run_id: 1, started_at: 5, finished_at: null, outcome: null, steps: [{ kind: "tool", id: "t", name: "n", summary: "s", result: null, status: "running", started_at: 5, finished_at: null }] },
      null,
      42,
      { kind: "mystery" },
    ]);
    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({ attachments: [] });
    expect(items[1]).toMatchObject({ streaming: false, engine: null });
    expect(items[2]).toMatchObject({ finished_at: 5, steps: [{ status: "interrupted" }] });
  });

  it("dropLastTurn removes the last user message and everything after it", () => {
    let s = addUser(emptyChat(), "one", [], 1);
    s = applyEvent(s, { type: "assistant_message", run_id: 1, text: "a1" }, 2);
    s = addUser(s, "two", [], 3);
    s = applyEvent(s, { type: "error", run_id: 2, kind: "timeout", message: "" }, 4);
    expect(dropLastTurn(s.items).map((i) => i.kind)).toEqual(["user", "assistant"]);
  });
});
