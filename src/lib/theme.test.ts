import { describe, expect, it } from "vitest";
import { BUILTIN_THEMES, deriveColors, mixHex, parseTheme, toInputColor, withColor } from "./theme";

describe("parseTheme", () => {
  it("keeps valid colors and reports the rest", () => {
    const { theme, problems } = parseTheme(
      {
        name: "Ocean",
        base: "light",
        colors: { accent: "#0af", text: "rgb(10 20 30)", sparkle: "#fff", border: "blue-ish" },
        radius: 99,
      },
      "fallback",
    );
    expect(theme).toEqual({
      name: "Ocean",
      base: "light",
      colors: { accent: "#0af", text: "rgb(10 20 30)" },
    });
    expect(problems).toEqual([
      'Unknown color "sparkle"',
      '"border" is not a color',
      '"radius" must be a number from 0 to 16',
    ]);
  });

  it("falls back for anything that is not a theme object", () => {
    const { theme, problems } = parseTheme([1, 2], "file-name");
    expect(theme).toEqual({ name: "file-name", base: "dark", colors: {} });
    expect(problems).toEqual([]);
  });

  it("parses every built-in theme without problems", () => {
    for (const entry of BUILTIN_THEMES) {
      expect(parseTheme(entry.theme, "x").problems).toEqual([]);
    }
  });
});

describe("colors", () => {
  it("derives accent tints unless the theme sets them", () => {
    const colors = deriveColors(
      { name: "t", base: "dark", colors: { accent: "#3d7eff", "border-focus": "#ffffff" } },
      true,
    );
    expect(colors["accent-soft"]).toBe("rgb(61 126 255 / 0.16)");
    expect(colors["border-focus"]).toBe("#ffffff");
    expect(colors["accent-hover"]).toBe(mixHex("#3d7eff", "white", 0.12));
  });

  it("lets derived colors follow a changed accent", () => {
    const theme = {
      name: "t",
      base: "dark" as const,
      colors: { accent: "#8a63e6", "accent-hover": "#9d7cf0", text: "#ffffff" },
    };
    expect(withColor(theme, "accent", "#3dbb76").colors).toEqual({
      accent: "#3dbb76",
      text: "#ffffff",
    });
    expect(withColor(theme, "text", "#eeeeee").colors["accent-hover"]).toBe("#9d7cf0");
  });

  it("mixes and normalizes hex colors", () => {
    expect(mixHex("#000000", "white", 0.5)).toBe("#808080");
    expect(toInputColor("#abc")).toBe("#aabbcc");
    expect(toInputColor("rgb(255 0 16 / 0.5)")).toBe("#ff0010");
    expect(toInputColor("hsl(10 20% 30%)")).toBeNull();
  });
});
