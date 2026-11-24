import { describe, expect, it } from "vitest";
import tokensCss from "../styles/tokens.css?raw";
import {
  BUILTIN_THEMES,
  alphaOf,
  deriveColors,
  keepAlpha,
  mixHex,
  parseTheme,
  rgbChannels,
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

  it("derives the compare colors from success and danger", () => {
    const colors = deriveColors(
      { name: "t", base: "light", colors: { success: "#00aa00", danger: "#cc0000" } },
      true,
    );
    expect(colors["diff-added"]).toBe("rgb(0 170 0 / 0.1)");
    expect(colors["diff-removed-word"]).toBe("rgb(204 0 0 / 0.22)");
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

  it("reads the channels of computed colors", () => {
    expect(rgbChannels("rgb(17, 19, 23)")).toEqual([17, 19, 23]);
    expect(rgbChannels("rgba(246, 247, 249, 0.5)")).toEqual([246, 247, 249]);
    expect(rgbChannels("rgb(10.6 300 0)")).toEqual([11, 255, 0]);
    expect(rgbChannels("transparent")).toBeNull();
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

  it("keep each compare color readable on its row's tint, hovered too", () => {
    const palette = (selector: string) => {
      const start = tokensCss.indexOf(`${selector} {`);
      const body = tokensCss.slice(start, tokensCss.indexOf("\n}", start));
      return Object.fromEntries(
        [...body.matchAll(/--([\w-]+):\s*([^;]+);/g)].map(([, key, value]) => [key, value.trim()]),
      );
    };
    const dark = palette(":root");
    const bases = { dark, light: { ...dark, ...palette(':root[data-theme="light"]') } };
    const rgb = (color: string) => {
      const channels = rgbChannels(color) ?? rgbChannels(hexAsRgb(toInputColor(color) ?? ""));
      if (!channels) throw new Error(`not a color: ${color}`);
      return channels.map((channel) => channel / 255);
    };
    const over = (top: number[], alpha: number, below: number[]) =>
      top.map((channel, index) => channel * alpha + below[index] * (1 - alpha));
    const luminance = (channels: number[]) => {
      const [r, g, b] = channels.map((value) =>
        value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
      );
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    };
    const contrast = (one: number[], other: number[]) => {
      const [high, low] = [luminance(one), luminance(other)].sort((a, b) => b - a);
      return (high + 0.05) / (low + 0.05);
    };
    const states = ["newer", "older", "only", "different", "identical"];
    const weak: string[] = [];
    for (const { theme } of BUILTIN_THEMES) {
      const kinds = theme.base === "system" ? (["dark", "light"] as const) : [theme.base];
      for (const kind of kinds) {
        const colors: Record<string, string> = { ...bases[kind], ...theme.colors };
        const panel = rgb(colors["surface-raised"]);
        const hover = over(rgb(colors["row-hover"]), alphaOf(colors["row-hover"]), panel);
        for (const state of states) {
          const color = rgb(colors[`compare-${state}`]);
          // As app.css draws a compared row: a 14% tint, and 24% over the hover shade.
          for (const [row, tint] of [
            [panel, 0.14],
            [hover, 0.24],
          ] as const) {
            const ratio = contrast(color, over(color, tint, row));
            if (ratio < 3) weak.push(`${theme.name} (${kind}) ${state}: ${ratio.toFixed(2)}`);
          }
        }
      }
    }
    expect(weak).toEqual([]);
  });
});

function hexAsRgb(hex: string): string {
  const value = Number.parseInt(hex.slice(1), 16);
  return `rgb(${value >> 16}, ${(value >> 8) & 255}, ${value & 255})`;
}
