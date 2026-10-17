// Desktop E2E: launches the real Relay executable (built with `--features e2e`) in app mode and drives its real
// WebView2 window over the Chrome DevTools protocol. No mocks: the Rust backend, SQLite and the CLIs are real.
//
//   cargo tauri build --no-bundle -- --features e2e
//   pnpm test:desktop            (set RELAY_EXE to use another path)
//
// Side effects are limited to a test conversation and a settings round trip that is restored at the end.
// Results are written to ../../../reports/e2e/desktop.json.

import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const exe = process.env.RELAY_EXE ?? resolve(root, "target/release/relay-app.exe");
const port = 9444;
const reports = resolve(root, "reports/e2e");
mkdirSync(reports, { recursive: true });
mkdirSync(resolve(root, "reports/screenshots"), { recursive: true });

const results = [];
async function check(name, fn) {
  const t0 = Date.now();
  try {
    await fn();
    results.push({ name, ok: true, ms: Date.now() - t0 });
    console.log(`ok    ${name}`);
  } catch (e) {
    results.push({ name, ok: false, ms: Date.now() - t0, error: String(e?.message ?? e) });
    console.log(`FAIL  ${name}: ${e?.message ?? e}`);
  }
}
const assert = (cond, msg) => {
  if (!cond) throw new Error(msg);
};

const child = spawn(exe, ["--run"], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
});
const stop = () => {
  try {
    execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* already gone */
  }
};

let browser;
try {
  for (let i = 0; i < 60; i++) {
    try {
      browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  assert(browser, "the app never opened its debug port (was it built with --features e2e?)");
  const page = browser.contexts()[0].pages()[0];
  await page.waitForSelector("text=Good", { timeout: 20000 }).catch(() => {});
  const invoke = (cmd, args = {}) =>
    page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [cmd, args]);

  await check("app window opens in app mode with the Relay shell", async () => {
    assert((await page.title()) === "Relay", `title was ${await page.title()}`);
    const text = await page.evaluate(() => document.body.innerText);
    assert(/New chat/.test(text) && /Task history/.test(text), "sidebar missing");
  });

  await check("the served CSP is strict: self only, no unsafe script sources, no remote origins", async () => {
    // (Evaluating through DevTools bypasses the page CSP, so the header itself is what is checked.)
    const csp = await page.evaluate(async () => (await fetch(location.href)).headers.get("content-security-policy"));
    assert(csp, "no Content-Security-Policy header");
    const script = csp.split(";").map((d) => d.trim()).find((d) => d.startsWith("script-src")) ?? "";
    assert(!/unsafe-eval|unsafe-inline/.test(script), `script-src is loose: ${script.slice(0, 80)}`);
    assert(/default-src 'self'/.test(csp) && /object-src 'none'/.test(csp) && /frame-ancestors 'none'/.test(csp), "missing basics");
    assert(!/https?:\/\/(?!ipc\.localhost)/.test(csp), "a remote origin is allowed");
  });

  await check("get_platform and get_hybrid answer from the real backend", async () => {
    const p = await invoke("get_platform");
    assert(p.os === "windows", JSON.stringify(p));
    const h = await invoke("get_hybrid");
    assert(h.tools.length === 15 && h.providers.length === 2, "unexpected hybrid payload");
    assert(h.confirm_level === "destructive", `confirm level ${h.confirm_level}`);
  });

  await check("privacy mode round-trips through settings.json", async () => {
    const before = (await invoke("get_hybrid")).privacy_mode;
    const set = await invoke("set_hybrid", { privacyMode: "local_only" });
    assert(set.privacy_mode === "local_only", "not applied");
    const again = await invoke("get_hybrid");
    assert(again.privacy_mode === "local_only", "not persisted");
    await invoke("set_hybrid", { privacyMode: before });
    assert((await invoke("get_hybrid")).privacy_mode === before, "not restored");
  });

  await check("bad settings are rejected, not stored", async () => {
    const err = await invoke("set_hybrid", { privacyMode: "everything" }).then(
      () => null,
      (e) => String(e),
    );
    assert(err && /unknown privacy mode/.test(err), `got ${err}`);
  });

  await check("Local only with no local model refuses honestly and sends nothing to the cloud", async () => {
    const status = await invoke("get_engines").catch(() => null);
    const localReady = Boolean(status?.local?.available && status?.local?.active);
    if (localReady) return; // a local model is installed on this PC: the refusal path cannot be reached
    await invoke("set_hybrid", { privacyMode: "local_only" });
    try {
      const box = page.getByRole("textbox").first();
      await box.fill("Tidy up my documents folder");
      await box.press("Enter");
      await page.waitForSelector("text=/Local only|local AI|not ready/i", { timeout: 20000 });
      const text = await page.evaluate(() => document.body.innerText);
      assert(!/Shared with Claude/.test(text), "context was shared in Local only mode");
    } finally {
      await invoke("set_hybrid", { privacyMode: "hybrid_auto" });
    }
  });

  await page.screenshot({ path: resolve(root, "reports/screenshots/desktop-e2e-final.png") });
} finally {
  await browser?.close().catch(() => {});
  stop();
  const failed = results.filter((r) => !r.ok);
  writeFileSync(resolve(reports, "desktop.json"), JSON.stringify({ exe, at: new Date().toISOString(), results }, null, 2));
  console.log(`\n${results.length - failed.length}/${results.length} passed`);
  process.exitCode = failed.length ? 1 : 0;
}
