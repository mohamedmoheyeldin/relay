// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, useMockBackend, axeViolations } from "@/test/render";
import { queryClient, qk } from "@/lib/query";
import type { Hybrid } from "@/lib/types";
import type { Transport } from "@/lib/backend";
import { ContextControls } from "./ContextControls";

let t: Transport;
const spy = () => vi.spyOn(t, "invoke");

describe("privacy selector and provider picker", () => {
  beforeEach(() => {
    t = useMockBackend("codex-missing");
  });

  it("shows skeletons first, then the current privacy mode and provider", async () => {
    const { container } = renderUi(<ContextControls />);
    expect(container.querySelector(".skeleton")).not.toBeNull();
    const hybrid = await screen.findByRole("radio", { name: /Hybrid/ });
    expect(hybrid).toHaveAttribute("aria-checked", "true");
    expect(screen.getByTestId("provider-picker")).toHaveAccessibleName(/Claude Code/);
  });

  it("switching privacy mode calls set_hybrid at once (optimistic)", async () => {
    renderUi(<ContextControls />);
    const local = await screen.findByRole("radio", { name: /Local only/ });
    const calls = spy();
    await userEvent.click(local);
    expect(local).toHaveAttribute("aria-checked", "true");
    expect(calls).toHaveBeenCalledWith("set_hybrid", { privacyMode: "local_only" });
    await waitFor(() => expect(queryClient.getQueryData<Hybrid>(qk.hybrid)?.privacy_mode).toBe("local_only"));
  });

  it("lists providers with install/sign-in state and refuses unavailable ones", async () => {
    renderUi(<ContextControls />);
    await userEvent.click(await screen.findByTestId("provider-picker"));
    const codex = await screen.findByTestId("provider-option-codex");
    expect(codex).toHaveAttribute("aria-disabled", "true");
    expect(codex).toHaveTextContent("Not installed");
    const calls = spy();
    await userEvent.click(codex);
    expect(calls).not.toHaveBeenCalledWith("set_hybrid", expect.anything());
    expect(screen.getByRole("menuitem", { name: /AI & Privacy settings/ })).toBeInTheDocument();
  });

  it("switches provider when it is ready", async () => {
    t = useMockBackend("all-ready");
    renderUi(<ContextControls />);
    await userEvent.click(await screen.findByTestId("provider-picker"));
    const calls = spy();
    await userEvent.click(await screen.findByTestId("provider-option-codex"));
    expect(calls).toHaveBeenCalledWith("set_hybrid", { defaultProvider: "codex" });
    await waitFor(() => expect(screen.getByTestId("provider-picker")).toHaveAccessibleName(/Codex/));
  });

  it("has no serious accessibility violations", async () => {
    const { container } = renderUi(<ContextControls />);
    await screen.findByRole("radio", { name: /Hybrid/ });
    expect(await axeViolations(container)).toEqual([]);
  });
});
