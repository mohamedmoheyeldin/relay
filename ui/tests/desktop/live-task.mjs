// Live check against the running app (started with the e2e build and a debug port): sends one message
// through the real composer, records every backend event, and reports timing and what was used.
//   node tests/desktop/live-task.mjs "<message>" [port] [screenshot.png]
import { chromium } from "@playwright/test";

const [, , message, port = "9333", shot] = process.argv;
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
const page = browser.contexts()[0].pages()[0];
await page.evaluate(async () => {
  window.__ev = [];
  const id = window.__TAURI_INTERNALS__.transformCallback((e) => {
    const p = typeof e.payload === "string" ? JSON.parse(e.payload) : e.payload;
    window.__ev.push({ t: Date.now(), ...p });
  });
  await window.__TAURI_INTERNALS__.invoke("plugin:event|listen", { event: "relay-event", target: { kind: "Any" }, handler: id });
});
await page.getByRole("link", { name: /new chat/i }).first().click().catch(() => {});
const box = page.getByRole("textbox").first();
await box.fill(message);
const t0 = Date.now();
await box.press("Enter");
let events = [];
for (let i = 0; i < 600; i++) {
  await page.waitForTimeout(500);
  events = await page.evaluate(() => window.__ev);
  const confirm = events.find((e) => e.type === "confirm" && !events.some((r) => r.type === "confirm_resolved" && r.id === e.id));
  if (confirm) {
    console.log(`CONFIRM asked: ${confirm.title} — declining (live test never submits)`);
    await page.evaluate((id) => window.__TAURI_INTERNALS__.invoke("answer_confirm", { id, approved: false }), confirm.id);
  }
  if (events.some((e) => e.type === "run_finished")) break;
}
const first = (type) => events.find((e) => e.type === type);
const route = first("route");
console.log(`route: ${route?.intent} / ${route?.strategy} — ${route?.reason}`);
console.log("used:", events.filter((e) => e.type === "used").map((e) => `${e.kind}:${e.label}`).join(", "));
const firstDelta = first("assistant_delta");
if (firstDelta) console.log(`first text after ${((firstDelta.t - t0) / 1000).toFixed(1)} s`);
for (const e of events.filter((e) => e.type === "step_added" || e.type === "step_finished")) {
  console.log(e.type === "step_added" ? `  + ${e.step.title} [${e.step.tool}${e.step.sensitive_data ? ", sensitive" : ""}]` : `    ${e.status}: ${e.summary.slice(0, 120)}`);
}
const msg = events.filter((e) => e.type === "assistant_message").pop();
console.log(`finished in ${((first("run_finished")?.t ?? Date.now()) - t0) / 1000} s: ${first("run_finished")?.outcome}`);
console.log("needs_user:", Boolean(msg?.needs_user));
console.log("---- message ----\n" + (msg?.text ?? "(none)").slice(0, 1500));
await page.waitForTimeout(800);
if (shot) await page.screenshot({ path: shot });
await browser.close();
