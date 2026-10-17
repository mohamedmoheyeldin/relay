import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { open, settle } from "../helpers";

test("search palette keyboard flow", async ({ page }) => {
  await open(page);
  await settle(page);
  await page.keyboard.press("Control+K");
  const box = page.getByRole("combobox", { name: /Search chats, projects and commands/ });
  await expect(box).toBeFocused();
  await page.keyboard.type("ssd");
  const hit = page.getByRole("option", { name: /Compare SSD prices/ });
  await expect(hit).toBeVisible();
  // Commands come first; arrow down until the chat is selected.
  for (let i = 0; i < 5 && (await hit.getAttribute("aria-selected")) !== "true"; i++) await page.keyboard.press("ArrowDown");
  await expect(hit).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("search-palette")).toBeHidden();
  await expect(page).toHaveURL(/#\/chat\//);
  await expect(page.getByRole("banner").or(page.locator("header")).first()).toContainText("Compare SSD prices");
  // Esc closes the palette without navigating.
  await page.keyboard.press("Control+K");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("search-palette")).toBeHidden();
});

test("settings navigation by keyboard; Esc goes back", async ({ page }) => {
  await open(page, { route: "/tasks" });
  await settle(page);
  await page.keyboard.press("Control+,");
  await expect(page).toHaveURL(/#\/settings\/general/);
  await expect(page.getByRole("heading", { level: 1, name: "General" })).toBeVisible();
  await page.getByRole("button", { name: "AI & Privacy" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "AI & Privacy" })).toBeVisible();
  // Roving focus in the section list.
  await page.getByRole("button", { name: "AI & Privacy" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("button", { name: "Relay Local AI" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1, name: "Relay Local AI" })).toBeVisible();
  // Settings search.
  await page.getByRole("textbox", { name: "Search settings" }).fill("shortcuts");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1, name: "Keyboard" })).toBeVisible();
  await page.locator("main").click({ position: { x: 5, y: 300 } });
  await page.keyboard.press("Escape");
  await expect(page).toHaveURL(/#\/tasks$/);
});

test("sidebar collapses to a rail and the shortcut toggles it", async ({ page }) => {
  await open(page);
  await settle(page);
  await page.keyboard.press("Control+B");
  await expect(page.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
  await page.reload();
  await expect(page.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
  await page.keyboard.press("Control+B");
  await expect(page.getByRole("button", { name: "Collapse sidebar" })).toBeVisible();
});

test("narrow windows use an overlay drawer that Esc closes", async ({ page }) => {
  await page.setViewportSize({ width: 520, height: 740 });
  await open(page);
  await settle(page);
  await page.getByRole("button", { name: "Open sidebar" }).click();
  const drawer = page.getByRole("complementary", { name: "Sidebar" });
  await expect(drawer.getByRole("button", { name: "Close sidebar" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(drawer.getByRole("button", { name: "Close sidebar" })).toBeHidden();
});

test("task history: filter, keyboard and detail", async ({ page }) => {
  await open(page, { route: "/tasks" });
  await settle(page);
  const list = page.getByTestId("task-list");
  await expect(list.getByTestId("task-row").first()).toBeVisible();
  await page.getByRole("radio", { name: "Failed" }).click();
  await expect(list.getByTestId("task-row").first()).toContainText(/Failed|Refused|Unavailable/);
  await page.getByRole("radio", { name: "All" }).click();
  await list.focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#\/tasks\/wf-/);
  await expect(page.getByTestId("task-detail").getByTestId("task-timeline")).toBeVisible();
});

for (const route of ["/", "/tasks/wf-1", "/settings/privacy"]) {
  for (const theme of ["light", "dark"] as const) {
    test(`axe (with colour contrast): ${route} ${theme}`, async ({ page }) => {
      await open(page, { route, theme });
      await settle(page);
      const res = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
      const serious = res.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
      expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
    });
  }
}

test("axe on a seeded chat with its timeline", async ({ page }) => {
  await open(page);
  await settle(page);
  await page.getByRole("region", { name: "Pinned" }).getByRole("button", { name: "Tidy up my Downloads folder", exact: true }).click();
  await page.getByTestId("task-timeline").waitFor();
  const res = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  expect(res.violations.filter((v) => v.impact === "serious" || v.impact === "critical").map((v) => v.id)).toEqual([]);
});
