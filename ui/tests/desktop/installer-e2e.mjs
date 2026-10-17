// Installer E2E: runs the real Setup (an `e2e` build copied anywhere outside Program Files), drives its window
// over CDP, clicks Install, waits for the elevated worker (Windows shows ONE UAC prompt that a person must accept),
// then checks what landed on disk and in the registry. Results: reports/installer/installer.json
//   node tests/desktop/installer-e2e.mjs "C:\path\to\Relay Setup.exe"
import { spawn, execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "@playwright/test";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const setup = process.argv[2];
const port = 9455;
const out = resolve(root, "reports/installer");
mkdirSync(out, { recursive: true });
const results = [];
const check = async (name, fn) => {
  try {
    await fn();
    results.push({ name, ok: true });
    console.log(`ok    ${name}`);
  } catch (e) {
    results.push({ name, ok: false, error: String(e?.message ?? e) });
    console.log(`FAIL  ${name}: ${e?.message ?? e}`);
  }
};
const assert = (c, m) => {
  if (!c) throw new Error(m);
};

const child = spawn(setup, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: "ignore",
});
let browser;
try {
  for (let i = 0; i < 60 && !browser; i++) {
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`).catch(() => null);
    if (!browser) await new Promise((r) => setTimeout(r, 500));
  }
  assert(browser, "Setup never opened (built with --features e2e?)");
  const page = browser.contexts()[0].pages()[0];
  await page.waitForTimeout(1500);

  await check("Setup window shows the install screen", async () => {
    const text = await page.evaluate(() => document.body.innerText);
    assert(/Install|Update/.test(text) && /Program Files/.test(text), text.slice(0, 200));
  });
  await page.screenshot({ path: resolve(out, "setup-before.png") });

  await check("install completes (accept the Windows prompt)", async () => {
    const button = page.getByRole("button", { name: /^(Install|Update)/ }).first();
    await button.click();
    console.log("      waiting for the Windows permission prompt to be accepted…");
    await page.waitForFunction(() => /installed|Open Relay|Launch|Start Relay/i.test(document.body.innerText), null, {
      timeout: 300_000,
    });
  });
  await page.screenshot({ path: resolve(out, "setup-after.png") });

  const dir = "C:\\Program Files\\Relay";
  await check("Relay.exe is in Program Files", async () => assert(existsSync(`${dir}\\Relay.exe`), "missing"));
  await check("uninstall entry is registered", async () => {
    const outp = execFileSync("reg", ["query", "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Relay"], {
      encoding: "utf8",
    });
    assert(/DisplayName/.test(outp) && /Relay/.test(outp), outp);
  });
  await check("Start menu shortcut exists", async () => {
    const lnk = "C:\\ProgramData\\Microsoft\\Windows\\Start Menu\\Programs\\Relay.lnk";
    assert(existsSync(lnk), lnk);
  });
  await check("installed copy reports the built version", async () => {
    const v = execFileSync(`${dir}\\Relay.exe`, ["--mcp", "--version"], { encoding: "utf8" });
    assert(/relay-mcp \d+\.\d+\.\d+/.test(v), v);
  });
} finally {
  await browser?.close().catch(() => {});
  try {
    execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {
    /* gone */
  }
  writeFileSync(resolve(out, "installer.json"), JSON.stringify({ setup, at: new Date().toISOString(), results }, null, 2));
  const failed = results.filter((r) => !r.ok).length;
  console.log(`\n${results.length - failed}/${results.length} passed`);
  process.exitCode = failed ? 1 : 0;
}
