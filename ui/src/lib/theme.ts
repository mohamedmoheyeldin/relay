// Appearance: themes, accents and custom colours resolved to CSS variables. Pure; tested in theme.test.ts.

export type Mode = "system" | "light" | "dark";
export type ThemeName = "relay" | "midnight" | "graphite" | "paper";
export type AccentName = "blue" | "green" | "yellow" | "pink" | "orange" | "purple" | "gray";
export type Scheme = "light" | "dark";

export type Appearance = {
  mode: Mode;
  theme: ThemeName;
  accent: AccentName;
  /** Optional background / foreground overrides, per resolved scheme. */
  custom: Partial<Record<Scheme, { bg?: string; fg?: string }>>;
};

export const DEFAULT_APPEARANCE: Appearance = { mode: "dark", theme: "relay", accent: "blue", custom: {} };

type Palette = {
  bg: string;
  sidebar: string;
  surface: string; // composer, user bubble
  elevated: string; // menus, dialogs
  fg: string;
  muted: string;
  subtle: string;
};

export const THEMES: Record<ThemeName, { label: string; description: string; dark: Palette; light: Palette }> = {
  relay: {
    label: "Relay",
    description: "Soft charcoal, the default",
    dark: { bg: "#212121", sidebar: "#181818", surface: "#303030", elevated: "#353535", fg: "#ececec", muted: "#b4b4b4", subtle: "#8f8f8f" },
    light: { bg: "#ffffff", sidebar: "#f9f9f9", surface: "#f4f4f4", elevated: "#ffffff", fg: "#0d0d0d", muted: "#5d5d5d", subtle: "#8f8f8f" },
  },
  midnight: {
    label: "Midnight",
    description: "True black with crisp text",
    dark: { bg: "#000000", sidebar: "#000000", surface: "#1f1f1f", elevated: "#1c1c1c", fg: "#ededed", muted: "#ababab", subtle: "#7d7d7d" },
    light: { bg: "#ffffff", sidebar: "#ffffff", surface: "#f2f2f2", elevated: "#ffffff", fg: "#000000", muted: "#555555", subtle: "#8a8a8a" },
  },
  graphite: {
    label: "Graphite",
    description: "Cool blue-grey",
    dark: { bg: "#1b1d21", sidebar: "#141619", surface: "#2a2d33", elevated: "#2e3138", fg: "#e6e8ec", muted: "#a5aab4", subtle: "#7e8490" },
    light: { bg: "#f6f7f9", sidebar: "#eceef2", surface: "#e7eaef", elevated: "#ffffff", fg: "#15171b", muted: "#555b66", subtle: "#878d98" },
  },
  paper: {
    label: "Paper",
    description: "Warm and calm",
    dark: { bg: "#1f1d1a", sidebar: "#181613", surface: "#2f2c27", elevated: "#34312b", fg: "#ede7dc", muted: "#b6ae9f", subtle: "#8c8577" },
    light: { bg: "#faf8f3", sidebar: "#f1eee6", surface: "#eeeae0", elevated: "#fffdf8", fg: "#23201b", muted: "#6a6457", subtle: "#958e80" },
  },
};

export const ACCENTS: Record<AccentName, { label: string; dark: string; light: string }> = {
  blue: { label: "Blue", dark: "#4d8eff", light: "#1f6feb" },
  green: { label: "Green", dark: "#3fbf7f", light: "#17834f" },
  yellow: { label: "Yellow", dark: "#e6b422", light: "#946b00" },
  pink: { label: "Pink", dark: "#f2669b", light: "#c93670" },
  orange: { label: "Orange", dark: "#f5853f", light: "#c2530f" },
  purple: { label: "Purple", dark: "#a78bfa", light: "#7445d6" },
  gray: { label: "Gray", dark: "#a3a3a3", light: "#636363" },
};

export const FOLDER_COLORS: Record<string, string> = {
  green: "#3fae6a",
  red: "#e5534b",
  blue: "#4c8dff",
  yellow: "#d9a521",
  purple: "#a371f7",
  orange: "#ec7a35",
  pink: "#e2609a",
  gray: "#8e8e8e",
};
export const PROJECT_COLORS = Object.keys(FOLDER_COLORS);
export const folderColor = (c: string): string => FOLDER_COLORS[c] ?? FOLDER_COLORS.gray!;

// --- colour maths -------------------------------------------------------------------------

/** "#abc" / "abc" / "#aabbcc" -> "#aabbcc" (lowercase), or null if invalid. */
export function normalizeHex(input: string): string | null {
  let h = input.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(h)) h = h.split("").map((c) => c + c).join("");
  return /^[0-9a-f]{6}$/.test(h) ? `#${h}` : null;
}

