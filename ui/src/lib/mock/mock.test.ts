// The mock backend must speak the contract exactly: every event it emits passes the Zod schema,
// every command result passes its schema, and each scenario reaches a final run_finished.
import { describe, expect, it } from "vitest";
import { createMockTransport } from "./index";
import { readScenario, type Flow } from "./scenario";
import { UiEventSchema, HybridSchema, WorkflowSchema, type UiEvent } from "../schemas";
import { z } from "zod/mini";
import { WorkflowSummarySchema } from "../schemas";

async function runFlow(flow: Flow, flags = "fast", answer?: (ev: UiEvent) => boolean | undefined) {
  const t = createMockTransport(readScenario(`?scenario=${flags}&flow=${flow}`));
  const events: UiEvent[] = [];
  const done = new Promise<void>((resolve) => {
    void t.listen((raw) => {
      const r = UiEventSchema.safeParse(raw);
      if (!r.success) throw new Error(`invalid event ${JSON.stringify(raw)}: ${r.error.message}`);
      events.push(r.data);
      if (r.data.type === "confirm" && answer) {
        const approved = answer(r.data);
        if (approved !== undefined) void t.invoke("answer_confirm", { id: r.data.id, approved });
      }
      if (r.data.type === "run_finished") resolve();
    });
  });
  const conv = (await t.invoke("conv_create", {})) as { id: string };
  await t.invoke("send_message", { convId: conv.id, text: "do the thing please" });
  await done;
  return { t, events, types: events.map((e) => e.type) };
}

const finished = (events: UiEvent[]) => events.find((e): e is Extract<UiEvent, { type: "run_finished" }> => e.type === "run_finished")!;
const steps = (events: UiEvent[]) => events.filter((e): e is Extract<UiEvent, { type: "step_finished" }> => e.type === "step_finished").map((e) => e.status);

describe("mock backend flows", () => {
  it("cloud plan: route, ledger, plan, verified steps, answer", async () => {
    const { events, types } = await runFlow("cloud");
    expect(types.slice(0, 4)).toEqual(["status", "run_started", "route", "context_shared"]);
    expect(types).toContain("plan_proposed");
    expect(steps(events)).toEqual(["verified", "verified", "verified"]);
    expect(finished(events).outcome).toBe("completed");
  });

  it("native and local plans share nothing", async () => {
    for (const flow of ["native", "local"] as Flow[]) {
      const { types } = await runFlow(flow);
      expect(types).not.toContain("context_shared");
      expect(types).toContain("plan_proposed");
    }
  });

  it("verification failure fails the run and skips the rest", async () => {
    const { events } = await runFlow("verify-fail");
    expect(steps(events)).toEqual(["verified", "failed", "skipped"]);
    expect(finished(events).outcome).toBe("failed");
  });

  it("a refused step reports why", async () => {
    const { events } = await runFlow("refused");
    const refused = events.find((e) => e.type === "step_finished" && e.status === "failed");
    expect(refused && "summary" in refused && refused.summary).toMatch(/^Refused:/);
  });

  it("declining a confirmation skips the step; approving runs it", async () => {
    const declined = await runFlow("declined", "fast", () => false);
    expect(declined.types).toContain("confirm_resolved");
    expect(steps(declined.events)).toEqual(["verified", "skipped"]);
    const approved = await runFlow("declined", "fast", () => true);
    expect(steps(approved.events)).toEqual(["verified", "verified"]);
    expect(finished(approved.events).outcome).toBe("completed");
  });

  it("cloud failure falls back to Relay Local AI", async () => {
    const { events } = await runFlow("cloud-fail");
    const routes = events.filter((e): e is Extract<UiEvent, { type: "route" }> => e.type === "route").map((r) => r.strategy);
    expect(routes).toEqual(["cloud_plan", "local_plan"]);
    expect(events.some((e) => e.type === "notice")).toBe(true);
    expect(finished(events).outcome).toBe("completed");
  });

  it("agent mode streams tools and text", async () => {
    const { types } = await runFlow("agent");
    expect(types).toContain("tool_started");
    expect(types).toContain("assistant_delta");
  });

  it("provider not installed / not signed in produce the matching errors", async () => {
    const a = await runFlow("not-installed");
    expect(a.events.find((e) => e.type === "error")).toMatchObject({ kind: "cli_not_installed" });
    const b = await runFlow("not-signed-in");
    expect(b.events.find((e) => e.type === "error")).toMatchObject({ kind: "not_logged_in" });
  });

  it("cloud-preferred with an unready provider fails instead of silently using the cloud", async () => {
    const { events } = await runFlow("cloud", "fast,cloud-preferred,claude-signed-out");
    expect(events.find((e) => e.type === "route")).toMatchObject({ strategy: "unavailable" });
    expect(finished(events).outcome).toBe("failed");
  });

  it("local only never shares context", async () => {
    const { types, events } = await runFlow("cloud", "fast,local-only");
    expect(types).not.toContain("context_shared");
    expect(events.find((e) => e.type === "route")).toMatchObject({ strategy: "local_plan" });
  });

  it("stop cancels a running task", async () => {
    const t = createMockTransport(readScenario("?scenario=fast&flow=cloud"));
    const events: UiEvent[] = [];
    const done = new Promise<void>((resolve) =>
      void t.listen((raw) => {
        const ev = UiEventSchema.parse(raw);
        events.push(ev);
        if (ev.type === "step_started") void t.invoke("stop_task");
        if (ev.type === "run_finished") resolve();
      }),
    );
    const conv = (await t.invoke("conv_create", {})) as { id: string };
    await t.invoke("send_message", { convId: conv.id, text: "long task" });
    await done;
    expect(finished(events).outcome).toBe("cancelled");
  });

  it("commands return schema-valid data and record task history", async () => {
    const { t } = await runFlow("cloud");
    expect(HybridSchema.safeParse(await t.invoke("get_hybrid")).success).toBe(true);
    const list = z.array(WorkflowSummarySchema).parse(await t.invoke("workflow_list", { limit: 5 }));
    expect(list[0]!.objective).toBe("do the thing please");
    expect(WorkflowSchema.safeParse(await t.invoke("workflow_get", { id: list[0]!.id })).success).toBe(true);
    const h = HybridSchema.parse(await t.invoke("set_hybrid", { privacyMode: "local_only", share: { screenshots: true } }));
    expect(h.privacy_mode).toBe("local_only");
    expect(h.share.screenshots).toBe(true);
  });
});
