// Appearance preferences (per PC, localStorage) applied to <html> as data attributes; tokens.css
// does the rest. Imported by main.tsx before the first render so there is no flash of the wrong theme.
import { create } from "zustand";
import {
  CUSTOM_VAR_NAMES,
  customVars,
  DEFAULT_APPEARANCE,
  parseAppearance,
  resolveScheme,
  type Appearance,
  type Scheme,
} from "../lib/theme";

const LS_KEY = "relay.appearance";

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
  set: (p: Partial<Appearance>) => void;
  setCustom: (key: "bg" | "fg", value: string | null) => void;
  resetCustom: () => void;
};

export function applyAppearance(a: Appearance): Scheme {
  const scheme = resolveScheme(a.mode, media?.matches ?? false);
  if (typeof document === "undefined") return scheme;
  const root = document.documentElement;
  root.dataset.mode = scheme;
  root.dataset.theme = a.theme;
  root.dataset.accent = a.accent;
  root.dataset.prose = a.prose;
  root.style.colorScheme = scheme;
  const vars = customVars(a, scheme);
  for (const name of CUSTOM_VAR_NAMES) {
    const v = vars?.[name];
    if (v) root.style.setProperty(name, v);
    else root.style.removeProperty(name);
  }
  return scheme;
}

function persist(a: Appearance) {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(a));
  } catch {
    /* ignore */
  }
}

const initial = load();

export const useAppearance = create<S>()((set, get) => ({
  appearance: initial,
  scheme: applyAppearance(initial),
  set: (p) => {
    const appearance = { ...get().appearance, ...p };
    persist(appearance);
    set({ appearance, scheme: applyAppearance(appearance) });
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
