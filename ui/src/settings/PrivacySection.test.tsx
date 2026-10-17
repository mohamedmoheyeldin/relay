// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, useMockBackend, axeViolations } from "@/test/render";
import type { Transport } from "@/lib/backend";
import { PrivacySection } from "./PrivacySection";

let t: Transport;

describe("Settings › AI & Privacy", () => {
  beforeEach(() => {
    t = useMockBackend("codex-missing");
  });

  it("explains privacy modes as radio cards and saves the choice", async () => {
    renderUi(<PrivacySection />);
    const local = await screen.findByRole("radio", { name: "Local only" });
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(local);
    expect(calls).toHaveBeenCalledWith("set_hybrid", { privacyMode: "local_only" });
    expect(local).toHaveAttribute("aria-checked", "true");
  });

  it("shows provider status with the exact install and sign-in commands", async () => {
    renderUi(<PrivacySection />);
    const codex = await screen.findByTestId("provider-card-codex");
    expect(codex).toHaveTextContent("Not installed");
    expect(codex).toHaveTextContent("npm install -g @openai/codex");
    expect(codex).toHaveTextContent("codex login");
    const claude = screen.getByTestId("provider-card-claude");
    expect(claude).toHaveTextContent("Yes, version 2.4.1");
    expect(within(claude).getByText("Ready")).toBeInTheDocument();
  });

  it("refresh re-probes providers and shows skeletons meanwhile", async () => {
    renderUi(<PrivacySection />);
    await screen.findByTestId("provider-card-claude");
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(screen.getByRole("button", { name: "Refresh provider status" }));
    expect(calls).toHaveBeenCalledWith("refresh_providers", undefined);
    expect(screen.queryByTestId("provider-card-claude")).toBeNull();
    expect(await screen.findByTestId("provider-card-claude", {}, { timeout: 3000 })).toBeInTheDocument();
  });

  it("context sharing switches save individually", async () => {
    renderUi(<PrivacySection />);
    const sw = await screen.findByRole("switch", { name: "Screenshots" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(sw);
    expect(calls).toHaveBeenCalledWith("set_hybrid", { share: { screenshots: true } });
    expect(sw).toHaveAttribute("aria-checked", "true");
  });

  it("validates and adds allowed folders, lists protected ones read-only", async () => {
    renderUi(<PrivacySection />);
    const input = await screen.findByLabelText("Folder to allow");
    await userEvent.type(input, "Documents{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Type a full path");
    const calls = vi.spyOn(t, "invoke");
    await userEvent.clear(input);
    await userEvent.type(input, "D:\\Photos{Enter}");
    expect(calls).toHaveBeenCalledWith("set_hybrid", { allowedRoots: expect.arrayContaining(["D:\\Photos"]) });
    expect(await screen.findByText("D:\\Photos")).toBeInTheDocument();
    expect(screen.getByText("C:\\Windows")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove C:\\Windows" })).toBeNull();
  });

  it("the tool allowlist toggles a tool off", async () => {
    renderUi(<PrivacySection />);
    const sw = await screen.findByRole("switch", { name: "Allow Delete files permanently" });
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(sw);
    await waitFor(() => expect(calls).toHaveBeenCalledWith("set_hybrid", { disabledTools: expect.arrayContaining(["delete_files", "run_powershell"]) }));
  });

  it("has no serious accessibility violations", async () => {
    const { container } = renderUi(<PrivacySection />);
    await screen.findByTestId("provider-card-codex");
    expect(await axeViolations(container)).toEqual([]);
  });
});
