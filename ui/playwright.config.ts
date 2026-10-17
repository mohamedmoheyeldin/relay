// Browser-level tests against `pnpm dev` with the mock backend (no Tauri, no Rust).
//   pnpm test:e2e-browser   keyboard and flow specs (tests/e2e)
//   pnpm test:visual        visual regression (tests/visual), baselines in tests/visual/__screenshots__
// The dev server is started for the run and stopped afterwards.
import { defineConfig, devices } from "@playwright/test";

const PORT = 1420;

export default defineConfig({
  testDir: "./tests",
  timeout: 45_000,
  expect: { timeout: 8_000, toHaveScreenshot: { maxDiffPixelRatio: 0.004, animations: "disabled", caret: "hide" } },
  fullyParallel: true,
  workers: process.env.CI ? 2 : 4,
  reporter: [["list"]],
  snapshotPathTemplate: "{testDir}/__screenshots__/{arg}{ext}",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    ...devices["Desktop Chrome"],
    viewport: { width: 1100, height: 740 },
    deviceScaleFactor: 1,
    locale: "en-US",
    timezoneId: "America/New_York",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "e2e", testDir: "./tests/e2e" },
    { name: "visual", testDir: "./tests/visual" },
    { name: "shots", testDir: "./tests/shots" },
  ],
  webServer: {
    command: "pnpm exec vite --port 1420 --strictPort",
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
