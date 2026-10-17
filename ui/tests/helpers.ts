import { expect, type Page } from "@playwright/test";

export type Theme = "light" | "dark";

/** Opens the app with a theme, mock scenario and route. The clock is frozen for stable visuals. */
export async function open(page: Page, opts: { theme?: Theme; scenario?: string; flow?: string; route?: string; freeze?: boolean; reduceMotion?: boolean } = {}) {
  const { theme = "light", scenario = "", flow, route = "/", freeze = false, reduceMotion = true } = opts;
  await page.addInitScript((t) => {
    try {
      localStorage.setItem("relay.appearance", JSON.stringify({ mode: t, theme: "relay", accent: "terracotta", prose: "sans", custom: {} }));
    } catch {
      /* ignore */
    }
  }, theme);
  if (reduceMotion) await page.emulateMedia({ reducedMotion: "reduce", colorScheme: theme });
  if (freeze) await page.clock.setFixedTime(new Date("2026-10-16T19:30:00-04:00"));
  const q = new URLSearchParams();
  const flags = ["fast", ...scenario.split(",").filter(Boolean)];
  q.set("scenario", flags.join(","));
  if (flow) q.set("flow", flow);
  await page.goto(`/?${q.toString()}#${route}`);
  await expect(page.locator("#root")).not.toBeEmpty();
}

/** Waits until fonts are ready and skeletons are gone. */
export async function settle(page: Page) {
  await page.evaluate(() => document.fonts.ready);
  await expect(page.locator(".skeleton")).toHaveCount(0, { timeout: 10_000 });
}

export async function sendMessage(page: Page, text: string) {
  const box = page.getByRole("textbox", { name: "Message Relay" });
  await box.fill(text);
  await box.press("Enter");
}
