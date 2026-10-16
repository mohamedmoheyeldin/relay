import { describe, expect, it } from "vitest";
import {
  ACCENTS,
  contrast,
  DEFAULT_APPEARANCE,
  effectivePalette,
  mix,
  normalizeHex,
  parseAppearance,
  resolveScheme,
  THEMES,
  themeVars,
  type ThemeName,
} from "./theme";
import { parseServerJson } from "./mcp";

describe("colour maths", () => {
  it("normalizes hex", () => {
    expect(normalizeHex("#ABC")).toBe("#aabbcc");
    expect(normalizeHex("ededed")).toBe("#ededed");
    expect(normalizeHex(" #000000 ")).toBe("#000000");
    expect(normalizeHex("#12345")).toBeNull();
    expect(normalizeHex("red")).toBeNull();
  });
  it("mixes linearly", () => {
    expect(mix("#000000", "#ffffff", 0)).toBe("#000000");
    expect(mix("#000000", "#ffffff", 1)).toBe("#ffffff");
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
  });
  it("computes WCAG contrast", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 0);
    expect(contrast("#777777", "#777777")).toBeCloseTo(1, 5);
  });
});

describe("themes", () => {
  it("resolves system mode from the OS preference", () => {
    expect(resolveScheme("system", true)).toBe("dark");
    expect(resolveScheme("system", false)).toBe("light");
    expect(resolveScheme("light", true)).toBe("light");
  });

  it("every theme keeps body and muted text readable (WCAG AA)", () => {
    for (const t of Object.keys(THEMES) as ThemeName[]) {
      for (const scheme of ["dark", "light"] as const) {
        const p = THEMES[t][scheme];
        expect(contrast(p.fg, p.bg), `${t}/${scheme} fg`).toBeGreaterThanOrEqual(7);
        expect(contrast(p.muted, p.bg), `${t}/${scheme} muted`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(p.fg, p.surface), `${t}/${scheme} surface`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("midnight is black with #EDEDED text", () => {
    expect(THEMES.midnight.dark.bg).toBe("#000000");
    expect(THEMES.midnight.dark.fg).toBe("#ededed");
  });

  it("custom colours override bg/fg and derive the rest", () => {
    const a = { ...DEFAULT_APPEARANCE, custom: { dark: { bg: "#102030", fg: "#f0f0f0" } } };
    const p = effectivePalette(a, "dark");
    expect(p.bg).toBe("#102030");
    expect(p.fg).toBe("#f0f0f0");
    expect(p.surface).not.toBe(p.bg);
    // Light mode is unaffected by a dark override.
    expect(effectivePalette(a, "light")).toEqual(THEMES.relay.light);
  });

  it("produces CSS variables, send button inverts the background", () => {
    const v = themeVars({ ...DEFAULT_APPEARANCE, accent: "green" }, "dark");
    expect(v["--bg"]).toBe("#212121");
    expect(v["--accent"]).toBe(ACCENTS.green.dark);
    expect(v["--send-bg"]).toBe(v["--fg"]);
    expect(v["--send-fg"]).toBe(v["--bg"]);
  });

  it("parses stored appearance defensively", () => {
    expect(parseAppearance(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance("{bad json")).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance(JSON.stringify({ mode: "light", theme: "nope", accent: "pink" }))).toEqual({
      mode: "light",
      theme: "relay",
      accent: "pink",
      custom: {},
    });
  });
});

describe("mcp json", () => {
  it("validates a single server", () => {
    expect(parseServerJson('{"command":"npx","args":[]}', false).ok).toBe(true);
    expect(parseServerJson('{"url":"https://x"}', false).ok).toBe(true);
    expect(parseServerJson('{"args":[]}', false).ok).toBe(false);
    expect(parseServerJson("[1]", false).ok).toBe(false);
    expect(parseServerJson("{", false).ok).toBe(false);
  });
  it("validates the whole config", () => {
    expect(parseServerJson('{"a":{"command":"x"}}', true).ok).toBe(true);
    expect(parseServerJson('{"a":1}', true).ok).toBe(false);
  });
});
