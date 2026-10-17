// Contract v3 in the browser (mock backend): chat answers without a task card, code blocks, the
// "what did the work" chips, web tasks with a growing step list, the user's turn, browser control,
// and nothing waiting on the slow get_hybrid probe.
import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { open, sendMessage, settle } from "../helpers";

test("a question is answered in chat: no task card, chips and a proper code block", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await open(page);
  await settle(page);
  await sendMessage(page, "example of playwright code for testing amazon search?");
  const answer = page.getByTestId("assistant-message").last();
  const code = answer.getByRole("figure", { name: "TypeScript code" });
  await expect(code).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId("task-timeline")).toHaveCount(0);
  await expect(answer.getByTestId("used-chip")).toHaveText(/Cloud AI · Claude Code/);
  await expect(code).toHaveAttribute("data-highlighted", "true", { timeout: 10_000 });
  // Never wraps: the code scrolls sideways inside its own box.
  const pre = code.locator("pre");
  expect(await pre.evaluate((el) => getComputedStyle(el).whiteSpace)).toBe("pre");
  expect(await pre.evaluate((el) => getComputedStyle(el).overflowX)).toBe("auto");
  expect(await pre.evaluate((el) => getComputedStyle(el).fontFamily)).toMatch(/Geist Mono/);
  // Inline code stays inline.
  await expect(answer.locator("p code").first()).toHaveText("npx playwright test");
  // Copy by keyboard, with a Copied state.
  const copy = code.getByRole("button", { name: "Copy TypeScript code" });
  await copy.focus();
  await page.keyboard.press("Enter");
  await expect(code.getByRole("button", { name: "Copied to the clipboard" })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('import { test, expect } from "@playwright/test";');
});

test("a web task grows step by step, collapses, and ends with the user's turn", async ({ page }) => {
  await open(page, { flow: "web" });
  await settle(page);
  await sendMessage(page, "Find remote Rust jobs on Dice and help me apply");
  const t = page.getByTestId("task-timeline").last();
  await expect(t).toHaveAttribute("data-verdict", "waiting", { timeout: 20_000 });
  await expect(t.getByTestId("task-verdict")).toHaveText("Waiting for you");
  await expect(t.getByTestId("used-chip")).toHaveText([/Cloud AI · Claude Code/, /Native tools · Windows/, /Playwright browser/]);
  await expect(t.getByTestId("plan-step")).toHaveCount(1);
  await expect(t.getByTestId("plan-step")).toContainText("Open “Easy Apply” for the top match");
  await t.getByTestId("steps-toggle").click();
  await expect(t.getByTestId("plan-step")).toHaveCount(9);
  await expect(t.getByTestId("via-chip").first()).toHaveText("Playwright browser");
  const turn = page.locator("[data-asks-user=true]");
  await expect(turn).toContainText("Your turn");
  await expect(turn).toContainText("Sign in to Dice in the Relay browser window, then say continue");
});

test("Sensitive is not shown for ordinary steps; destructive steps say what they do", async ({ page }) => {
  await open(page);
  await settle(page);
  await page.getByRole("region", { name: "Pinned" }).getByRole("button", { name: "Tidy up my Downloads folder", exact: true }).click();
  const t = page.getByTestId("task-timeline");
  await expect(t.locator("[data-testid=plan-step] > button [data-testid=risk-badge]")).toHaveText(["Changes or deletes data"]);
  await expect(t.getByTestId("confirm-line")).toContainText("Changes or deletes data");
  await expect(t.getByText("Sensitive", { exact: true })).toHaveCount(0);
});

test("the composer works at once while the slow provider probe is still running", async ({ page }) => {
  await open(page, { scenario: "slow-probe", flow: "native" });
  await expect(page.getByRole("textbox", { name: "Message Relay" })).toBeEditable();
  // The privacy controls show skeletons meanwhile; nothing else waits for them.
  await expect(page.locator(".skeleton").first()).toBeVisible();
  await sendMessage(page, "Open Notepad and type hello");
  await expect(page.getByTestId("task-timeline").last()).toBeVisible({ timeout: 2_500 });
  await expect(page.getByRole("radio", { name: /Hybrid/ })).toBeVisible({ timeout: 6_000 });
});

test("settings: no Personalization; name and custom instructions live in General", async ({ page }) => {
  await open(page, { route: "/settings/general" });
  await settle(page);
  await expect(page.getByRole("button", { name: "Personalization" })).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Your name" })).toHaveValue("Mohamed Moheyeldin");
  const box = page.getByRole("textbox", { name: "Custom instructions" });
  await box.fill("Prefer short answers.");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved")).toBeVisible();
  // An old link to the removed page lands on General.
  await page.goto(page.url().replace(/#.*$/, "#/settings/personalization"));
  await expect(page.getByRole("heading", { level: 1, name: "General" })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Custom instructions" })).toHaveValue("Prefer short answers.");
});

test("browser control setting, and its reason when unavailable", async ({ page }) => {
  await open(page, { route: "/settings/privacy" });
  await settle(page);
  const sw = page.getByRole("switch", { name: "Browser control (Playwright)" });
  await expect(sw).toBeChecked();
  await sw.click();
  await expect(sw).not.toBeChecked();

  await open(page, { route: "/settings/privacy", scenario: "no-node" });
  await settle(page);
  await expect(page.getByRole("switch", { name: "Browser control (Playwright)" })).toBeDisabled();
  await expect(page.getByTestId("browser-unavailable")).toContainText("Node.js was not found");
});

for (const theme of ["light", "dark"] as const) {
  test(`axe (with colour contrast) on a chat answer with code: ${theme}`, async ({ page }) => {
    await open(page, { theme });
    await settle(page);
    await page.getByRole("button", { name: "Playwright test for a store search", exact: true }).click();
    const code = page.getByTestId("code-block").first();
    await expect(code).toHaveAttribute("data-highlighted", "true", { timeout: 10_000 });
    const res = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
    const serious = res.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
    expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
  });
}
