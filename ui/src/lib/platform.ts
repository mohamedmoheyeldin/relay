// Platform awareness: window materials, title bar and shortcut labels.
// The first paint uses a guess from the user agent; get_platform replaces it when it answers.
import type { Platform } from "./schemas";

export type Os = Platform["os"];

export function guessOs(): Os {
  if (typeof navigator === "undefined") return "windows";
  const p = `${navigator.platform} ${navigator.userAgent}`;
  if (/Mac|iPhone|iPad/i.test(p)) return "macos";
  if (/Linux|X11/i.test(p) && !/Windows/i.test(p)) return "linux";
  return "windows";
}

export const FALLBACK_PLATFORM: Platform = { os: guessOs(), materials: "none", title_bar: "native", version: "" };

/** Applies materials and title-bar layout to <html> (tokens.css reacts to these attributes). */
export function applyPlatform(p: Platform) {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.dataset.os = p.os;
  root.dataset.material = p.materials;
  root.dataset.titlebar = p.title_bar;
}

/** A shortcut as a list of keys; "Mod" is Ctrl on Windows/Linux and Command on macOS. */
export type Keys = readonly string[];

const MAC_SYMBOL: Record<string, string> = { Mod: "⌘", Shift: "⇧", Alt: "⌥", Ctrl: "⌃", Enter: "↩", Esc: "Esc", Backspace: "⌫" };

/** Display parts for a shortcut, one per key (rendered as separate <kbd> elements). */
export function shortcutParts(keys: Keys, os: Os): string[] {
  return keys.map((k) => (os === "macos" ? (MAC_SYMBOL[k] ?? k) : k === "Mod" ? "Ctrl" : k));
}

/** Single-string label: "Ctrl+K" on Windows, "⌘K" on macOS. */
export function shortcutLabel(keys: Keys, os: Os): string {
  const parts = shortcutParts(keys, os);
  return os === "macos" ? parts.join("") : parts.join("+");
}

/** True when a keyboard event matches the shortcut (Mod = Ctrl or Command depending on the OS). */
export function matchesShortcut(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">, keys: Keys, os: Os): boolean {
  const want = new Set(keys);
  const mod = os === "macos" ? e.metaKey : e.ctrlKey;
  if (want.has("Mod") !== mod) return false;
  if (want.has("Shift") !== e.shiftKey) return false;
  if (want.has("Alt") !== e.altKey) return false;
  const key = keys.find((k) => !["Mod", "Shift", "Alt", "Ctrl"].includes(k));
  if (!key) return false;
  const k = key.toLowerCase();
  if (k === ",") return e.key === "," || e.code === "Comma";
  if (k === "/") return e.key === "/" || e.code === "Slash";
  if (k === "\\") return e.key === "\\" || e.code === "Backslash";
  return e.key.toLowerCase() === k || e.code.toLowerCase() === `key${k}`;
}

export type ShortcutId = "search" | "newChat" | "toggleSidebar" | "settings" | "shortcuts" | "tasks" | "focusComposer";

export const SHORTCUTS: { id: ShortcutId; label: string; keys: Keys }[] = [
  { id: "search", label: "Search chats and commands", keys: ["Mod", "K"] },
  { id: "newChat", label: "New chat", keys: ["Mod", "N"] },
  { id: "toggleSidebar", label: "Show or hide the sidebar", keys: ["Mod", "B"] },
  { id: "tasks", label: "Task history", keys: ["Mod", "Shift", "H"] },
  { id: "settings", label: "Settings", keys: ["Mod", ","] },
  { id: "shortcuts", label: "Keyboard shortcuts", keys: ["Mod", "/"] },
  { id: "focusComposer", label: "Focus the message box", keys: ["Shift", "Esc"] },
];

/** Keys that are not global handlers but are worth listing. */
export const OTHER_KEYS: { label: string; keys: Keys }[] = [
  { label: "Send message", keys: ["Enter"] },
  { label: "New line", keys: ["Shift", "Enter"] },
  { label: "Stop the running task (in the message box)", keys: ["Esc"] },
  { label: "Close a dialog, menu or settings", keys: ["Esc"] },
  { label: "Move through lists and menus", keys: ["↑", "↓"] },
];

export function shortcutKeys(id: ShortcutId): Keys {
  return SHORTCUTS.find((s) => s.id === id)!.keys;
}
