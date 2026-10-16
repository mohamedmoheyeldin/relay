// Lazy syntax highlighting with shiki (JavaScript regex engine: no WASM, CSP friendly).
// Only this module imports shiki, and it is loaded on first use, so the main bundle stays small.
import type { HighlighterCore } from "shiki/core";

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

function highlighter(): Promise<HighlighterCore> {
  hlP ??= (async () => {
    const [{ createHighlighterCore }, { createJavaScriptRegexEngine }, dark, light] = await Promise.all([
      import("shiki/core"),
      import("shiki/engine/javascript"),
      import("shiki/dist/themes/github-dark-default.mjs"),
      import("shiki/dist/themes/github-light-default.mjs"),
    ]);
    return createHighlighterCore({
      themes: [dark.default, light.default],
      langs: [],
      engine: createJavaScriptRegexEngine({ forgiving: true }),
    });
  })();
  return hlP;
}

const cache = new Map<string, string>();

/** Returns highlighted HTML, or null when the language is unknown. */
export async function highlight(code: string, lang: string): Promise<string | null> {
  const name = canonicalLang(lang);
  if (!name) return null;
  const key = `${name}\u0000${code}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const hl = await highlighter();
  if (!loaded.has(name)) {
    const mod = (await LANGS[name]!()) as { default: Parameters<HighlighterCore["loadLanguage"]>[0] };
    await hl.loadLanguage(mod.default);
    loaded.add(name);
  }
  const html = hl.codeToHtml(code, {
    lang: name,
    themes: { dark: "github-dark-default", light: "github-light-default" },
    defaultColor: false,
  });
  if (cache.size > 200) cache.delete(cache.keys().next().value!);
  cache.set(key, html);
  return html;
}
