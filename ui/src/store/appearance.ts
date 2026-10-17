// Appearance preferences, applied to <html> as data attributes and CSS variables; tokens.css and
// index.css do the rest, so every change shows at once without a reload. Imported by main.tsx
// before the first render so there is no flash of the wrong theme.
//
// Persistence: the backend keeps the source of truth (`get_appearance` / `set_appearance`, an
// opaque object in settings.json). localStorage is a cache for the first paint (public/boot.js and
// this module read it synchronously). `syncAppearance()` runs once at start-up: values saved in the
// backend win; settings that only exist in the local cache (from before the backend stored them)
// are uploaded once. Every change writes the cache at once and sends a debounced patch.
import { create } from "zustand";
import { api, errorText } from "../lib/backend";
import { codeTheme } from "../lib/codeThemes";
import {
  appearanceDiff,
  APPEARANCE_KEYS,
  clampSidebarWidth,
  CUSTOM_VAR_NAMES,
  customVars,
  DEFAULT_APPEARANCE,
  normalizeAppearance,
  parseAppearance,
  resolveScheme,
  sanitizeFontName,
  type Appearance,
  type Scheme,
} from "../lib/theme";

export const LS_KEY = "relay.appearance";
const SAVE_DELAY_MS = 300;

function load(): Appearance {
  try {
    return parseAppearance(localStorage.getItem(LS_KEY));
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

const media = typeof window !== "undefined" && typeof window.matchMedia === "function" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

type S = {
  appearance: Appearance;
  scheme: Scheme;
  /** "failed" when the backend could not be read or written (the local cache still applies). */
  sync: "idle" | "loading" | "ready" | "failed";
  set: (p: Partial<Appearance>) => void;
  setCustom: (key: "bg" | "fg", value: string | null) => void;
  resetCustom: () => void;
};

/** Sets the sidebar width CSS variable without saving (used while dragging the resize handle). */
export function previewSidebarWidth(px: number) {
  if (typeof document === "undefined") return;
  document.documentElement.style.setProperty("--relay-sidebar-w", `${clampSidebarWidth(px)}px`);
}

export function applyAppearance(a: Appearance): Scheme {
  const scheme = resolveScheme(a.mode, media?.matches ?? false);
  if (typeof document === "undefined") return scheme;
  const root = document.documentElement;
  root.dataset.mode = scheme;
  root.dataset.theme = a.theme;
  root.dataset.accent = a.accent;
  root.dataset.prose = a.prose;
  root.dataset.uiFont = a.uiFont;
  root.dataset.textSize = a.textSize;
  root.dataset.density = a.density;
  root.style.colorScheme = scheme;
  const vars = customVars(a, scheme);
  for (const name of CUSTOM_VAR_NAMES) {
    const v = vars?.[name];
    if (v) root.style.setProperty(name, v);
    else root.style.removeProperty(name);
  }
  root.style.setProperty("--relay-sidebar-w", `${clampSidebarWidth(a.sidebarWidth)}px`);
  // Code: the chosen theme's own background and text colour (Relay's own theme keeps the tokens).
  const ct = codeTheme(scheme === "light" ? a.codeThemeLight : a.codeThemeDark, scheme);
  if (ct.bg && ct.fg) {
    root.style.setProperty("--relay-code-bg", ct.bg);
    root.style.setProperty("--relay-code-fg", ct.fg);
  } else {
    root.style.removeProperty("--relay-code-bg");
    root.style.removeProperty("--relay-code-fg");
  }
  const font = sanitizeFontName(a.codeFont);
  if (font) root.style.setProperty("--relay-font-code", `"${font}", var(--relay-font-mono)`);
  else root.style.removeProperty("--relay-font-code");
  return scheme;
}

function cache(a: Appearance) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(a));
  } catch {
    /* storage unavailable: the backend copy still applies on the next start */
  }
}

// ---- backend persistence ------------------------------------------------------------------

let pending: Partial<Appearance> = {};
let timer: ReturnType<typeof setTimeout> | null = null;
let hydrated = false;

function reportSaveError(e: unknown) {
  console.warn(`[relay] could not save appearance: ${errorText(e)}`);
  useAppearance.setState({ sync: "failed" });
  void import("./app").then((m) => m.useApp.getState().toast("Couldn't save your appearance settings. They still apply on this PC.", "error"));
}

/** Sends the queued patch now (also used by tests). */
export async function flushAppearance(): Promise<void> {
  if (timer) clearTimeout(timer);
  timer = null;
  if (!hydrated) return; // sent after syncAppearance has merged the backend copy
  const patch = pending;
  pending = {};
  if (Object.keys(patch).length === 0) return;
  try {
    await api.setAppearance(patch);
    useAppearance.setState({ sync: "ready" });
  } catch (e) {
    reportSaveError(e);
  }
}

function queueSave(p: Partial<Appearance>) {
  pending = { ...pending, ...p };
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void flushAppearance(), SAVE_DELAY_MS);
}

/** Reads the backend copy once and reconciles it with the local cache (see the header). */
export async function syncAppearance(): Promise<void> {
  useAppearance.setState({ sync: "loading" });
  let remote: Record<string, unknown>;
  try {
    remote = await api.getAppearance();
  } catch (e) {
    console.warn(`[relay] could not load appearance: ${errorText(e)}`);
    hydrated = true;
    useAppearance.setState({ sync: "failed" });
    return;
  }
  const local = useAppearance.getState().appearance;
  const known = APPEARANCE_KEYS.filter((k) => k in remote);
  // Backend values win; keys it lacks keep the local value. Changes made while loading win over both.
  const merged = normalizeAppearance({ ...local, ...Object.fromEntries(known.map((k) => [k, remote[k]])), ...pending });
  const upload: Partial<Appearance> = {};
  for (const k of APPEARANCE_KEYS) {
    if (!(k in remote) && JSON.stringify(merged[k]) !== JSON.stringify(DEFAULT_APPEARANCE[k])) (upload as Record<string, unknown>)[k] = merged[k];
  }
  hydrated = true;
  cache(merged);
  useAppearance.setState({ appearance: merged, scheme: applyAppearance(merged), sync: "ready" });
  pending = { ...upload, ...pending };
  await flushAppearance();
}

/** Test hook: forget the backend sync state. */
export function resetAppearanceSync() {
  if (timer) clearTimeout(timer);
  timer = null;
  pending = {};
  hydrated = false;
}

const initial = load();

export const useAppearance = create<S>()((set, get) => ({
  appearance: initial,
  scheme: applyAppearance(initial),
  sync: "idle",
  set: (p) => {
    const prev = get().appearance;
    const appearance = normalizeAppearance({ ...prev, ...p });
    const diff = appearanceDiff(prev, appearance);
    cache(appearance);
    set({ appearance, scheme: applyAppearance(appearance) });
    if (Object.keys(diff).length) queueSave(diff);
  },
  setCustom: (key, value) => {
    const { appearance, scheme } = get();
    const cur = { ...(appearance.custom[scheme] ?? {}) };
    if (value) cur[key] = value;
    else delete cur[key];
    get().set({ custom: { ...appearance.custom, [scheme]: cur } });
  },
  resetCustom: () => {
    const { appearance, scheme } = get();
    const custom = { ...appearance.custom };
    delete custom[scheme];
    get().set({ custom });
  },
}));

media?.addEventListener("change", () => {
  const { appearance } = useAppearance.getState();
  if (appearance.mode === "system") useAppearance.setState({ scheme: applyAppearance(appearance) });
});
