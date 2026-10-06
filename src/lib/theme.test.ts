import { describe, expect, it } from "vitest";
import {
  BUILTIN_THEMES,
  alphaOf,
  deriveColors,
  keepAlpha,
  mixHex,
  parseTheme,
  serializeTheme,
  toInputColor,
  withColor,
  withoutColor,
} from "./theme";

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

  it("reads and keeps transparency", () => {
    expect(alphaOf("#3d7eff")).toBe(1);
    expect(alphaOf("#3d7eff80")).toBe(0.5);
    expect(alphaOf("#fff8")).toBe(0.53);
    expect(alphaOf("rgb(61 126 255 / 0.16)")).toBe(0.16);
    expect(alphaOf("rgba(61, 126, 255, 0.4)")).toBe(0.4);
    expect(alphaOf("rgb(61 126 255 / 30%)")).toBe(0.3);
    expect(alphaOf("rgb(61, 126, 255)")).toBe(1);
    expect(keepAlpha("#ff0000", "rgb(61 126 255 / 0.22)")).toBe("rgb(255 0 0 / 0.22)");
    expect(keepAlpha("#ff0000", "#3d7eff")).toBe("#ff0000");
  });

  it("removes a color so the base decides it again", () => {
    const theme = { name: "t", base: "dark" as const, colors: { accent: "#3dbb76", text: "#fff" } };
    expect(withoutColor(theme, "text").colors).toEqual({ accent: "#3dbb76" });
    expect(theme.colors.text).toBe("#fff");
  });
});

describe("built-in themes", () => {
  it("use only known colors and valid values", () => {
    for (const { theme } of BUILTIN_THEMES) {
      expect(parseTheme(serializeTheme(theme), theme.name).problems).toEqual([]);
    }
  });
});
