// Automated AA check of the design tokens: parses tokens.css, resolves every theme x mode x accent
// combination the way the cascade does, and checks each text pair against WCAG 2.x thresholds.
import { readFileSync, writeFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { contrast, THEMES, ACCENTS, type ThemeName, type AccentName, type Scheme } from "../lib/theme";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

type Block = { selectors: string[]; vars: Record<string, string> };
function parse(): Block[] {
  const out: Block[] = [];
  const re = /([^{}@]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css))) {
    const selectors = m[1]!.split(",").map((s) => s.trim()).filter(Boolean);
    const vars: Record<string, string> = {};
    for (const d of m[2]!.split(";")) {
      const i = d.indexOf(":");
      if (i < 0) continue;
      const k = d.slice(0, i).trim();
      if (k.startsWith("--")) vars[k] = d.slice(i + 1).trim();
    }
    out.push({ selectors, vars });
  }
  return out;
}
const blocks = parse();

/** Minimal matcher for the selector shapes used in tokens.css: `:root` plus attribute tests. */
function matches(sel: string, attrs: Record<string, string>): number | null {
  if (!sel.startsWith(":root")) return null;
  if (sel.includes(":not(") || sel.includes(" ")) return null; // material/media rules are not colour pairs
  const tests = [...sel.matchAll(/\[data-([a-z]+)="([^"]+)"\]/g)];
  for (const t of tests) if (attrs[t[1]!] !== t[2]) return null;
  return 1 + tests.length; // specificity in attribute units (:root counts as one)
}

function resolve(theme: ThemeName, mode: Scheme, accent: AccentName): Record<string, string> {
  const attrs = { theme, mode, accent };
  const hits: { spec: number; order: number; vars: Record<string, string> }[] = [];
  blocks.forEach((b, order) => {
    let best: number | null = null;
    for (const s of b.selectors) {
      const sp = matches(s, attrs);
      if (sp !== null && (best === null || sp > best)) best = sp;
    }
    if (best !== null) hits.push({ spec: best, order, vars: b.vars });
  });
  hits.sort((a, b) => a.spec - b.spec || a.order - b.order);
  const out: Record<string, string> = {};
  for (const h of hits) Object.assign(out, h.vars);
  return out;
}

const TEXT_PAIRS: [string, string, number][] = [
  ["--relay-text", "--relay-bg", 7],
  ["--relay-text", "--relay-sidebar", 7],
  ["--relay-text", "--relay-surface", 7],
  ["--relay-text", "--relay-surface-raised", 7],
  ["--relay-text-2", "--relay-bg", 4.5],
  ["--relay-text-2", "--relay-surface", 4.5],
  ["--relay-text-2", "--relay-surface-sunk", 4.5],
  ["--relay-text-muted", "--relay-bg", 4.5],
  ["--relay-text-muted", "--relay-sidebar", 4.5],
  ["--relay-text-muted", "--relay-surface", 4.5],
  ["--relay-text-muted", "--relay-surface-raised", 4.5],
  ["--relay-text-muted", "--relay-surface-sunk", 4.5],
  ["--relay-on-accent", "--relay-accent", 4.5],
  ["--relay-on-accent", "--relay-accent-hover", 4.5],
  ["--relay-accent-text", "--relay-bg", 4.5],
  ["--relay-accent-text", "--relay-surface", 4.5],
  ["--relay-accent-text", "--relay-accent-soft", 4.5],
  ["--relay-tooltip-text", "--relay-tooltip-bg", 7],
];
const STATUS_PAIRS: [string, string, number][] = [
  ["--relay-danger", "--relay-danger-soft", 4.5],
  ["--relay-success", "--relay-success-soft", 4.5],
  ["--relay-warning", "--relay-warning-soft", 4.5],
  ["--relay-info", "--relay-info-soft", 4.5],
  ["--relay-danger", "--relay-surface", 4.5],
  ["--relay-success", "--relay-surface", 4.5],
  ["--relay-on-danger", "--relay-danger", 4.5],
];
// Non-text: control boundaries (switch track, inputs) need 3:1 (WCAG 1.4.11).
const UI_PAIRS: [string, string, number][] = [
  ["--relay-border-strong", "--relay-bg", 3],
  ["--relay-border-strong", "--relay-surface", 3],
  ["--relay-focus", "--relay-bg", 3],
  ["--relay-accent", "--relay-surface", 3],
];

const themes = Object.keys(THEMES) as ThemeName[];
const accents = Object.keys(ACCENTS) as AccentName[];

export const contrastReport: string[] = [];

describe("design tokens meet WCAG AA", () => {
  for (const theme of themes) {
    for (const mode of ["light", "dark"] as Scheme[]) {
      for (const accent of accents) {
        it(`${theme} / ${mode} / ${accent}`, () => {
          const v = resolve(theme, mode, accent);
          v["--relay-focus"] = v["--relay-accent"]!;
          const all = [...TEXT_PAIRS, ...STATUS_PAIRS, ...UI_PAIRS];
          for (const [fg, bg, min] of all) {
            const a = v[fg];
            const b = v[bg];
            expect(a, `${fg} defined`).toMatch(/^#[0-9a-f]{6}$/i);
            expect(b, `${bg} defined`).toMatch(/^#[0-9a-f]{6}$/i);
            const c = contrast(a!, b!);
            expect(c, `${fg} on ${bg} = ${c.toFixed(2)} (need ${min})`).toBeGreaterThanOrEqual(min);
          }
        });
      }
    }
  }

  it("theme swatches in lib/theme.ts match tokens.css", () => {
    for (const theme of themes)
      for (const mode of ["light", "dark"] as Scheme[]) {
        const v = resolve(theme, mode, "terracotta");
        const sw = THEMES[theme][mode];
        expect(sw.bg, `${theme}/${mode} bg`).toBe(v["--relay-bg"]);
        expect(sw.sidebar, `${theme}/${mode} sidebar`).toBe(v["--relay-sidebar"]);
        expect(sw.surface, `${theme}/${mode} surface`).toBe(v["--relay-surface"]);
        expect(sw.fg, `${theme}/${mode} fg`).toBe(v["--relay-text"]);
        expect(sw.muted, `${theme}/${mode} muted`).toBe(v["--relay-text-muted"]);
      }
    for (const a of accents)
      for (const mode of ["light", "dark"] as Scheme[]) expect(ACCENTS[a][mode], `${a}/${mode}`).toBe(resolve("relay", mode, a)["--relay-accent"]);
  });

  it("prints the default theme's key ratios (for the design doc)", () => {
    for (const mode of ["light", "dark"] as Scheme[]) {
      const v = resolve("relay", mode, "terracotta");
      for (const [fg, bg] of [...TEXT_PAIRS, ...STATUS_PAIRS, ...UI_PAIRS]) {
        const f = fg === "--relay-focus" ? v["--relay-accent"]! : v[fg]!;
        contrastReport.push(`${mode}\t${fg.replace("--relay-", "")} on ${bg.replace("--relay-", "")}\t${contrast(f, v[bg]!).toFixed(2)}`);
      }
    }
    if (process.env.RELAY_PRINT_CONTRAST) writeFileSync(process.env.RELAY_PRINT_CONTRAST, contrastReport.join("\n"));
    expect(contrastReport.length).toBeGreaterThan(0);
  });
});
