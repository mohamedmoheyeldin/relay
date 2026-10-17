// @vitest-environment jsdom
import { beforeAll, describe, expect, it, vi } from "vitest";
import { setTransport, type Transport } from "@/lib/backend";
import { queryClient, qk } from "@/lib/query";
import type { StatusPayload, UiEvent } from "@/lib/types";
import { useApp } from "./app";

let emit: (ev: unknown) => void = () => undefined;
const invoke = vi.fn(async (cmd: string, _args?: unknown) => (cmd === "conv_save" ? undefined : null));
const fake: Transport = {
  kind: "mock",
  invoke: (cmd, args) => invoke(cmd, args),
  async listen(h) {
    emit = h;
    return () => undefined;
  },
};

const flush = () => new Promise((r) => setTimeout(r, 160));
async function send(...events: UiEvent[]) {
  for (const e of events) emit(e);
  await flush();
}

describe("app store event pipeline", () => {
  beforeAll(async () => {
    setTransport(fake);
    useApp.getState().init();
    await flush();
  });

  it("routes run events to the conversation and builds the timeline", async () => {
    await send(
      { type: "run_started", run_id: 41, conv_id: "c-1" },
      { type: "route", run_id: 41, strategy: "native", provider: null, reason: "direct" },
      { type: "plan_proposed", run_id: 41, source: "native", objective: "o", notes: null, steps: [{ id: "a", title: "A", tool: "t", risk: "safe" }] },
      { type: "step_started", run_id: 41, step_id: "a" },
    );
    const s = useApp.getState();
    expect(s.activeRun).toMatchObject({ run_id: 41, conv_id: "c-1" });
    const task = s.chats["c-1"]!.items.find((i) => i.kind === "task");
    expect(task).toMatchObject({ kind: "task", steps: [{ status: "running" }] });
    expect(s.live).toMatch(/Plan proposed/);
  });

  it("queues approvals globally and removes them when resolved", async () => {
    await send({ type: "confirm", id: 7, run_id: 41, title: "Delete?", detail: "", risk: "destructive" });
    expect(useApp.getState().confirms.map((c) => c.id)).toEqual([7]);
    expect(useApp.getState().live).toMatch(/needs your approval/);
    await send({ type: "confirm_resolved", id: 7, approved: true });
    expect(useApp.getState().confirms).toEqual([]);
    const task = useApp.getState().chats["c-1"]!.items.find((i) => i.kind === "task");
    expect(task && task.kind === "task" && task.confirms[0]!.state).toBe("approved");
  });

  it("status events land in the query cache", async () => {
    const st: StatusPayload = { provider: "Claude Code", ready: true, detail: "ready", busy: true, kill_hotkey: "k", local_model: null, route_mode: "hybrid_auto" };
    await send({ type: "status", ...st });
    expect(queryClient.getQueryData(qk.status)).toEqual(st);
  });

  it("run_finished clears the run, saves the chat and refreshes task history", async () => {
    const inv = vi.spyOn(queryClient, "invalidateQueries");
    await send(
      { type: "step_finished", run_id: 41, step_id: "a", status: "verified", summary: "ok" },
      { type: "assistant_message", run_id: 41, text: "Done." },
      { type: "run_finished", run_id: 41, outcome: "completed" },
    );
    const s = useApp.getState();
    expect(s.activeRun).toBeNull();
    expect(s.live).toBe("Relay: Done.");
    expect(inv).toHaveBeenCalledWith({ queryKey: qk.workflowsAll });
    expect(invoke).toHaveBeenCalledWith("conv_save", expect.objectContaining({ id: "c-1" }));
  });

  it("errors without a run become toasts when no chat is open", async () => {
    await send({ type: "error", run_id: null, kind: "internal", message: "backend hiccup" });
    expect(useApp.getState().toasts.at(-1)).toMatchObject({ text: "backend hiccup", tone: "error" });
  });

  it("drops malformed events instead of crashing", async () => {
    const before = useApp.getState().chats;
    const err = vi.spyOn(console, "error").mockImplementation(() => undefined);
    emit({ type: "step_finished", run_id: 41 });
    await flush();
    expect(useApp.getState().chats).toBe(before);
    err.mockRestore();
  });
});
