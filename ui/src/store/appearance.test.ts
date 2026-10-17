// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setTransport, type Transport } from "@/lib/backend";
import { DEFAULT_APPEARANCE, SIDEBAR_MAX, SIDEBAR_MIN } from "@/lib/theme";
import { flushAppearance, LS_KEY, resetAppearanceSync, syncAppearance, useAppearance } from "./appearance";

/** A fake backend that only knows the appearance commands and records every call. */
function fakeBackend(initial: Record<string, unknown> = {}, opts: { fail?: boolean } = {}) {
  const stored: Record<string, unknown> = { ...initial };
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  const t: Transport = {
    kind: "mock",
    async invoke(cmd, args) {
      calls.push({ cmd, args });
      if (opts.fail) throw new Error("backend down");
      if (cmd === "get_appearance") return { ...stored };
      if (cmd === "set_appearance") {
        const patch = args?.patch as Record<string, unknown>;
        for (const [k, v] of Object.entries(patch)) {
          if (v === null) delete stored[k];
          else stored[k] = v;
        }
        return { ...stored };
      }
      throw new Error(`unexpected ${cmd}`);
    },
    async listen() {
      return () => undefined;
    },
  };
  setTransport(t);
  return { calls, stored: () => stored };
}

const root = () => document.documentElement;

describe("appearance store", () => {
  beforeEach(() => {
    localStorage.clear();
    resetAppearanceSync();
    useAppearance.setState({ appearance: DEFAULT_APPEARANCE, sync: "idle" });
    useAppearance.getState().set({});
  });
  afterEach(() => {
    vi.useRealTimers();
    setTransport(null);
  });

  it("applies every option to <html> at once (no reload)", () => {
    const { set } = useAppearance.getState();
    set({ uiFont: "atkinson", textSize: "large", density: "compact", sidebarWidth: 333, codeFont: "JetBrains Mono" });
    expect(root().dataset.uiFont).toBe("atkinson");
    expect(root().dataset.textSize).toBe("large");
    expect(root().dataset.density).toBe("compact");
    expect(root().style.getPropertyValue("--relay-sidebar-w")).toBe("333px");
    expect(root().style.getPropertyValue("--relay-font-code")).toBe('"JetBrains Mono", var(--relay-font-mono)');
    set({ codeFont: "" });
    expect(root().style.getPropertyValue("--relay-font-code")).toBe("");
  });

  it("uses the chosen code theme's colours for the current scheme, Relay's own tokens otherwise", () => {
    const { set } = useAppearance.getState();
    set({ mode: "dark", codeThemeDark: "dracula", codeThemeLight: "solarized-light" });
    expect(root().dataset.mode).toBe("dark");
    expect(root().style.getPropertyValue("--relay-code-bg")).toBe("#282a36");
    set({ mode: "light" });
    expect(root().style.getPropertyValue("--relay-code-bg")).toBe("#fdf6e3");
    set({ codeThemeLight: "relay-light" });
    expect(root().style.getPropertyValue("--relay-code-bg")).toBe("");
  });

  it("rejects invalid values field by field and clamps the sidebar width", () => {
    const { set } = useAppearance.getState();
    set({ sidebarWidth: 9999 });
    expect(useAppearance.getState().appearance.sidebarWidth).toBe(SIDEBAR_MAX);
    set({ sidebarWidth: 10 });
    expect(useAppearance.getState().appearance.sidebarWidth).toBe(SIDEBAR_MIN);
    // A dark theme id is not a valid light theme; the font name loses characters CSS would choke on.
    set({ codeThemeLight: "dracula", codeFont: 'Fira Code"; color: red' } as never);
    expect(useAppearance.getState().appearance.codeThemeLight).toBe("relay-light");
    expect(useAppearance.getState().appearance.codeFont).toBe("Fira Code color: red");
  });

  it("caches every change in localStorage for the first paint", () => {
    useAppearance.getState().set({ taskView: "verbose" });
    expect(JSON.parse(localStorage.getItem(LS_KEY)!).taskView).toBe("verbose");
  });

  it("sends only the changed keys, debounced, once the backend copy is loaded", async () => {
    const be = fakeBackend();
    await syncAppearance();
    vi.useFakeTimers();
    const { set } = useAppearance.getState();
    set({ textSize: "small" });
    set({ textSize: "large", density: "compact" });
    set({ density: "compact" }); // no change: nothing new to send
    expect(be.calls.filter((c) => c.cmd === "set_appearance")).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(400);
    const sets = be.calls.filter((c) => c.cmd === "set_appearance");
    expect(sets).toHaveLength(1);
    expect(sets[0]!.args).toEqual({ patch: { textSize: "large", density: "compact" } });
    expect(be.stored()).toMatchObject({ textSize: "large", density: "compact" });
  });

  it("backend values win over the local cache; local-only settings are uploaded once", async () => {
    // The local cache from before the backend stored appearance (loaded at start, not changed now).
    useAppearance.setState({ appearance: { ...DEFAULT_APPEARANCE, accent: "sage", taskView: "simple" } });
    const be = fakeBackend({ taskView: "verbose", codeThemeDark: "monokai", bogus: 1, textSize: "huge" });
    await syncAppearance();
    const a = useAppearance.getState().appearance;
    expect(a.taskView).toBe("verbose"); // backend wins
    expect(a.codeThemeDark).toBe("monokai");
    expect(a.textSize).toBe("medium"); // invalid backend value -> default
    expect(a.accent).toBe("sage"); // only known locally -> kept and uploaded
    expect(useAppearance.getState().sync).toBe("ready");
    const sets = be.calls.filter((c) => c.cmd === "set_appearance");
    expect(sets).toHaveLength(1);
    expect(sets[0]!.args!.patch).toEqual({ accent: "sage" });
    expect(JSON.parse(localStorage.getItem(LS_KEY)!).codeThemeDark).toBe("monokai");
  });

  it("keeps working from the local cache when the backend fails", async () => {
    fakeBackend({}, { fail: true });
    await syncAppearance();
    expect(useAppearance.getState().sync).toBe("failed");
    useAppearance.getState().set({ density: "compact" });
    expect(root().dataset.density).toBe("compact");
    await flushAppearance();
    expect(useAppearance.getState().sync).toBe("failed");
  });
});
