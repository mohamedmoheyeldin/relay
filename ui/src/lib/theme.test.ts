import { describe, expect, it } from "vitest";
import { ACCENTS, contrast, customVars, DEFAULT_APPEARANCE, effectiveSwatch, mix, normalizeHex, parseAppearance, resolveScheme, softenTo, THEMES, type ThemeName } from "./theme";
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
  it("softens a colour only as far as the contrast floor allows", () => {
    const c = softenTo("#1f1d1a", "#faf7f2", 4.5);
    expect(contrast(c, "#faf7f2")).toBeGreaterThanOrEqual(4.5);
    expect(c).not.toBe("#1f1d1a");
  });
});

describe("themes", () => {
  it("resolves system mode from the OS preference", () => {
    expect(resolveScheme("system", true)).toBe("dark");
    expect(resolveScheme("system", false)).toBe("light");
    expect(resolveScheme("light", true)).toBe("light");
  });

  it("every theme swatch keeps body and muted text readable (WCAG AA)", () => {
    for (const t of Object.keys(THEMES) as ThemeName[]) {
      for (const scheme of ["dark", "light"] as const) {
        const p = THEMES[t][scheme];
        expect(contrast(p.fg, p.bg), `${t}/${scheme} fg`).toBeGreaterThanOrEqual(7);
        expect(contrast(p.muted, p.bg), `${t}/${scheme} muted`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(p.fg, p.surface), `${t}/${scheme} surface`).toBeGreaterThanOrEqual(4.5);
        expect(contrast(p.muted, p.sidebar), `${t}/${scheme} muted on sidebar`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("accents are readable as text on the default canvas", () => {
    for (const a of Object.values(ACCENTS)) {
      expect(contrast(a.light, THEMES.relay.light.bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(a.dark, THEMES.relay.dark.bg)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("custom colours override bg/fg and derive readable secondary text", () => {
    const a = { ...DEFAULT_APPEARANCE, custom: { dark: { bg: "#102030", fg: "#f0f0f0" } } };
    const v = customVars(a, "dark")!;
    expect(v["--relay-bg"]).toBe("#102030");
    expect(v["--relay-text"]).toBe("#f0f0f0");
    expect(contrast(v["--relay-text-muted"]!, "#102030")).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v["--relay-text-2"]!, "#102030")).toBeGreaterThanOrEqual(7);
    // Light mode is unaffected by a dark override.
    expect(customVars(a, "light")).toBeNull();
    expect(effectiveSwatch(a, "light")).toEqual({ bg: THEMES.relay.light.bg, fg: THEMES.relay.light.fg });
    expect(effectiveSwatch(a, "dark")).toEqual({ bg: "#102030", fg: "#f0f0f0" });
  });

  it("parses stored appearance defensively", () => {
    expect(parseAppearance(null)).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance("{bad json")).toEqual(DEFAULT_APPEARANCE);
    expect(parseAppearance(JSON.stringify({ mode: "light", theme: "nope", accent: "sage", prose: "serif" }))).toEqual({
      mode: "light",
      theme: "relay",
      accent: "sage",
      prose: "serif",
      custom: {},
    });
    // Accents from the previous UI fall back to the default.
    expect(parseAppearance(JSON.stringify({ accent: "blue" })).accent).toBe("terracotta");
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
