import { describe, expect, it } from "vitest";
import { recommendedFonts, usableFonts, type MeasureText } from "./fonts";

/** Measures like a web view that knows only `known`, where `fixed` fonts have one glyph width. */
function fakeMeasure(known: string[], fixed: string[]): MeasureText {
  const glyphWidths: Record<string, [narrow: number, wide: number]> = {
    monospace: [10, 10],
    serif: [4, 11],
  };
  return (font, text) => {
    const stack = font
      .replace(/^\S+ /, "")
      .split(", ")
      .map((family) => family.replace(/^"|"$/g, ""));
    const drawnIn = stack.find((family) => known.includes(family) || family in glyphWidths)!;
    const [narrow, wide] =
      glyphWidths[drawnIn] ?? (fixed.includes(drawnIn) ? [9, 9] : [3, 12 + known.indexOf(drawnIn)]);
    return [...text].reduce((total, character) => total + (character === "i" ? narrow : wide), 0);
  };
}

describe("usableFonts", () => {
  const families = [
    { name: "Segoe UI Variable", alternates: ["Segoe UI Variable Text"], monospaced: false },
    { name: "Fira Code", alternates: [], monospaced: false },
    { name: "Ghost", alternates: ["Ghost Regular"], monospaced: true },
    { name: "Inter", alternates: [], monospaced: false },
  ];

  it("keeps fonts the web view can draw, by a name it knows", () => {
    const measure = fakeMeasure(["Segoe UI Variable Text", "Fira Code", "Inter"], ["Fira Code"]);
    expect(usableFonts(families, measure)).toEqual([
      { name: "Fira Code", monospaced: true },
      { name: "Inter", monospaced: false },
      { name: "Segoe UI Variable Text", monospaced: false },
    ]);
  });

  it("trusts the font files when it cannot measure", () => {
    expect(usableFonts(families, null).map((option) => option.name)).toEqual([
      "Fira Code",
      "Ghost",
      "Inter",
      "Segoe UI Variable",
    ]);
  });
});

describe("recommendedFonts", () => {
  it("lists the installed recommendations, best first", () => {
    const installed = [
      { name: "consolas", monospaced: true },
      { name: "Arial", monospaced: false },
      { name: "JetBrains Mono", monospaced: true },
    ];
    expect(recommendedFonts(installed, "mono").map((option) => option.name)).toEqual([
      "JetBrains Mono",
      "consolas",
    ]);
    expect(recommendedFonts(installed, "ui")).toEqual([]);
  });
});
