// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, screen, within } from "@testing-library/react";
import { useAppearance } from "@/store/appearance";
import userEvent from "@testing-library/user-event";
import { taskFixture } from "@/lib/fixtures";
import { renderUi, axeViolations } from "@/test/render";
import { TaskTimeline, rawLog } from "./TaskTimeline";

describe("TaskTimeline", () => {
  it("shows the route, the AI-proposal label and a verified verdict", () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    expect(screen.getByText("Claude Code makes the plan; Relay does the steps on this PC")).toBeInTheDocument();
    expect(screen.getByTestId("route-badge")).toHaveTextContent("Cloud");
    expect(screen.getByTestId("plan-source")).toHaveTextContent("Proposed by Claude Code · Relay checks each step");
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Done · all 3 steps checked");
    expect(screen.getAllByTestId("plan-step").map((li) => li.getAttribute("data-status"))).toEqual(["verified", "verified", "verified"]);
  });

  it("distinguishes 'ran but not verified' and a neutral done tick", () => {
    renderUi(<TaskTimeline item={taskFixture("unverified")} />);
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Done · 2 of 3 steps checked");
    const done = screen.getAllByTestId("plan-step")[1]!;
    expect(done).toHaveAttribute("data-status", "done");
    expect(within(done).getByText("Done, not checked")).toBeInTheDocument();
  });

  it("opens a failed step with its reason and offers Retry", async () => {
    const onRetry = vi.fn();
    renderUi(<TaskTimeline item={taskFixture("failed")} onRetry={onRetry} />);
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Step 2 didn't work");
    expect(screen.getByText("Ran, but the check failed: no files matched")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: /Retry task/ }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("expands the privacy ledger to list what was shared", async () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    const ledger = screen.getByRole("button", { name: /Shared with Claude Code/ });
    expect(ledger).toHaveAttribute("aria-expanded", "false");
    expect(ledger).toHaveTextContent("3 categories · 48 items · 12 KB");
    await userEvent.click(ledger);
    expect(ledger).toHaveAttribute("aria-expanded", "true");
    const table = screen.getByRole("table");
    expect(within(table).getByText("Window titles")).toBeInTheDocument();
    expect(within(table).getByText("41")).toBeInTheDocument();
  });

  it("says nothing was shared for local runs", () => {
    renderUi(<TaskTimeline item={taskFixture("local")} />);
    expect(screen.getByText(/Nothing was shared/)).toBeInTheDocument();
    expect(screen.getByTestId("route-badge")).toHaveTextContent("Relay Local AI");
  });

  it("has a Stop button only while live and running", async () => {
    const onStop = vi.fn();
    const { rerender } = renderUi(<TaskTimeline item={taskFixture("running")} live onStop={onStop} />);
    await userEvent.click(screen.getByRole("button", { name: "Stop the task" }));
    expect(onStop).toHaveBeenCalled();
    rerender(<TaskTimeline item={taskFixture("verified")} live onStop={onStop} />);
    expect(screen.queryByRole("button", { name: "Stop the task" })).toBeNull();
  });

  it("shows a pending approval inline and the declined state afterwards", () => {
    const { rerender } = renderUi(<TaskTimeline item={taskFixture("awaiting")} live />);
    expect(screen.getByTestId("confirm-line")).toHaveAttribute("data-state", "pending");
    expect(screen.getByTestId("confirm-line")).toHaveTextContent("Waiting for your OK");
    rerender(<TaskTimeline item={taskFixture("declined")} />);
    expect(screen.getByTestId("confirm-line")).toHaveTextContent("You said no");
    expect(screen.getByTestId("task-verdict")).toHaveTextContent(/you said no/i);
  });

  it("keeps raw output behind Details", async () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    expect(screen.queryByLabelText("Raw task log")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByLabelText("Raw task log")).toHaveTextContent("recycle_files");
    expect(rawLog(taskFixture("verified"))).toMatch(/outcome completed/);
  });

  it("steps are one tab stop with arrow-key navigation", async () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    const buttons = screen.getAllByTestId("plan-step").map((li) => within(li).getAllByRole("button")[0]!);
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    buttons[0]!.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(buttons[1]).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(buttons[2]).toHaveFocus();
  });

  it("tags what did the work in the header and how each step ran", () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    const chips = screen.getAllByTestId("used-chip");
    expect(chips.map((c) => c.getAttribute("data-kind"))).toEqual(["cloud_ai", "native"]);
    expect(chips[0]).toHaveTextContent("Cloud AI · Claude Code");
    expect(chips[1]).toHaveTextContent("Native tools · Windows");
    // Not colour only: each chip carries an icon and words.
    expect(chips[0]!.querySelector("svg")).not.toBeNull();
    expect(screen.getAllByTestId("via-chip").every((c) => c.getAttribute("data-via") === "native")).toBe(true);
  });

  it("shows Sensitive info only for real sensitive data, and destructive steps in plain words", () => {
    renderUi(<TaskTimeline item={taskFixture("sensitive")} />);
    const [plain, secret, destructive] = screen.getAllByTestId("plan-step");
    expect(within(plain!).queryByTestId("risk-badge")).toBeNull(); // risk "sensitive" alone shows nothing
    expect(within(secret!).getByTestId("risk-badge")).toHaveTextContent("Sensitive info");
    expect(within(destructive!).getByTestId("risk-badge")).toHaveTextContent("Changes or deletes data");
    expect(screen.queryByText(/^Sensitive$/)).toBeNull();
  });

  it("collapses a long web task to the latest step and expands on request", async () => {
    renderUi(<TaskTimeline item={taskFixture("web")} />);
    expect(screen.getAllByTestId("plan-step")).toHaveLength(1);
    expect(screen.getByTestId("plan-step")).toHaveTextContent("Open Easy Apply for the top match");
    expect(screen.getByTestId("via-chip")).toHaveTextContent("Playwright browser");
    const toggle = screen.getByTestId("steps-toggle");
    expect(toggle).toHaveTextContent("Show all 12 steps (11 more)");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggle);
    expect(screen.getAllByTestId("plan-step")).toHaveLength(12);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Waiting for you");
    expect(screen.getAllByTestId("used-chip").map((c) => c.textContent)).toEqual(["Used Cloud AI · Claude Code", "Used Playwright browser"]);
  });

  it("has no serious accessibility violations", async () => {
    const { container } = renderUi(<TaskTimeline item={taskFixture("failed")} onRetry={() => undefined} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe("TaskTimeline task views (Settings › Appearance › Default task view)", () => {
  afterEach(() => useAppearance.getState().set({ taskView: "detailed" }));

  it("simple shows the verdict without the step list", () => {
    useAppearance.getState().set({ taskView: "simple" });
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    expect(screen.getByTestId("task-timeline")).toHaveAttribute("data-view", "simple");
    expect(screen.queryAllByTestId("plan-step")).toHaveLength(0);
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Done · all 3 steps checked");
  });

  it("verbose opens every step and the raw log; switching applies at once", () => {
    useAppearance.getState().set({ taskView: "verbose" });
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    for (const step of screen.getAllByTestId("plan-step")) expect(within(step).getByRole("button")).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("Raw task log")).toBeInTheDocument();
    expect(screen.getAllByTestId("plan-step")).toHaveLength(3);
    act(() => useAppearance.getState().set({ taskView: "detailed" }));
    expect(screen.getAllByTestId("task-timeline")).toHaveLength(1);
    expect(screen.queryByLabelText("Raw task log")).toBeNull();
    expect(screen.getAllByTestId("plan-step")).toHaveLength(3);
  });
});
