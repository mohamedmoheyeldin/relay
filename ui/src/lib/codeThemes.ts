// Code themes for highlighted code blocks. Every theme except Relay's own ships with shiki (see the
// version in package.json) and is loaded lazily by lib/highlight.ts the first time it is used.
// `bg`/`fg` are the theme's editor colours as shiki normalises them (codeThemes.test.ts keeps them
// equal to the theme files), so the code block background can switch instantly, before the theme
// itself has loaded. "Relay" keeps Relay's own code surface (--relay-code-bg) and syntax colours.

export type CodeThemeScheme = "light" | "dark";
export type CodeTheme = {
  id: string;
  label: string;
  scheme: CodeThemeScheme;
  /** shiki theme name used for the syntax colours. */
  shiki: string;
  /** Editor background / foreground; null = Relay's own tokens. */
  bg: string | null;
  fg: string | null;
};

export const RELAY_CODE_LIGHT = "relay-light";
export const RELAY_CODE_DARK = "relay-dark";

const t = (id: string, label: string, scheme: CodeThemeScheme, bg: string | null, fg: string | null, shiki = id): CodeTheme => ({ id, label, scheme, shiki, bg, fg });

export const CODE_THEMES: CodeTheme[] = [
  t(RELAY_CODE_LIGHT, "Relay", "light", null, null, "github-light-default"),
  t("github-light", "GitHub Light", "light", "#fff", "#24292e"),
  t("one-light", "One Light", "light", "#fafafa", "#383a42"),
  t("catppuccin-latte", "Catppuccin Latte", "light", "#eff1f5", "#4c4f69"),
  t("solarized-light", "Solarized Light", "light", "#fdf6e3", "#657b83"),
  t("vitesse-light", "Vitesse Light", "light", "#ffffff", "#393a34"),
  t("min-light", "Min Light", "light", "#ffffff", "#24292eff"),
  t("rose-pine-dawn", "Rosé Pine Dawn", "light", "#faf4ed", "#575279"),

  t(RELAY_CODE_DARK, "Relay", "dark", null, null, "github-dark-default"),
  t("github-dark", "GitHub Dark", "dark", "#24292e", "#e1e4e8"),
  t("github-dark-dimmed", "GitHub Dark Dimmed", "dark", "#22272e", "#adbac7"),
  t("one-dark-pro", "One Dark Pro", "dark", "#282c34", "#abb2bf"),
  t("dracula", "Dracula", "dark", "#282a36", "#f8f8f2"),
  t("dracula-soft", "Dracula Soft", "dark", "#282a36", "#f6f6f4"),
  t("catppuccin-mocha", "Catppuccin Mocha", "dark", "#1e1e2e", "#cdd6f4"),
  t("vitesse-dark", "Vitesse Dark", "dark", "#121212", "#dbd7caee"),
  t("min-dark", "Min Dark", "dark", "#1f1f1f", "#b392f0"),
  t("monokai", "Monokai", "dark", "#272822", "#f8f8f2"),
  t("tokyo-night", "Tokyo Night", "dark", "#1a1b26", "#a9b1d6"),
  t("night-owl", "Night Owl", "dark", "#011627", "#d6deeb"),
  t("rose-pine", "Rosé Pine", "dark", "#191724", "#e0def4"),
  t("ayu-dark", "Ayu Dark", "dark", "#0d1017", "#bfbdb6"),
  t("solarized-dark", "Solarized Dark", "dark", "#002b36", "#839496"),
];

const BY_ID = new Map(CODE_THEMES.map((x) => [x.id, x]));

export function codeThemesFor(scheme: CodeThemeScheme): CodeTheme[] {
  return CODE_THEMES.filter((x) => x.scheme === scheme);
}

/** The theme for an id, falling back to Relay's own for that scheme (unknown or wrong-scheme ids). */
export function codeTheme(id: string | undefined, scheme: CodeThemeScheme): CodeTheme {
  const hit = id ? BY_ID.get(id) : undefined;
  if (hit && hit.scheme === scheme) return hit;
  return BY_ID.get(scheme === "light" ? RELAY_CODE_LIGHT : RELAY_CODE_DARK)!;
}

export function isCodeThemeId(id: unknown, scheme: CodeThemeScheme): id is string {
  return typeof id === "string" && BY_ID.get(id)?.scheme === scheme;
}

/** Dynamic imports of the shiki theme modules (one chunk each, fetched on first use). */
export const SHIKI_THEME_LOADERS: Record<string, () => Promise<{ default: unknown }>> = {
  "github-light-default": () => import("shiki/dist/themes/github-light-default.mjs"),
  "github-dark-default": () => import("shiki/dist/themes/github-dark-default.mjs"),
  "github-light": () => import("shiki/dist/themes/github-light.mjs"),
  "one-light": () => import("shiki/dist/themes/one-light.mjs"),
  "catppuccin-latte": () => import("shiki/dist/themes/catppuccin-latte.mjs"),
  "solarized-light": () => import("shiki/dist/themes/solarized-light.mjs"),
  "vitesse-light": () => import("shiki/dist/themes/vitesse-light.mjs"),
  "min-light": () => import("shiki/dist/themes/min-light.mjs"),
  "rose-pine-dawn": () => import("shiki/dist/themes/rose-pine-dawn.mjs"),
  "github-dark": () => import("shiki/dist/themes/github-dark.mjs"),
  "github-dark-dimmed": () => import("shiki/dist/themes/github-dark-dimmed.mjs"),
  "one-dark-pro": () => import("shiki/dist/themes/one-dark-pro.mjs"),
  dracula: () => import("shiki/dist/themes/dracula.mjs"),
  "dracula-soft": () => import("shiki/dist/themes/dracula-soft.mjs"),
  "catppuccin-mocha": () => import("shiki/dist/themes/catppuccin-mocha.mjs"),
  "vitesse-dark": () => import("shiki/dist/themes/vitesse-dark.mjs"),
  "min-dark": () => import("shiki/dist/themes/min-dark.mjs"),
  monokai: () => import("shiki/dist/themes/monokai.mjs"),
  "tokyo-night": () => import("shiki/dist/themes/tokyo-night.mjs"),
  "night-owl": () => import("shiki/dist/themes/night-owl.mjs"),
  "rose-pine": () => import("shiki/dist/themes/rose-pine.mjs"),
  "ayu-dark": () => import("shiki/dist/themes/ayu-dark.mjs"),
  "solarized-dark": () => import("shiki/dist/themes/solarized-dark.mjs"),
};
