// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi, useMockBackend, axeViolations } from "@/test/render";
import { useApp } from "@/store/app";
import * as nav from "@/lib/nav";
import { SearchPalette } from "./SearchPalette";

describe("SearchPalette", () => {
  beforeEach(() => {
    useMockBackend();
    useApp.setState({ searchOpen: true });
  });

  it("is a combobox with quick actions and recent chats", async () => {
    renderUi(<SearchPalette />);
    const box = screen.getByRole("combobox", { name: /Search chats, projects and commands/ });
    expect(box).toHaveFocus();
    expect(await screen.findByRole("option", { name: /Tidy up my Downloads folder/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /New chat/ })).toHaveAttribute("aria-selected", "true");
  });

  it("searches, moves with arrows and opens with Enter", async () => {
    const go = vi.spyOn(nav, "go");
    renderUi(<SearchPalette />);
    await userEvent.type(screen.getByRole("combobox"), "ssd");
    const hit = await screen.findByRole("option", { name: /Compare SSD prices/ });
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(hit).toHaveAttribute("aria-selected", "true"));
    expect(screen.getByRole("combobox")).toHaveAttribute("aria-activedescendant", hit.id);
    await userEvent.keyboard("{Enter}");
    expect(go).toHaveBeenCalledWith(expect.stringMatching(/^\/chat\//));
    expect(useApp.getState().searchOpen).toBe(false);
  });

  it("finds commands by keyword", async () => {
    renderUi(<SearchPalette />);
    await userEvent.type(screen.getByRole("combobox"), "privacy");
    expect(await screen.findByRole("option", { name: /AI & Privacy settings/ })).toBeInTheDocument();
  });

  it("Esc closes", async () => {
    renderUi(<SearchPalette />);
    await userEvent.keyboard("{Escape}");
    expect(useApp.getState().searchOpen).toBe(false);
  });

  it("has no serious accessibility violations", async () => {
    renderUi(<SearchPalette />);
    await screen.findByRole("option", { name: /Tidy up my Downloads folder/ });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});
