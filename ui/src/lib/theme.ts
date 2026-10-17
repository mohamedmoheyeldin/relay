// Appearance: themes, accents and custom colours. The palettes themselves live in
// src/styles/tokens.css; this module holds labels, preview swatches (kept equal to the CSS by
// tokens.test.ts), colour maths and the derivation for custom background/foreground colours.

import { isCodeThemeId, RELAY_CODE_DARK, RELAY_CODE_LIGHT } from "./codeThemes";

export type Mode = "system" | "light" | "dark";
export type ThemeName = "relay" | "graphite" | "midnight";
export type AccentName = "terracotta" | "sage" | "indigo" | "plum" | "ink";
export type Scheme = "light" | "dark";
export type ProseFont = "sans" | "serif";

export type UiFont = "inter" | "system" | "atkinson";
export type TextSize = "small" | "medium" | "large";
export type Density = "comfortable" | "compact";
/** How much of a task the timeline shows: verdict only, the step list, or every detail and the raw log. */
export type TaskView = "simple" | "detailed" | "verbose";

export const SIDEBAR_MIN = 220;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 280;
export const CODE_FONT_MAX = 80;

export type Appearance = {
  mode: Mode;
  theme: ThemeName;
  accent: AccentName;
  /** Font for assistant answers. Chrome and tool output always use the sans face. */
  prose: ProseFont;
  /** Optional background / foreground overrides, per resolved scheme. */
  custom: Partial<Record<Scheme, { bg?: string; fg?: string }>>;
  /** Code theme ids (lib/codeThemes.ts) for light and dark mode. */
  codeThemeLight: string;
  codeThemeDark: string;
  /** A monospace font family name for code; "" = the bundled Geist Mono. */
  codeFont: string;
  uiFont: UiFont;
  textSize: TextSize;
  density: Density;
  taskView: TaskView;
  /** Expanded sidebar width in px (SIDEBAR_MIN..SIDEBAR_MAX). */
  sidebarWidth: number;
};

export const DEFAULT_APPEARANCE: Appearance = {
  mode: "system",
  theme: "relay",
  accent: "terracotta",
  prose: "sans",
  custom: {},
  codeThemeLight: RELAY_CODE_LIGHT,
  codeThemeDark: RELAY_CODE_DARK,
  codeFont: "",
  uiFont: "inter",
  textSize: "medium",
  density: "comfortable",
  taskView: "detailed",
  sidebarWidth: SIDEBAR_DEFAULT,
};

export const APPEARANCE_KEYS = Object.keys(DEFAULT_APPEARANCE) as (keyof Appearance)[];

export function clampSidebarWidth(w: number): number {
  if (!Number.isFinite(w)) return SIDEBAR_DEFAULT;
  return Math.round(Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, w)));
}

