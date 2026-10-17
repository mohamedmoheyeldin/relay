// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, useMockBackend } from "@/test/render";
import { useApp } from "@/store/app";
import { Composer } from "./Composer";

describe("Composer", () => {
  const send = vi.fn(async () => undefined);
  const stop = vi.fn();
  const original = { send: useApp.getState().send, stop: useApp.getState().stop };
  beforeEach(() => {
    useMockBackend();
    useApp.setState({ send, stop, activeRun: null, sending: null });
    send.mockClear();
    stop.mockClear();
  });
  afterEach(() => useApp.setState({ ...original, activeRun: null, sending: null }));

  it("sends with Enter and keeps Shift+Enter for new lines", async () => {
    renderUi(<Composer convId="c1" />);
    const box = screen.getByRole("textbox", { name: "Message Relay" });
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
    await userEvent.type(box, "line one{Shift>}{Enter}{/Shift}line two");
    expect(box).toHaveValue("line one\nline two");
    await userEvent.keyboard("{Enter}");
    expect(send).toHaveBeenCalledWith("line one\nline two", [], { projectId: null, convId: "c1" });
    expect(box).toHaveValue("");
  });

  it("attaches files through pick_files and can remove them", async () => {
    renderUi(<Composer />);
    await userEvent.click(screen.getByRole("button", { name: "Attach files" }));
    const list = await screen.findByRole("list", { name: "Attached files" });
    expect(list).toHaveTextContent("budget-2026.xlsx");
    await userEvent.click(screen.getByRole("button", { name: "Remove budget-2026.xlsx" }));
    expect(list).not.toHaveTextContent("budget-2026.xlsx");
  });

  it("turns into a Stop button while a task runs, and Esc stops", async () => {
    useApp.setState({ activeRun: { run_id: 1, conv_id: "c1", started_at: Date.now() } });
    renderUi(<Composer convId="c1" />);
    expect(screen.queryByRole("button", { name: "Send message" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Stop the task" }));
    expect(stop).toHaveBeenCalledTimes(1);
    screen.getByRole("textbox", { name: "Message Relay" }).focus();
    await userEvent.keyboard("{Escape}");
    expect(stop).toHaveBeenCalledTimes(2);
  });
});
