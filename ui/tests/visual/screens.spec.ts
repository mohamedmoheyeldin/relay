// Visual regression: key screens in light and dark at 1100x740 and 520x740.
// Update baselines with `pnpm test:visual:update` after an intended visual change, then review them.
import { expect, test, type Page } from "@playwright/test";
import { open, settle, type Theme } from "../helpers";

const SIZES = [
  { name: "1100", width: 1100, height: 740 },
  { name: "520", width: 520, height: 740 },
];

const SCREENS: { name: string; route: string; scenario?: string; prepare?: (page: Page, narrow: boolean) => Promise<void> }[] = [
  { name: "home", route: "/" },
  {
    name: "chat-task",
    route: "/",
    prepare: async (page, narrow) => {
      if (narrow) await page.getByRole("button", { name: "Open sidebar" }).click();
      await page.getByRole("region", { name: "Pinned" }).getByRole("button", { name: "Tidy up my Downloads folder", exact: true }).click();
      await page.getByTestId("task-timeline").waitFor();
      await page.locator("[data-md=rendered]").first().waitFor();
    },
  },
  {
    // v3: a question answered in chat (no task card), with chips and a highlighted code block.
    name: "chat-code",
    route: "/",
    prepare: async (page, narrow) => {
      if (narrow) await page.getByRole("button", { name: "Open sidebar" }).click();
      await page.getByRole("button", { name: "Playwright test for a store search", exact: true }).click();
      await page.getByTestId("code-block").first().waitFor();
      await page.locator("[data-testid=code-block][data-highlighted=true]").first().waitFor();
    },
  },
  {
    // v3: a web task with a collapsed step list that ends with the user's turn.
    name: "chat-web",
    route: "/",
    prepare: async (page, narrow) => {
      if (narrow) await page.getByRole("button", { name: "Open sidebar" }).click();
      await page.getByRole("button", { name: "Find remote Rust jobs on Dice", exact: true }).click();
      await page.getByTestId("task-timeline").waitFor();
      await page.locator("[data-asks-user=true] [data-md=rendered]").waitFor();
    },
  },
  { name: "tasks", route: "/tasks/wf-2" },
  { name: "settings-general", route: "/settings/general" },
  { name: "settings-privacy", route: "/settings/privacy", scenario: "codex-missing" },
  { name: "settings-appearance", route: "/settings/appearance" },
];

for (const theme of ["light", "dark"] as Theme[]) {
  for (const size of SIZES) {
    for (const s of SCREENS) {
      test(`${s.name} ${theme} ${size.name}`, async ({ page }) => {
        await page.setViewportSize(size);
        await open(page, { theme, route: s.route, scenario: s.scenario, freeze: true });
        await settle(page);
        if (s.prepare) await s.prepare(page, size.width < 768);
        await settle(page);
        await page.mouse.move(0, size.height - 1);
        await expect(page).toHaveScreenshot(`${s.name}-${theme}-${size.name}.png`);
      });
    }
  }
  test(`installer ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 560, height: 620 });
    await open(page, { theme, route: "/installer", freeze: true });
    await page.getByTestId("installer").waitFor();
    await page.evaluate(() => document.fonts.ready);
    await expect(page).toHaveScreenshot(`installer-${theme}.png`);
  });
}
