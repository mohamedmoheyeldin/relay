// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, useMockBackend, axeViolations } from "@/test/render";
import type { Transport } from "@/lib/backend";
import { useApp } from "@/store/app";
import { Sidebar } from "./Sidebar";
import { ConfirmDialog } from "./Dialogs";

let t: Transport;

async function rowFor(title: string) {
  const btn = await screen.findByRole("button", { name: title });
  return btn.closest("[data-testid=chat-row]") as HTMLElement;
}

describe("Sidebar", () => {
  beforeEach(() => {
    t = useMockBackend();
    useApp.setState({ sidebarCollapsed: false, confirmDialog: null });
  });

  it("groups recents by date and shows pinned chats and projects", async () => {
    renderUi(<Sidebar />);
    expect(await screen.findByRole("region", { name: "Pinned" })).toHaveTextContent("Tidy up my Downloads folder");
    const recents = screen.getByRole("region", { name: "Recent chats" });
    expect(within(recents).getByRole("heading", { name: "Today" })).toBeInTheDocument();
    expect(within(recents).getByRole("heading", { name: "Yesterday" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Projects" })).toHaveTextContent("Home PC");
  });

  it("renames inline from the hover menu (optimistic)", async () => {
    renderUi(<Sidebar />);
    const row = await rowFor("Compare SSD prices");
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(within(row).getByRole("button", { name: "Options for Compare SSD prices" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /Rename/ }));
    const input = await screen.findByRole("textbox", { name: "Chat title" });
    await userEvent.clear(input);
    await userEvent.type(input, "SSD shortlist{Enter}");
    expect(await screen.findByRole("button", { name: "SSD shortlist" })).toBeInTheDocument();
    expect(calls).toHaveBeenCalledWith("conv_update", { id: expect.any(String), title: "SSD shortlist" });
  });

  it("pins from the menu and F2 renames the focused row", async () => {
    renderUi(<Sidebar />);
    const row = await rowFor("Find large videos");
    await userEvent.click(within(row).getByRole("button", { name: "Options for Find large videos" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: /^Pin$/ }));
    await waitFor(() => expect(screen.getByRole("region", { name: "Pinned" })).toHaveTextContent("Find large videos"));
    screen.getAllByRole("button", { name: "Find large videos" })[0]!.focus();
    await userEvent.keyboard("{F2}");
    expect(await screen.findByRole("textbox", { name: "Chat title" })).toHaveValue("Find large videos");
  });

  it("Delete asks for confirmation before deleting", async () => {
    renderUi(
      <>
        <Sidebar />
        <ConfirmDialog />
      </>,
    );
    const btn = await screen.findByRole("button", { name: "Organise screenshots folder" });
    btn.focus();
    await userEvent.keyboard("{Delete}");
    const dlg = await screen.findByRole("alertdialog");
    expect(dlg).toHaveTextContent("Delete chat?");
    const calls = vi.spyOn(t, "invoke");
    await userEvent.click(within(dlg).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Organise screenshots folder" })).toBeNull());
    expect(calls).toHaveBeenCalledWith("conv_delete", { id: expect.any(String) });
  });

  it("collapses to a rail and remembers it", async () => {
    renderUi(<Sidebar />);
    await screen.findByRole("region", { name: "Pinned" });
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(useApp.getState().sidebarCollapsed).toBe(true);
    expect(localStorage.getItem("relay.sidebarCollapsed")).toBe("true");
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
  });

  it("has no serious accessibility violations", async () => {
    const { container } = renderUi(<Sidebar />);
    await screen.findByRole("region", { name: "Pinned" });
    expect(await axeViolations(container)).toEqual([]);
  });
});
