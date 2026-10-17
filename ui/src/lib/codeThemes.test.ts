import { describe, expect, it } from "vitest";
import { normalizeTheme } from "shiki/core";
import { CODE_THEMES, codeTheme, codeThemesFor, RELAY_CODE_DARK, RELAY_CODE_LIGHT, SHIKI_THEME_LOADERS } from "./codeThemes";

describe("code themes", () => {
  it("offers the requested light and dark themes plus Relay's own", () => {
    expect(codeThemesFor("light").map((t) => t.label)).toEqual([
      "Relay",
      "GitHub Light",
      "One Light",
      "Catppuccin Latte",
      "Solarized Light",
      "Vitesse Light",
      "Min Light",
      "Rosé Pine Dawn",
    ]);
    expect(codeThemesFor("dark").map((t) => t.label)).toEqual([
      "Relay",
      "GitHub Dark",
      "GitHub Dark Dimmed",
      "One Dark Pro",
      "Dracula",
      "Dracula Soft",
      "Catppuccin Mocha",
      "Vitesse Dark",
      "Min Dark",
      "Monokai",
      "Tokyo Night",
      "Night Owl",
      "Rosé Pine",
      "Ayu Dark",
      "Solarized Dark",
    ]);
  });

  it("keeps the background and text colours equal to the shiki theme files", async () => {
    for (const t of CODE_THEMES) {
      expect(SHIKI_THEME_LOADERS[t.shiki], t.id).toBeTypeOf("function");
      const raw = (await SHIKI_THEME_LOADERS[t.shiki]!()).default as Parameters<typeof normalizeTheme>[0];
      const n = normalizeTheme(raw);
      expect(n.type, t.id).toBe(t.scheme);
      if (t.bg) {
        expect(t.bg, `${t.id} bg`).toBe(n.bg.toLowerCase());
        expect(t.fg, `${t.id} fg`).toBe(n.fg.toLowerCase());
      }
    }
  });

  it("falls back to Relay's own theme for unknown or wrong-scheme ids", () => {
    expect(codeTheme("nope", "light").id).toBe(RELAY_CODE_LIGHT);
    expect(codeTheme("dracula", "light").id).toBe(RELAY_CODE_LIGHT);
    expect(codeTheme(undefined, "dark").id).toBe(RELAY_CODE_DARK);
    expect(codeTheme("dracula", "dark").label).toBe("Dracula");
  });
});