export function hexToRgb(hex: string): [number, number, number] {
  const h = normalizeHex(hex) ?? "#000000";
  return [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
}

export function rgbToHex([r, g, b]: [number, number, number]): string {
  const c = (n: number) => Math.round(Math.min(255, Math.max(0, n))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

/** Linear mix: t=0 -> a, t=1 -> b. */
export function mix(a: string, b: string, t: number): string {
  const x = hexToRgb(a);
  const y = hexToRgb(b);
  return rgbToHex([x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t]);
}

export function luminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

export function alpha(hex: string, a: number): string {
  const [r, g, b] = hexToRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

// --- resolution ---------------------------------------------------------------------------

export function resolveScheme(mode: Mode, systemDark: boolean): Scheme {
  return mode === "system" ? (systemDark ? "dark" : "light") : mode;
}

export function basePalette(theme: ThemeName, scheme: Scheme): Palette {
  return (THEMES[theme] ?? THEMES.relay)[scheme];
}

/** Final palette after applying the user's background/foreground overrides. */
export function effectivePalette(a: Appearance, scheme: Scheme): Palette {
  const base = basePalette(a.theme, scheme);
  const o = a.custom[scheme] ?? {};
  const bg = o.bg ? normalizeHex(o.bg) : null;
  const fg = o.fg ? normalizeHex(o.fg) : null;
  if (!bg && !fg) return base;
  const B = bg ?? base.bg;
  const F = fg ?? base.fg;
  const dark = luminance(B) < 0.35;
  return {
    bg: B,
    sidebar: bg ? (dark ? (luminance(B) < 0.005 ? B : mix(B, "#000000", 0.28)) : mix(B, F, 0.025)) : base.sidebar,
    surface: mix(B, F, dark ? 0.1 : 0.045),
    elevated: dark ? mix(B, F, 0.12) : mix(B, "#ffffff", 0.6),
    fg: F,
    muted: mix(F, B, 0.3),
    subtle: mix(F, B, 0.47),
  };
}

/** CSS custom properties for the whole app. */
export function themeVars(a: Appearance, scheme: Scheme): Record<string, string> {
  const p = effectivePalette(a, scheme);
  const dark = scheme === "dark";
  const accent = (ACCENTS[a.accent] ?? ACCENTS.blue)[scheme];
  return {
    "--bg": p.bg,
    "--sidebar": p.sidebar,
    "--surface": p.surface,
    "--elevated": p.elevated,
    "--fg": p.fg,
    "--muted": p.muted,
    "--subtle": p.subtle,
    "--border": alpha(p.fg, dark ? 0.1 : 0.1),
    "--border-strong": alpha(p.fg, dark ? 0.16 : 0.15),
    "--hover": alpha(p.fg, dark ? 0.07 : 0.05),
    "--active": alpha(p.fg, dark ? 0.11 : 0.08),
    "--accent": accent,
    "--accent-soft": alpha(accent, dark ? 0.18 : 0.13),
    "--accent-fg": luminance(accent) > 0.45 ? "#111111" : "#ffffff",
    "--danger": dark ? "#f87171" : "#d92d20",
    "--danger-soft": dark ? "rgba(248, 113, 113, 0.12)" : "rgba(217, 45, 32, 0.08)",
    "--success": dark ? "#4ade80" : "#16a34a",
    "--send-bg": p.fg,
    "--send-fg": p.bg,
    "--code-bg": dark ? mix(p.bg, "#000000", 0.35) : mix(p.bg, p.fg, 0.035),
    "--code-head": dark ? mix(p.bg, p.fg, 0.07) : mix(p.bg, p.fg, 0.07),
    "--scrim": dark ? "rgba(0, 0, 0, 0.55)" : "rgba(0, 0, 0, 0.3)",
    "--shadow": dark
      ? "0 8px 28px rgba(0, 0, 0, 0.45), 0 0 0 1px rgba(255, 255, 255, 0.06)"
      : "0 8px 28px rgba(0, 0, 0, 0.12), 0 0 0 1px rgba(0, 0, 0, 0.06)",
  };
}

export function parseAppearance(raw: string | null): Appearance {
  if (!raw) return DEFAULT_APPEARANCE;
  try {
    const v = JSON.parse(raw) as Partial<Appearance>;
    return {
      mode: v.mode === "light" || v.mode === "dark" || v.mode === "system" ? v.mode : DEFAULT_APPEARANCE.mode,
      theme: v.theme && v.theme in THEMES ? v.theme : DEFAULT_APPEARANCE.theme,
      accent: v.accent && v.accent in ACCENTS ? v.accent : DEFAULT_APPEARANCE.accent,
      custom: v.custom && typeof v.custom === "object" ? v.custom : {},
    };
  } catch {
    return DEFAULT_APPEARANCE;
  }
}
