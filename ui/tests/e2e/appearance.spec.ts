// Appearance settings, the resizable sidebar and the system check, in the browser (mock backend).
import { expect, test } from "@playwright/test";
import { open, settle } from "../helpers";

test("code theme pickers search, preview and re-highlight code at once", async ({ page }) => {
  await open(page, { theme: "dark", route: "/settings/appearance" });
  await settle(page);
  const dark = page.getByTestId("code-theme-dark");
  await dark.getByRole("combobox", { name: /Dark theme/ }).click();
  const search = page.getByRole("combobox", { name: "Search dark themes" });
  await search.fill("drac");
  await expect(page.getByRole("option")).toHaveText(["Dracula", "Dracula Soft"]);
  await search.press("Enter");
  await expect(dark.getByRole("combobox", { name: "Dark theme: Dracula" })).toBeVisible();
  const preview = dark.getByTestId("code-theme-preview");
  await expect(preview).toHaveAttribute("data-theme-id", "dracula");
  await expect(preview).toHaveAttribute("data-highlighted", "true");
  await expect(preview).toHaveCSS("background-color", "rgb(40, 42, 54)");
  // The app's code surface follows at once (dark mode uses the dark theme).
  expect(await page.evaluate(() => document.documentElement.style.getPropertyValue("--relay-code-bg"))).toBe("#282a36");

  // A code block in a chat uses the chosen themes.
  await page.goto(page.url().replace(/#.*$/, "#/"));
  await page.getByRole("button", { name: "Playwright test for a store search", exact: true }).click();
  await expect(page.locator("[data-testid=code-block][data-highlighted=true]").first()).toHaveAttribute("data-code-themes", "github-light-default/dracula");
});

test("interface options apply without a reload and are cached for the next start", async ({ page }) => {
  await open(page, { route: "/settings/appearance" });
  await settle(page);
  await page.getByRole("radiogroup", { name: "Text size" }).getByRole("radio", { name: "Large" }).click();
  await page.getByRole("radiogroup", { name: "Density" }).getByRole("radio", { name: "Compact" }).click();
  await page.getByRole("radiogroup", { name: "Interface font" }).getByRole("radio", { name: "Atkinson Hyperlegible" }).click();
  const html = page.locator("html");
  await expect(html).toHaveAttribute("data-text-size", "large");
  await expect(html).toHaveAttribute("data-density", "compact");
  await expect(html).toHaveAttribute("data-ui-font", "atkinson");
  await page.getByLabel("Code font").fill("Cascadia Code");
  await page.getByLabel("Code font").press("Enter");
  expect(await page.evaluate(() => document.documentElement.style.getPropertyValue("--relay-font-code"))).toContain("Cascadia Code");
  // Cached for the first paint of the next start (the backend copy is checked in unit tests).
  const cached = await page.evaluate(() => JSON.parse(localStorage.getItem("relay.appearance") ?? "{}") as Record<string, unknown>);
  expect(cached).toMatchObject({ textSize: "large", density: "compact", uiFont: "atkinson", codeFont: "Cascadia Code" });
});

test("the sidebar resizes by dragging, by keyboard, and resets on double-click", async ({ page }) => {
  await open(page);
  await settle(page);
  const sidebar = page.getByRole("complementary", { name: "Sidebar" });
  const handle = page.getByRole("separator", { name: "Resize sidebar" });
  await expect(sidebar).toHaveJSProperty("offsetWidth", 280);
  await handle.hover();
  await expect(page.getByRole("tooltip")).toHaveText("Drag to resize");
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + 200);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + 200, { steps: 5 });
  await page.mouse.up();
  await expect(sidebar).toHaveJSProperty("offsetWidth", 360);
  await expect(handle).toHaveAttribute("aria-valuenow", "360");
  // Limits.
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + 200);
  await page.mouse.down();
  await page.mouse.move(box.x + 600, box.y + 200, { steps: 5 });
  await page.mouse.up();
  await expect(sidebar).toHaveJSProperty("offsetWidth", 420);
  await handle.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(sidebar).toHaveJSProperty("offsetWidth", 404);
  await handle.dblclick();
  await expect(sidebar).toHaveJSProperty("offsetWidth", 280);
  // Collapse still works and the width comes back.
  await page.keyboard.press("Control+b");
  await expect(page.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
  await expect(handle).toHaveCount(0);
  await page.keyboard.press("Control+b");
  await expect(sidebar).toHaveJSProperty("offsetWidth", 280);
});

test("system check: Home banner leads to the section; actions and Check again work", async ({ page }) => {
  await open(page, { scenario: "needs-setup" });
  await settle(page);
  const banner = page.getByTestId("system-check-banner");
  await expect(banner).toContainText("2 things need attention");
  await banner.getByRole("button", { name: "Review" }).click();
  await expect(page.getByRole("heading", { name: "System check" })).toBeVisible();
  await expect(page.getByTestId("system-check-item")).toHaveCount(5);
  await expect(page.locator("[data-testid=system-check-item][data-id=node]")).toContainText("Missing");
  await expect(page.locator("[data-testid=system-check-item][data-id=browser]").getByRole("button", { name: "Get the Playwright extension" })).toBeVisible();
  await page.getByRole("button", { name: "Check again" }).click();
  await expect(page.getByTestId("system-check-item")).toHaveCount(5);
  // Dismissed banners stay dismissed for the same problems.
  await page.getByRole("button", { name: "Back to Relay" }).click();
  await page.getByTestId("system-check-banner").getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByTestId("system-check-banner")).toHaveCount(0);
  await page.reload();
  await settle(page);
  await page.waitForTimeout(400);
  await expect(page.getByTestId("system-check-banner")).toHaveCount(0);
});

test("browser setting shows its mode and the extension link", async ({ page }) => {
  await open(page, { scenario: "browser-separate", route: "/settings/privacy" });
  await expect(page.getByTestId("browser-mode")).toHaveText("Separate Relay browser");
  await expect(page.getByTestId("browser-action")).toHaveText(/Get the Playwright extension/);
});
