import { create } from "zustand";
import {
  DEFAULT_APPEARANCE,
  parseAppearance,
  resolveScheme,
  themeVars,
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

const media = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

type S = {
  appearance: Appearance;
  scheme: Scheme;
  set: (p: Partial<Appearance>) => void;
  setCustom: (key: "bg" | "fg", value: string | null) => void;
  resetCustom: () => void;
};

function apply(a: Appearance): Scheme {
  const scheme = resolveScheme(a.mode, media?.matches ?? true);
  const root = document.documentElement;
  const vars = themeVars(a, scheme);
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  root.dataset.mode = scheme;
  root.dataset.theme = a.theme;
  root.style.colorScheme = scheme;
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
  scheme: typeof document !== "undefined" ? apply(initial) : "dark",
  set: (p) => {
    const appearance = { ...get().appearance, ...p };
    persist(appearance);
    set({ appearance, scheme: apply(appearance) });
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
  if (appearance.mode === "system") useAppearance.setState({ scheme: apply(appearance) });
});
