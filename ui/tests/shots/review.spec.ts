// Review screenshots for design iteration (not assertions). Saved to ui/.shots (gitignored).
//   pnpm exec playwright test --project=shots
import { test } from "@playwright/test";
import { open, sendMessage, settle, type Theme } from "../helpers";

const SIZES = [
  { name: "wide", width: 1100, height: 740 },
  { name: "narrow", width: 520, height: 740 },
];
const dir = ".shots";

for (const theme of ["light", "dark"] as Theme[]) {
  for (const size of SIZES) {
    test(`home ${theme} ${size.name}`, async ({ page }) => {
      await page.setViewportSize(size);
      await open(page, { theme });
      await settle(page);
      await page.screenshot({ path: `${dir}/home-${theme}-${size.name}.png` });
    });

    test(`seeded task ${theme} ${size.name}`, async ({ page }) => {
      await page.setViewportSize(size);
      await open(page, { theme });
      await settle(page);
      if (size.name === "narrow") {
        await page.getByRole("button", { name: "Open sidebar" }).click();
      }
      await page.getByRole("button", { name: "Tidy up my Downloads folder" }).first().click();
      await page.getByTestId("task-timeline").first().waitFor();
      await page.locator("[data-md=rendered]").first().waitFor();
      await settle(page);
      await page.screenshot({ path: `${dir}/chat-task-${theme}-${size.name}.png` });
    });

    test(`tasks ${theme} ${size.name}`, async ({ page }) => {
      await page.setViewportSize(size);
      await open(page, { theme, route: "/tasks/wf-1" });
      await settle(page);
      await page.screenshot({ path: `${dir}/tasks-${theme}-${size.name}.png` });
    });

    test(`settings privacy ${theme} ${size.name}`, async ({ page }) => {
      await page.setViewportSize(size);
      await open(page, { theme, route: "/settings/privacy", scenario: "codex-missing" });
      await settle(page);
      await page.screenshot({ path: `${dir}/settings-privacy-${theme}-${size.name}.png`, fullPage: false });
    });
  }

  test(`live run ${theme}`, async ({ page }) => {
    await open(page, { theme, flow: "download" });
    await settle(page);
    await sendMessage(page, "Tidy up my Downloads folder");
    await page.getByTestId("permission-dialog").waitFor();
    await page.screenshot({ path: `${dir}/permission-${theme}.png` });
    await page.getByRole("button", { name: "Approve" }).click();
    await page.locator('[data-testid="task-timeline"]:not([data-verdict="running"])').waitFor({ timeout: 20_000 });
    await page.waitForTimeout(400);
    await page.screenshot({ path: `${dir}/live-verified-${theme}.png` });
  });

  test(`failed run ${theme}`, async ({ page }) => {
    await open(page, { theme, flow: "verify-fail" });
    await settle(page);
    await sendMessage(page, "Set brightness to 60%");
    await page.locator('[data-testid="task-timeline"][data-verdict="failed"]').waitFor({ timeout: 20_000 });
    await page.waitForTimeout(400);
    await page.screenshot({ path: `${dir}/failed-${theme}.png` });
  });

  test(`palette ${theme}`, async ({ page }) => {
    await open(page, { theme });
    await settle(page);
    await page.keyboard.press("Control+K");
    await page.getByTestId("search-palette").waitFor();
    await page.keyboard.type("down");
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${dir}/palette-${theme}.png` });
  });

  test(`collapsed ${theme}`, async ({ page }) => {
    await open(page, { theme });
    await settle(page);
    await page.keyboard.press("Control+B");
    await page.waitForTimeout(400);
    await page.screenshot({ path: `${dir}/collapsed-${theme}.png` });
  });

  test(`appearance + local ai ${theme}`, async ({ page }) => {
    await open(page, { theme, route: "/settings/appearance" });
    await settle(page);
    await page.screenshot({ path: `${dir}/settings-appearance-${theme}.png` });
    await page.getByRole("button", { name: "Relay Local AI" }).click();
    await settle(page);
    await page.screenshot({ path: `${dir}/settings-localai-${theme}.png` });
  });

  test(`installer ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 560, height: 620 });
    await open(page, { theme, route: "/installer" });
    await page.getByTestId("installer").waitFor();
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: `${dir}/installer-${theme}.png` });
  });

  test(`gallery ${theme}`, async ({ page }) => {
    await open(page, { theme, route: "/gallery" });
    await page.getByTestId("gallery").waitFor();
    await page.evaluate(() => document.fonts.ready);
    const g = page.getByTestId("gallery");
    const height = await g.evaluate((el) => el.scrollHeight);
    for (let y = 0, i = 0; y < height; y += 700, i++) {
      await g.evaluate((el, top) => el.scrollTo({ top }), y);
      await page.waitForTimeout(80);
      await page.screenshot({ path: `${dir}/gallery-${theme}-${String(i).padStart(2, "0")}.png` });
    }
  });
  for (const plat of ["mac", "mica"]) {
    test(`platform ${plat} ${theme}`, async ({ page }) => {
      await open(page, { theme, scenario: plat });
      await settle(page);
      // Simulate the desktop behind a transparent window.
      await page.addStyleTag({ content: "html { background: linear-gradient(135deg, #5b7fa6 0%, #c98d6b 60%, #e3c9a6 100%) !important; }" });
      await page.screenshot({ path: `${dir}/platform-${plat}-${theme}.png` });
      await page.keyboard.press(plat === "mac" ? "Meta+B" : "Control+B");
      await page.waitForTimeout(300);
      await page.screenshot({ path: `${dir}/platform-${plat}-${theme}-collapsed.png` });
    });
  }

  test(`narrow chat ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 520, height: 740 });
    await open(page, { theme, flow: "verify-fail" });
    await settle(page);
    await sendMessage(page, "Set brightness to 60%");
    await page.locator('[data-testid="task-timeline"][data-verdict="failed"]').waitFor({ timeout: 20_000 });
    await page.locator("[data-md=rendered]").first().waitFor();
    await page.screenshot({ path: `${dir}/narrow-chat-${theme}.png` });
  });
}