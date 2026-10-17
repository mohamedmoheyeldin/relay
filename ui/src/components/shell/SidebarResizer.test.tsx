// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderUi } from "@/test/render";
import { DEFAULT_APPEARANCE, SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN } from "@/lib/theme";
import { useAppearance } from "@/store/appearance";
import { SidebarResizer } from "./SidebarResizer";

const width = () => useAppearance.getState().appearance.sidebarWidth;
const cssWidth = () => document.documentElement.style.getPropertyValue("--relay-sidebar-w");

describe("sidebar resize handle", () => {
  beforeEach(() => {
    localStorage.clear();
    useAppearance.setState({ appearance: DEFAULT_APPEARANCE });
    useAppearance.getState().set({ sidebarWidth: SIDEBAR_DEFAULT });
  });

  it("is a focusable vertical separator that announces its width", () => {
    renderUi(<SidebarResizer />);
    const h = screen.getByRole("separator", { name: "Resize sidebar" });
    expect(h).toHaveAttribute("aria-orientation", "vertical");
    expect(h).toHaveAttribute("aria-valuenow", "280");
    expect(h).toHaveAttribute("aria-valuemin", String(SIDEBAR_MIN));
    expect(h).toHaveAttribute("aria-valuemax", String(SIDEBAR_MAX));
    expect(h).toHaveAttribute("tabindex", "0");
  });

  it("resizes with the arrow keys, Home/End and resets with Enter", async () => {
    renderUi(<SidebarResizer />);
    const h = screen.getByRole("separator", { name: "Resize sidebar" });
    h.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(width()).toBe(296);
    await userEvent.keyboard("{Shift>}{ArrowLeft}{/Shift}");
    expect(width()).toBe(248);
    expect(h).toHaveAttribute("aria-valuenow", "248");
    expect(cssWidth()).toBe("248px");
    await userEvent.keyboard("{Home}");
    expect(width()).toBe(SIDEBAR_MIN);
    await userEvent.keyboard("{ArrowLeft}");
    expect(width()).toBe(SIDEBAR_MIN); // clamped
    await userEvent.keyboard("{End}");
    expect(width()).toBe(SIDEBAR_MAX);
    await userEvent.keyboard("{Enter}");
    expect(width()).toBe(SIDEBAR_DEFAULT);
  });

  it("drags live (CSS only) and saves on release, within the limits", () => {
    renderUi(<SidebarResizer />);
    const h = screen.getByRole("separator", { name: "Resize sidebar" });
    fireEvent.pointerDown(h, { button: 0, clientX: 280, pointerId: 1 });
    fireEvent.pointerMove(h, { clientX: 340, pointerId: 1 });
    expect(cssWidth()).toBe("340px");
    expect(width()).toBe(SIDEBAR_DEFAULT); // not saved while dragging
    fireEvent.pointerMove(h, { clientX: 2000, pointerId: 1 });
    expect(cssWidth()).toBe(`${SIDEBAR_MAX}px`);
    fireEvent.pointerUp(h, { clientX: 2000, pointerId: 1 });
    expect(width()).toBe(SIDEBAR_MAX);
    expect(JSON.parse(localStorage.getItem("relay.appearance")!).sidebarWidth).toBe(SIDEBAR_MAX);
  });

  it("double-click resets to the default width", () => {
    useAppearance.getState().set({ sidebarWidth: 400 });
    renderUi(<SidebarResizer />);
    fireEvent.doubleClick(screen.getByRole("separator", { name: "Resize sidebar" }));
    expect(width()).toBe(SIDEBAR_DEFAULT);
  });
});
