import { expect, test } from "@playwright/test";

test.use({ viewport: { width: 560, height: 620 } });

test("install screen: options, progress, done", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/?mode=install&scenario=fast");
  await expect(page.getByRole("heading", { name: "Install Relay" })).toBeVisible();
  await expect(page.getByText("C:\\Program Files\\Relay")).toBeVisible();
  await expect(page.getByText("Windows will ask for permission.")).toBeVisible();
  const shortcut = page.getByRole("switch", { name: "Add a desktop shortcut" });
  await expect(shortcut).toHaveAttribute("aria-checked", "true");
  await shortcut.click();
  await expect(shortcut).toHaveAttribute("aria-checked", "false");
  await page.getByRole("button", { name: "Install" }).click();
  await expect(page.getByRole("heading", { name: "Installing Relay…" })).toBeVisible();
  await expect(page.getByRole("progressbar")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Relay is installed" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("button", { name: "Open Relay" })).toBeFocused();
});

test("install failure explains and offers to try again", async ({ page }) => {
  await page.goto("/?mode=install&fail=1&scenario=fast");
  await page.getByRole("button", { name: "Install" }).click();
  await expect(page.getByRole("alert")).toContainText("access denied", { timeout: 15_000 });
  await expect(page.getByRole("button", { name: "Try again" })).toBeVisible();
});

test("uninstall screen: data choice, progress, done", async ({ page }) => {
  await page.goto("/?mode=uninstall&scenario=fast");
  await expect(page.getByRole("heading", { name: "Remove Relay?" })).toBeVisible();
  const del = page.getByRole("switch", { name: /Also delete my chats/ });
  await expect(del).toHaveAttribute("aria-checked", "false");
  await del.click();
  await page.getByRole("button", { name: "Remove Relay" }).click();
  await expect(page.getByRole("heading", { name: "Relay was removed" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("Your chats, settings and the local AI were deleted too.")).toBeVisible();
});
