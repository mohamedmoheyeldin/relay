import { expect, test, type Page } from "@playwright/test";
import { open, sendMessage, settle } from "../helpers";

const timeline = (page: Page) => page.getByTestId("task-timeline").last();

test("send a message and watch the timeline reach verified", async ({ page }) => {
  await open(page, { flow: "cloud" });
  await settle(page);
  await sendMessage(page, "Rename my screenshots by date");
  await expect(page).toHaveURL(/#\/chat\/c/);
  const t = timeline(page);
  await expect(t.getByTestId("route-badge")).toHaveText("Cloud");
  await expect(t.getByTestId("plan-source")).toContainText("Proposed by Claude Code");
  await expect(t.getByRole("button", { name: /Shared with Claude Code/ })).toBeVisible();
  await expect(t).toHaveAttribute("data-verdict", "verified", { timeout: 15_000 });
  await expect(t.getByTestId("task-verdict")).toHaveText("Done · all 3 steps checked");
  await expect(page.getByTestId("live-region")).toContainText("Relay:");
  // The sidebar lists the new chat under Today with its title.
  await expect(page.getByRole("region", { name: "Recent chats" }).getByRole("button", { name: "Rename my screenshots by date", exact: true })).toBeVisible();
});

test("switch provider and privacy mode; the next task uses them", async ({ page }) => {
  await open(page, { scenario: "all-ready", flow: "cloud" });
  await settle(page);
  await page.getByTestId("provider-picker").click();
  await page.getByTestId("provider-option-codex").click();
  await expect(page.getByTestId("provider-picker")).toHaveAccessibleName(/Codex/);
  await sendMessage(page, "Sort my desktop icons");
  await expect(timeline(page).getByTestId("plan-source")).toContainText("Proposed by Codex", { timeout: 10_000 });
  await expect(timeline(page)).not.toHaveAttribute("data-verdict", "running", { timeout: 15_000 });

  await page.getByRole("radio", { name: /Local only/ }).click();
  await expect(page.getByRole("radio", { name: /Local only/ })).toHaveAttribute("aria-checked", "true");
  await sendMessage(page, "Sort my desktop icons again");
  await expect(timeline(page).getByTestId("route-badge")).toHaveText("Relay Local AI", { timeout: 10_000 });
  await expect(timeline(page).getByText(/Nothing was shared/)).toBeVisible();
});

test("decline then approve a permission", async ({ page }) => {
  await open(page, { flow: "declined" });
  await settle(page);
  await sendMessage(page, "Empty the Recycle Bin");
  const dialog = page.getByTestId("permission-dialog");
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("button", { name: "Decline" })).toBeFocused();
  await page.getByRole("button", { name: "Decline" }).click();
  await expect(dialog).toBeHidden();
  await expect(timeline(page)).toHaveAttribute("data-verdict", "declined", { timeout: 10_000 });
  await expect(timeline(page).getByTestId("confirm-line")).toHaveAttribute("data-state", "declined");

  await sendMessage(page, "Empty the Recycle Bin, I'm sure");
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await page.keyboard.press("Tab"); // Decline -> Approve
  await expect(page.getByRole("button", { name: "Approve" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(timeline(page)).toHaveAttribute("data-verdict", "verified", { timeout: 10_000 });
  await expect(timeline(page).getByTestId("confirm-line")).toHaveAttribute("data-state", "approved");
});

test("Esc in the permission dialog declines", async ({ page }) => {
  await open(page, { flow: "download" });
  await settle(page);
  await sendMessage(page, "Tidy up my Downloads folder");
  await expect(page.getByTestId("permission-dialog")).toBeVisible({ timeout: 10_000 });
  await page.keyboard.press("Escape");
  await expect(timeline(page)).toHaveAttribute("data-verdict", "declined", { timeout: 10_000 });
});

test("stop a run", async ({ page }) => {
  await open(page, { scenario: "slow", flow: "cloud" });
  await settle(page);
  await sendMessage(page, "A long careful task");
  const t = timeline(page);
  await expect(t.getByTestId("plan-step").first()).toHaveAttribute("data-status", "running", { timeout: 15_000 });
  await t.getByRole("button", { name: "Stop the task" }).click();
  await expect(t).toHaveAttribute("data-verdict", "stopped", { timeout: 15_000 });
  await expect(t.getByTestId("task-verdict")).toHaveText("Stopped");
  await expect(page.getByRole("button", { name: "Send message" })).toBeVisible();
});

test("a failed verification shows where and why, with retry", async ({ page }) => {
  await open(page, { flow: "verify-fail" });
  await settle(page);
  await sendMessage(page, "Set brightness to 60%");
  const t = timeline(page);
  await expect(t).toHaveAttribute("data-verdict", "failed", { timeout: 15_000 });
  await expect(t.getByTestId("task-verdict")).toHaveText("Step 2 didn't work");
  await expect(t.getByText("Ran, but the check failed: brightness still reads 100%")).toBeVisible();
  await expect(t.getByRole("button", { name: /Retry task/ })).toBeVisible();
});

test("provider not signed in shows the fix in plain words", async ({ page }) => {
  await open(page, { flow: "not-signed-in" });
  await settle(page);
  await sendMessage(page, "Do something");
  const alert = page.getByRole("alert").filter({ hasText: "isn't signed in" });
  await expect(alert).toBeVisible({ timeout: 10_000 });
  await expect(alert).toContainText("claude auth login");
  await alert.getByRole("button", { name: /AI & Privacy settings/ }).click();
  await expect(page).toHaveURL(/#\/settings\/privacy/);
});