/** A font family name safe to put in CSS: no quotes, semicolons, braces or angle brackets. */
export function sanitizeFontName(v: string): string {
  return v
    .replace(/["'`;{}<>\\]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, CODE_FONT_MAX);
}

export type Swatch = { bg: string; sidebar: string; surface: string; fg: string; muted: string };

export const THEMES: Record<ThemeName, { label: string; description: string; light: Swatch; dark: Swatch }> = {
  relay: {
    label: "Relay",
    description: "Warm paper and charcoal, the default",
    light: { bg: "#faf7f2", sidebar: "#f1ede4", surface: "#fffefb", fg: "#1f1d1a", muted: "#6b665b" },
    dark: { bg: "#1c1a17", sidebar: "#151311", surface: "#262320", fg: "#f2ede4", muted: "#9a9385" },
  },
  graphite: {
    label: "Graphite",
    description: "Cool, neutral greys",
    light: { bg: "#f7f7f8", sidebar: "#eeeff1", surface: "#ffffff", fg: "#17181b", muted: "#62666e" },
    dark: { bg: "#18191c", sidebar: "#121315", surface: "#222327", fg: "#ecedef", muted: "#969aa3" },
  },
  midnight: {
    label: "Midnight",
    description: "True black, crisp white",
    light: { bg: "#ffffff", sidebar: "#f7f7f7", surface: "#ffffff", fg: "#0d0d0d", muted: "#666666" },
    dark: { bg: "#000000", sidebar: "#0a0a0a", surface: "#161616", fg: "#ededed", muted: "#9a9a9a" },
  },
};

export const ACCENTS: Record<AccentName, { label: string; light: string; dark: string }> = {
  terracotta: { label: "Terracotta", light: "#b4532f", dark: "#e58a67" },
  sage: { label: "Sage", light: "#4e6b3a", dark: "#9cc07e" },
  indigo: { label: "Indigo", light: "#4b5bb0", dark: "#9aa6ee" },
  plum: { label: "Plum", light: "#8a4a78", dark: "#d597c2" },
  ink: { label: "Ink", light: "#1f1d1a", dark: "#f2ede4" },
};

export const FOLDER_COLORS: Record<string, string> = {
  green: "#5c8a3e",
  red: "#c4553f",
  blue: "#4a77b8",
  yellow: "#c49a2c",
  purple: "#8a63b8",
  orange: "#d27a3b",
  pink: "#c25a8a",
  gray: "#8a857a",
};
export const PROJECT_COLORS = Object.keys(FOLDER_COLORS);
export const folderColor = (c: string): string => FOLDER_COLORS[c] ?? FOLDER_COLORS.gray!;

// --- colour maths -------------------------------------------------------------------------

/** "#abc" / "abc" / "#aabbcc" -> "#aabbcc" (lowercase), or null if invalid. */
export function normalizeHex(input: string): string | null {
  let h = input.trim().replace(/^#/, "").toLowerCase();
  if (/^[0-9a-f]{3}$/.test(h))
    h = h
      .split("")
      .map((c) => c + c)
      .join("");
  return /^[0-9a-f]{6}$/.test(h) ? `#${h}` : null;
}

export function hexToRgb(hex: string): [number, number, number] {
  const h = normalizeHex(hex) ?? "#000000";
  return [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
}

export function rgbToHex([r, g, b]: [number, number, number]): string {
  const c = (n: number) =>
    Math.round(Math.min(255, Math.max(0, n)))
      .toString(16)
      .padStart(2, "0");
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

/** Moves `fg` towards `bg` until it no longer meets `min` contrast, returning the last colour that does. */
export function softenTo(fg: string, bg: string, min: number, maxT = 0.6): string {
  let best = fg;
  for (let t = 0.05; t <= maxT; t += 0.05) {
    const c = mix(fg, bg, t);
    if (contrast(c, bg) < min) break;
    best = c;
  }
  return best;
}

// --- resolution ---------------------------------------------------------------------------

export function resolveScheme(mode: Mode, systemDark: boolean): Scheme {
  return mode === "system" ? (systemDark ? "dark" : "light") : mode;
}

/** CSS variables overriding the theme when the user picked their own background/foreground.
 *  Secondary and muted text are derived so they keep AA contrast against the chosen background. */
export function customVars(a: Appearance, scheme: Scheme): Record<string, string> | null {
  const o = a.custom[scheme] ?? {};
  const bg = o.bg ? normalizeHex(o.bg) : null;
  const fg = o.fg ? normalizeHex(o.fg) : null;
  if (!bg && !fg) return null;
  const base = THEMES[a.theme][scheme];
  const B = bg ?? base.bg;
  const F = fg ?? base.fg;
  const dark = luminance(B) < 0.2;
  const surface = mix(B, F, dark ? 0.06 : 0.0);
  return {
    "--relay-bg": B,
    "--relay-sidebar": dark ? mix(B, "#000000", 0.22) : mix(B, F, 0.035),
    "--relay-surface": dark ? surface : mix(B, "#ffffff", 0.6),
    "--relay-surface-raised": dark ? mix(B, F, 0.09) : mix(B, "#ffffff", 0.7),
    "--relay-surface-sunk": dark ? mix(B, "#000000", 0.3) : mix(B, F, 0.05),
    "--relay-text": F,
    "--relay-text-2": softenTo(F, B, 7),
    "--relay-text-muted": softenTo(F, B, 4.8),
    "--relay-border": mix(B, F, dark ? 0.12 : 0.1),
    "--relay-border-strong": softenTo(F, B, 3.1, 0.9),
    "--relay-hover": alpha(F, dark ? 0.06 : 0.05),
    "--relay-active": alpha(F, dark ? 0.1 : 0.09),
    "--relay-bg-translucent": alpha(B, dark ? 0.72 : 0.78),
    "--relay-sidebar-translucent": alpha(B, 0.5),
  };
}

/** Background/foreground in effect for a scheme (custom overrides, else the theme). */
export function effectiveSwatch(a: Appearance, scheme: Scheme): { bg: string; fg: string } {
  const base = THEMES[a.theme][scheme];
  const o = a.custom[scheme] ?? {};
  return { bg: (o.bg && normalizeHex(o.bg)) || base.bg, fg: (o.fg && normalizeHex(o.fg)) || base.fg };
}

export const CUSTOM_VAR_NAMES = [
  "--relay-bg",
  "--relay-sidebar",
  "--relay-surface",
  "--relay-surface-raised",
  "--relay-surface-sunk",
  "--relay-text",
  "--relay-text-2",
  "--relay-text-muted",
  "--relay-border",
  "--relay-border-strong",
  "--relay-hover",
  "--relay-active",
  "--relay-bg-translucent",
  "--relay-sidebar-translucent",
];

function oneOf<T extends string>(v: unknown, options: readonly T[], fallback: T): T {
  return typeof v === "string" && (options as readonly string[]).includes(v) ? (v as T) : fallback;
}

/** Reads stored appearance (localStorage cache or get_appearance) leniently: unknown or invalid
 *  values fall back to the defaults one field at a time, so a bad key never resets the rest. */
export function normalizeAppearance(input: unknown): Appearance {
  const v = (input && typeof input === "object" && !Array.isArray(input) ? input : {}) as Record<string, unknown>;
  const d = DEFAULT_APPEARANCE;
  const custom = v.custom && typeof v.custom === "object" && !Array.isArray(v.custom) ? (v.custom as Appearance["custom"]) : {};
  return {
    mode: oneOf(v.mode, ["system", "light", "dark"], d.mode),
    theme: oneOf(v.theme, Object.keys(THEMES) as ThemeName[], d.theme),
    accent: oneOf(v.accent, Object.keys(ACCENTS) as AccentName[], d.accent),
    prose: oneOf(v.prose, ["sans", "serif"], d.prose),
    custom,
    codeThemeLight: isCodeThemeId(v.codeThemeLight, "light") ? v.codeThemeLight : d.codeThemeLight,
    codeThemeDark: isCodeThemeId(v.codeThemeDark, "dark") ? v.codeThemeDark : d.codeThemeDark,
    codeFont: typeof v.codeFont === "string" ? sanitizeFontName(v.codeFont) : d.codeFont,
    uiFont: oneOf(v.uiFont, ["inter", "system", "atkinson"], d.uiFont),
    textSize: oneOf(v.textSize, ["small", "medium", "large"], d.textSize),
    density: oneOf(v.density, ["comfortable", "compact"], d.density),
    taskView: oneOf(v.taskView, ["simple", "detailed", "verbose"], d.taskView),
    sidebarWidth: typeof v.sidebarWidth === "number" ? clampSidebarWidth(v.sidebarWidth) : d.sidebarWidth,
  };
}

export function parseAppearance(raw: string | null): Appearance {
  if (!raw) return DEFAULT_APPEARANCE;
  try {
    return normalizeAppearance(JSON.parse(raw));
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

/** The keys whose value differs between `a` and `b`, with `b`'s values (a set_appearance patch). */
export function appearanceDiff(a: Appearance, b: Appearance): Partial<Appearance> {
  const out: Partial<Appearance> = {};
  for (const k of APPEARANCE_KEYS) {
    if (JSON.stringify(a[k]) !== JSON.stringify(b[k])) (out as Record<string, unknown>)[k] = b[k];
  }
  return out;
}
