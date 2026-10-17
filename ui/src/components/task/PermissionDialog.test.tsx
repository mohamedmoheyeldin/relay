// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, axeViolations } from "@/test/render";
import { PermissionDialogView } from "./PermissionDialog";
import type { ConfirmReq } from "@/store/app";

const req = (risk: ConfirmReq["risk"], at = Date.now()): ConfirmReq => ({ id: 5, run_id: 1, title: "Move 23 files to the Recycle Bin?", detail: "C:\\Users\\you\\Downloads", risk, at });

describe("PermissionDialog", () => {
  it("shows the request with risk wording and both choices", () => {
    renderUi(<PermissionDialogView req={req("destructive")} more={1} onAnswer={() => undefined} />);
    const dlg = screen.getByRole("alertdialog");
    expect(dlg).toHaveAccessibleName("Move 23 files to the Recycle Bin?");
    expect(screen.getByText(/may not be undoable/)).toBeInTheDocument();
    expect(screen.getByText("C:\\Users\\you\\Downloads")).toBeInTheDocument();
    expect(screen.getByText("1 more waiting")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Decline" })).toBeInTheDocument();
  });

  it("approves and declines by click", async () => {
    const onAnswer = vi.fn();
    renderUi(<PermissionDialogView req={req("reversible")} more={0} onAnswer={onAnswer} />);
    await userEvent.click(screen.getByRole("button", { name: "Decline" }));
    await userEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onAnswer.mock.calls).toEqual([
      [5, false],
      [5, true],
    ]);
  });

  it("Esc declines (never approves)", async () => {
    const onAnswer = vi.fn();
    renderUi(<PermissionDialogView req={req("destructive")} more={0} onAnswer={onAnswer} />);
    await userEvent.keyboard("{Escape}");
    expect(onAnswer).toHaveBeenCalledWith(5, false);
  });

  it("focuses Decline first for destructive actions", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    renderUi(<PermissionDialogView req={req("destructive")} more={0} onAnswer={() => undefined} />);
    await act(async () => {
      vi.advanceTimersByTime(60);
    });
    expect(screen.getByRole("button", { name: "Decline" })).toHaveFocus();
    vi.useRealTimers();
  });

  it("counts down from two minutes", () => {
    renderUi(<PermissionDialogView req={req("reversible", Date.now() - 30_000)} more={0} onAnswer={() => undefined} />);
    expect(screen.getByText("Declines in 1:30")).toBeInTheDocument();
  });

  it("has no serious accessibility violations", async () => {
    renderUi(<PermissionDialogView req={req("destructive")} more={0} onAnswer={() => undefined} />);
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
