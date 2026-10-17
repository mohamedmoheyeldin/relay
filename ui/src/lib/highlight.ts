// Lazy syntax highlighting with shiki (JavaScript regex engine: no WASM, CSP friendly).
// Only this module imports shiki, and it is loaded on first use, so the main bundle stays small.
import type { HighlighterCore } from "shiki/core";
import { SHIKI_THEME_LOADERS } from "./codeThemes";

const LANGS: Record<string, () => Promise<unknown>> = {
  javascript: () => import("shiki/dist/langs/javascript.mjs"),
  typescript: () => import("shiki/dist/langs/typescript.mjs"),
  tsx: () => import("shiki/dist/langs/tsx.mjs"),
  jsx: () => import("shiki/dist/langs/jsx.mjs"),
  json: () => import("shiki/dist/langs/json.mjs"),
  python: () => import("shiki/dist/langs/python.mjs"),
  rust: () => import("shiki/dist/langs/rust.mjs"),
  powershell: () => import("shiki/dist/langs/powershell.mjs"),
  shellscript: () => import("shiki/dist/langs/shellscript.mjs"),
  bat: () => import("shiki/dist/langs/bat.mjs"),
  html: () => import("shiki/dist/langs/html.mjs"),
  css: () => import("shiki/dist/langs/css.mjs"),
  markdown: () => import("shiki/dist/langs/markdown.mjs"),
  yaml: () => import("shiki/dist/langs/yaml.mjs"),
  toml: () => import("shiki/dist/langs/toml.mjs"),
  sql: () => import("shiki/dist/langs/sql.mjs"),
  c: () => import("shiki/dist/langs/c.mjs"),
  cpp: () => import("shiki/dist/langs/cpp.mjs"),
  csharp: () => import("shiki/dist/langs/csharp.mjs"),
  go: () => import("shiki/dist/langs/go.mjs"),
  java: () => import("shiki/dist/langs/java.mjs"),
  diff: () => import("shiki/dist/langs/diff.mjs"),
  xml: () => import("shiki/dist/langs/xml.mjs"),
  ini: () => import("shiki/dist/langs/ini.mjs"),
};

const ALIASES: Record<string, string> = {
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  py: "python",
  rs: "rust",
  ps: "powershell",
  ps1: "powershell",
  pwsh: "powershell",
  sh: "shellscript",
  bash: "shellscript",
  shell: "shellscript",
  zsh: "shellscript",
  console: "shellscript",
  cmd: "bat",
  batch: "bat",
  md: "markdown",
  yml: "yaml",
  "c++": "cpp",
  cs: "csharp",
  "c#": "csharp",
  golang: "go",
  htm: "html",
  svg: "xml",
  jsonc: "json",
  json5: "json",
};

export function canonicalLang(lang: string): string | null {
  const l = lang.toLowerCase().trim();
  const name = ALIASES[l] ?? l;
  return name in LANGS ? name : null;
}

let hlP: Promise<HighlighterCore> | null = null;
const loaded = new Set<string>();
const loadedThemes = new Set<string>();

function highlighter(): Promise<HighlighterCore> {
  hlP ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }] = await Promise.all([import("shiki/core"), import("shiki/engine/javascript")]);
    return createHighlighterCore({ themes: [], langs: [], engine: createJavaScriptRegexEngine({ forgiving: true }) });
  })();
  return hlP;
}

async function ensureLang(hl: HighlighterCore, name: string) {
  if (loaded.has(name)) return;
  const mod = (await LANGS[name]!()) as { default: Parameters<HighlighterCore["loadLanguage"]>[0] };
  await hl.loadLanguage(mod.default);
  loaded.add(name);
}

async function ensureTheme(hl: HighlighterCore, name: string): Promise<string> {
  const theme = SHIKI_THEME_LOADERS[name] ? name : "github-dark-default";
  if (loadedThemes.has(theme)) return theme;
  const mod = (await SHIKI_THEME_LOADERS[theme]!()) as { default: Parameters<HighlighterCore["loadTheme"]>[0] };
  await hl.loadTheme(mod.default);
  loadedThemes.add(theme);
  return theme;
}

const cache = new Map<string, string>();

/** shiki theme names for the two colour schemes. */
export type ThemePair = { light: string; dark: string };
const DEFAULT_PAIR: ThemePair = { light: "github-light-default", dark: "github-dark-default" };

/** Returns highlighted HTML (both schemes as CSS variables), or null when the language is unknown. */
export async function highlight(code: string, lang: string, themes: ThemePair = DEFAULT_PAIR): Promise<string | null> {
  const name = canonicalLang(lang);
  if (!name) return null;
  const key = `${name}\u0000${themes.light}\u0000${themes.dark}\u0000${code}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const hl = await highlighter();
  const [light, dark] = await Promise.all([ensureTheme(hl, themes.light), ensureTheme(hl, themes.dark), ensureLang(hl, name)]);
  const html = hl.codeToHtml(code, { lang: name, themes: { dark, light }, defaultColor: false });
  if (cache.size > 200) cache.delete(cache.keys().next().value!);
  cache.set(key, html);
  return html;
}

export type Token = { content: string; color?: string };

/** Tokens per line in one theme (used by the theme previews in Settings). */
export async function tokenize(code: string, lang: string, theme: string): Promise<Token[][] | null> {
  const name = canonicalLang(lang);
  if (!name) return null;
  const hl = await highlighter();
  const [resolved] = await Promise.all([ensureTheme(hl, theme), ensureLang(hl, name)]);
  return hl.codeToTokensBase(code, { lang: name, theme: resolved }).map((line) => line.map((t) => ({ content: t.content, color: t.color })));
}
