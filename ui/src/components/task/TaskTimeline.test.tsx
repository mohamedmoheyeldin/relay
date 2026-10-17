// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { taskFixture } from "@/lib/fixtures";
import { renderUi, axeViolations } from "@/test/render";
import { TaskTimeline, rawLog } from "./TaskTimeline";

describe("TaskTimeline", () => {
  it("shows the route, the AI-proposal label and a verified verdict", () => {
    renderUi(<TaskTimeline item={taskFixture("verified")} />);
    expect(screen.getByText("Hybrid: Claude Code plans it, Relay runs it here")).toBeInTheDocument();
    expect(screen.getByTestId("route-badge")).toHaveTextContent("Cloud");
    expect(screen.getByTestId("plan-source")).toHaveTextContent("Proposed by Claude Code · AI-generated");
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Verified · 3 of 3 steps");
    expect(screen.getAllByTestId("plan-step").map((li) => li.getAttribute("data-status"))).toEqual(["verified", "verified", "verified"]);
  });

  it("distinguishes 'ran but not verified' and a neutral done tick", () => {
    renderUi(<TaskTimeline item={taskFixture("unverified")} />);
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Ran, 2 of 3 verified");
    const done = screen.getAllByTestId("plan-step")[1]!;
    expect(done).toHaveAttribute("data-status", "done");
    expect(within(done).getByText("Done")).toBeInTheDocument();
  });

  it("opens a failed step with its reason and offers Retry", async () => {
    const onRetry = vi.fn();
    renderUi(<TaskTimeline item={taskFixture("failed")} onRetry={onRetry} />);
    expect(screen.getByTestId("task-verdict")).toHaveTextContent("Failed at step 2");
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
    expect(screen.getByTestId("confirm-line")).toHaveTextContent("Waiting for your approval");
    rerender(<TaskTimeline item={taskFixture("declined")} />);
    expect(screen.getByTestId("confirm-line")).toHaveTextContent("Declined");
    expect(screen.getByTestId("task-verdict")).toHaveTextContent(/declined/i);
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

  it("has no serious accessibility violations", async () => {
    const { container } = renderUi(<TaskTimeline item={taskFixture("failed")} onRetry={() => undefined} />);
    expect(await axeViolations(container)).toEqual([]);
  });
});
