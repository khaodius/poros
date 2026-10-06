import type { FontFamily } from "./types";

export type FontKind = "ui" | "mono";

export interface FontOption {
  name: string;
  monospaced: boolean;
}

/** Good screen fonts, best first. Only the installed ones are offered. */
export const RECOMMENDED_FONTS: Record<FontKind, string[]> = {
  ui: [
    "Inter",
    "Segoe UI Variable Text",
    "Segoe UI",
    "IBM Plex Sans",
    "Source Sans 3",
    "Noto Sans",
    "Roboto",
    "Open Sans",
    "Fira Sans",
    "Ubuntu",
    "Cantarell",
    "Atkinson Hyperlegible",
    "Lato",
    "Work Sans",
    "Verdana",
    "DejaVu Sans",
    "Liberation Sans",
  ],
  mono: [
    "JetBrains Mono",
    "Cascadia Code",
    "Cascadia Mono",
    "Fira Code",
    "Source Code Pro",
    "IBM Plex Mono",
    "Hack",
    "Iosevka",
    "Roboto Mono",
    "Ubuntu Mono",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Consolas",
    "Inconsolata",
    "Liberation Mono",
  ],
};

/** Width in pixels of `text` drawn in the CSS `font`. */
export type MeasureText = (font: string, text: string) => number;

/** Letters only: emoji and symbol fonts draw digits and signs but should not count. */
const PROBE_TEXT = "mmmmmmmmmmlliWQ";
const PROBE_SIZE = "72px";
const GENERIC_FAMILIES = ["monospace", "serif"];

export function quoteFamily(name: string): string {
  return `"${name.replace(/["\\]/g, "\\$&")}"`;
}

/**
 * Whether the web view can draw `name`. A family it does not know falls back to the generic one
 * after it, so the text measures the same as the generic family alone.
 */
function isDrawable(name: string, measure: MeasureText): boolean {
  return GENERIC_FAMILIES.some(
    (generic) =>
      measure(`${PROBE_SIZE} ${quoteFamily(name)}, ${generic}`, PROBE_TEXT) !==
      measure(`${PROBE_SIZE} ${generic}`, PROBE_TEXT),
  );
}

function isFixedWidth(name: string, measure: MeasureText): boolean {
  const font = `${PROBE_SIZE} ${quoteFamily(name)}, monospace`;
  return measure(font, "iiiiiiiiii") === measure(font, "MMMMMMMMMM");
}

export function canvasMeasure(): MeasureText | null {
  const context = document.createElement("canvas").getContext("2d");
  if (!context) return null;
  return (font, text) => {
    context.font = font;
    return context.measureText(text).width;
  };
}

/**
 * The installed families the web view can actually use, by the name it knows them by. Without a
 * way to measure, every family is offered and the font file says which are fixed width.
 */
export function usableFonts(families: FontFamily[], measure: MeasureText | null): FontOption[] {
  const options = new Map<string, FontOption>();
  const add = (name: string, monospaced: boolean) => {
    const key = name.toLowerCase();
    if (!options.has(key)) options.set(key, { name, monospaced });
  };
  for (const family of families) {
    if (!measure) {
      add(family.name, family.monospaced);
      continue;
    }
    const names = isDrawable(family.name, measure)
      ? [family.name]
      : family.alternates.filter((alternate) => isDrawable(alternate, measure));
    for (const name of names) add(name, isFixedWidth(name, measure));
  }
  return [...options.values()].sort((left, right) =>
    left.name.localeCompare(right.name, undefined, { sensitivity: "base" }),
  );
}

/** The recommended fonts of `kind` that are installed, best first. */
export function recommendedFonts(options: FontOption[], kind: FontKind): FontOption[] {
  const byName = new Map(options.map((option) => [option.name.toLowerCase(), option]));
  return RECOMMENDED_FONTS[kind].flatMap((name) => byName.get(name.toLowerCase()) ?? []);
}
