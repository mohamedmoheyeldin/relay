import { describe, expect, it } from "vitest";
import { matchesShortcut, shortcutLabel, shortcutParts, SHORTCUTS } from "./platform";

const key = (k: string, mods: Partial<Record<"ctrlKey" | "metaKey" | "shiftKey" | "altKey", boolean>> = {}) => ({
  key: k,
  code: k.length === 1 ? `Key${k.toUpperCase()}` : k,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

describe("shortcut labels", () => {
  it("use Ctrl on Windows and the Command symbol on macOS", () => {
    expect(shortcutLabel(["Mod", "K"], "windows")).toBe("Ctrl+K");
    expect(shortcutLabel(["Mod", "K"], "macos")).toBe("⌘K");
    expect(shortcutLabel(["Mod", "Shift", "H"], "macos")).toBe("⌘⇧H");
    expect(shortcutParts(["Mod", ","], "linux")).toEqual(["Ctrl", ","]);
  });
  it("every global shortcut is unique", () => {
    const labels = SHORTCUTS.map((s) => shortcutLabel(s.keys, "windows"));
    expect(new Set(labels).size).toBe(labels.length);
  });
});

describe("matching", () => {
  it("matches Mod to the platform key only", () => {
    expect(matchesShortcut(key("k", { ctrlKey: true }), ["Mod", "K"], "windows")).toBe(true);
    expect(matchesShortcut(key("k", { metaKey: true }), ["Mod", "K"], "windows")).toBe(false);
    expect(matchesShortcut(key("k", { metaKey: true }), ["Mod", "K"], "macos")).toBe(true);
    expect(matchesShortcut(key("K", { ctrlKey: true, shiftKey: true }), ["Mod", "K"], "windows")).toBe(false);
  });
  it("handles punctuation by code", () => {
    expect(matchesShortcut({ ...key(","), code: "Comma", ctrlKey: true }, ["Mod", ","], "windows")).toBe(true);
    expect(matchesShortcut({ ...key("?"), code: "Slash", ctrlKey: true }, ["Mod", "/"], "windows")).toBe(true);
  });
});
