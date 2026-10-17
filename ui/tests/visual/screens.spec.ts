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
  { name: "tasks", route: "/tasks/wf-2" },
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
