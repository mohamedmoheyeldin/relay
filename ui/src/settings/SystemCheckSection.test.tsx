// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axeViolations, renderUi, useMockBackend } from "@/test/render";
import type { Transport } from "@/lib/backend";
import { SystemCheckSection } from "./SystemCheckSection";
import { SystemCheckBanner } from "@/components/chat/SystemCheckBanner";
import { attentionItems, attentionText } from "@/lib/systemCheck";

let t: Transport;

describe("Settings › System check", () => {
  beforeEach(() => {
    localStorage.clear();
    t = useMockBackend("needs-setup,local-setup");
  });

  it("shows skeletons while checking, then each item with its status in words", async () => {
    renderUi(<SystemCheckSection />);
    expect(screen.getAllByRole("status", { name: "Checking" }).length).toBeGreaterThan(0);
    const rows = await screen.findAllByTestId("system-check-item", {}, { timeout: 3000 });
    expect(rows.map((r) => r.dataset.id)).toEqual(["local_ai", "claude", "codex", "node", "browser"]);
    const node = rows.find((r) => r.dataset.id === "node")!;
    expect(node).toHaveAttribute("data-status", "missing");
    expect(node).toHaveTextContent("Missing");
    expect(rows.find((r) => r.dataset.id === "claude")).toHaveTextContent("Ready");
    expect(screen.getByTestId("system-check-summary")).toHaveTextContent(/of 5 ready/);
    expect(await axeViolations(document.body)).toEqual([]);
  });

  it("runs actions: links open with open_url, the local AI action starts setup", async () => {
    renderUi(<SystemCheckSection />);
    const node = (await screen.findAllByTestId("system-check-item", {}, { timeout: 3000 })).find((r) => r.dataset.id === "node")!;
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(within(node).getByRole("button", { name: "Get Node.js" }));
    expect(calls).toHaveBeenCalledWith("open_url", { url: "https://nodejs.org/" });
    await userEvent.click(screen.getByRole("button", { name: "Set up Relay Local AI" }));
    expect(calls).toHaveBeenCalledWith("local_ai_setup", { action: "start" });
  });

  it("Check again probes once more", async () => {
    renderUi(<SystemCheckSection />);
    await screen.findAllByTestId("system-check-item", {}, { timeout: 3000 });
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(calls).toHaveBeenCalledWith("system_check", undefined);
    await waitFor(() => expect(screen.getAllByTestId("system-check-item")).toHaveLength(5), { timeout: 3000 });
  });
});

describe("Home banner", () => {
  beforeEach(() => localStorage.clear());

  it("counts only Relay Local AI, Node.js and browser problems", () => {
    const item = (id: string, status: "ok" | "warn" | "missing") => ({ id, label: id, status, detail: "", action: null });
    const check = { items: [item("local_ai", "ok"), item("claude", "missing"), item("node", "missing"), item("browser", "warn")], ok: 1, total: 4 };
    expect(attentionItems(check).map((i) => i.id)).toEqual(["node", "browser"]);
    expect(attentionText(2)).toBe("2 things need attention");
    expect(attentionText(1)).toBe("1 thing needs attention");
  });

  it("shows when something needs attention, links to the section and can be dismissed", async () => {
    useMockBackend("needs-setup");
    const { unmount } = renderUi(<SystemCheckBanner />);
    const banner = await screen.findByTestId("system-check-banner", {}, { timeout: 3000 });
    expect(banner).toHaveTextContent("2 things need attention");
    expect(within(banner).getByRole("button", { name: "Review" })).toBeInTheDocument();
    await userEvent.click(within(banner).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByTestId("system-check-banner")).toBeNull();
    unmount();
    // Still dismissed for the same problems.
    renderUi(<SystemCheckBanner />);
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.queryByTestId("system-check-banner")).toBeNull();
  });

  it("stays hidden when everything is ready", async () => {
    const t2 = useMockBackend("");
    const spy = vi.spyOn(t2, "invoke");
    renderUi(<SystemCheckBanner />);
    await waitFor(() => expect(spy).toHaveBeenCalledWith("system_check", undefined));
    await new Promise((r) => setTimeout(r, 300));
    expect(screen.queryByTestId("system-check-banner")).toBeNull();
  });
});
