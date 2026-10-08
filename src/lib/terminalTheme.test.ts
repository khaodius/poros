import { describe, expect, it } from "vitest";
import { mixColors, terminalTheme, toHexColor } from "./terminalTheme";

describe("toHexColor", () => {
  it("reads the forms browsers compute colors to", () => {
    expect(toHexColor("rgb(61, 126, 255)")).toBe("#3d7eff");
    expect(toHexColor("rgba(61, 126, 255, 0.5)")).toBe("#3d7eff80");
    expect(toHexColor("rgb(61 126 255 / 0.22)")).toBe("#3d7eff38");
    expect(toHexColor("color(srgb 1 0.5 0)")).toBe("#ff8000");
    expect(toHexColor("color(srgb 0 0 1 / 0.25)")).toBe("#0000ff40");
  });

  it("rejects what it cannot read", () => {
    expect(toHexColor("")).toBeNull();
    expect(toHexColor("var(--accent)")).toBeNull();
    expect(toHexColor("rgb(red, green)")).toBeNull();
  });
});

describe("terminalTheme", () => {
  const palette: Record<string, string> = {
    "--surface-sunken": "rgb(12, 14, 17)",
    "--text": "rgb(227, 231, 238)",
    "--accent": "rgb(138, 99, 230)",
    "--success": "rgb(61, 187, 118)",
  };

  it("follows the theme's background, text and accent", () => {
    const theme = terminalTheme((variable) => palette[variable] ?? null);
    expect(theme.background).toBe("#0c0e11");
    expect(theme.foreground).toBe("#e3e7ee");
    expect(theme.cursor).toBe("#8a63e6");
    expect(theme.blue).toBe("#8a63e6");
    expect(theme.cyan).toBe(mixColors("#3dbb76", "#8a63e6", 0.5));
  });

  it("keeps black dark and white light only on dark backgrounds", () => {
    const light = terminalTheme(
      (variable) =>
        ({ ...palette, "--surface-sunken": "rgb(236, 238, 242)", "--text": "rgb(27, 31, 38)" })[
          variable
        ] ?? null,
    );
    expect(light.black).toBe("#1b1f26");
    const dark = terminalTheme((variable) => palette[variable] ?? null);
    expect(dark.brightWhite).toBe("#e3e7ee");
  });
});
