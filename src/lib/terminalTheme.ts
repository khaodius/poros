// Terminal colors drawn from the app theme, so a shell follows the theme and accent like the
// rest of the window. The terminal renderer needs plain hex colors, not CSS variables.

import type { ITheme } from "@xterm/xterm";

type Rgba = [red: number, green: number, blue: number, alpha: number];

/** Reads a computed CSS color (`rgb()`, `rgba()` or `color(srgb ...)`) as `#rrggbb`, with an
 * alpha byte when it is see-through. */
export function toHexColor(computed: string): string | null {
  const rgba = parseColor(computed.trim());
  if (!rgba) return null;
  const [red, green, blue, alpha] = rgba;
  const byte = (value: number) =>
    Math.round(Math.min(255, Math.max(0, value)))
      .toString(16)
      .padStart(2, "0");
  return `#${byte(red)}${byte(green)}${byte(blue)}${alpha < 1 ? byte(alpha * 255) : ""}`;
}

function parseColor(computed: string): Rgba | null {
  const legacy = /^rgba?\((.+)\)$/i.exec(computed);
  const srgb = /^color\(srgb (.+)\)$/i.exec(computed);
  const parts = (legacy ?? srgb)?.[1]
    .split(/[\s,/]+/)
    .filter(Boolean)
    .map(Number);
  if (!parts || parts.length < 3 || parts.some(Number.isNaN)) return null;
  const scale = srgb ? 255 : 1;
  return [parts[0] * scale, parts[1] * scale, parts[2] * scale, parts[3] ?? 1];
}

function channels(hex: string): Rgba {
  const value = (offset: number) => parseInt(hex.slice(offset, offset + 2), 16);
  return [value(1), value(3), value(5), hex.length > 7 ? value(7) / 255 : 1];
}

/** `share` of `second` blended into `first`, ignoring transparency. */
export function mixColors(first: string, second: string, share: number): string {
  const [from, to] = [channels(first), channels(second)];
  const mixed = from.slice(0, 3).map((value, index) => value + (to[index] - value) * share);
  return toHexColor(`rgb(${mixed.join(", ")})`)!;
}

function luminance(hex: string): number {
  const [red, green, blue] = channels(hex);
  return (0.2126 * red + 0.7152 * green + 0.0722 * blue) / 255;
}

/** Reads the app's color variables (`--accent` and so on) as computed CSS colors. */
export type ThemeColorReader = (variable: string) => string | null;

export function terminalTheme(read: ThemeColorReader): ITheme {
  const color = (variable: string, fallback: string) => {
    const computed = read(variable);
    return (computed && toHexColor(computed)) ?? fallback;
  };
  const background = color("--surface-sunken", "#0c0e11");
  const text = color("--text", "#e3e7ee");
  const dark = luminance(background) < 0.5;
  const accent = color("--accent", "#3d7eff");
  const success = color("--success", "#3dbb76");
  const magenta = color("--icon-image", "#c678dd");
  const cyan = mixColors(success, accent, 0.5);
  const brighter = (base: string) => mixColors(base, text, 0.3);
  const pressed = color("--surface-pressed", "#2a303a");
  return {
    background,
    foreground: text,
    cursor: accent,
    cursorAccent: background,
    selectionBackground: color("--row-selected", "#3d7eff38"),
    scrollbarSliderBackground: color("--scrollbar", "#353c47"),
    scrollbarSliderHoverBackground: color("--scrollbar-hover", "#6b7483"),
    scrollbarSliderActiveBackground: color("--scrollbar-hover", "#6b7483"),
    black: dark ? pressed : text,
    brightBlack: color("--text-faint", "#6b7483"),
    red: color("--danger", "#e5484d"),
    brightRed: color("--danger-hover", "#f05a5f"),
    green: success,
    brightGreen: brighter(success),
    yellow: color("--warning", "#f0a33a"),
    brightYellow: color("--icon-key", "#e5c07b"),
    blue: accent,
    brightBlue: color("--accent-hover", "#5590ff"),
    magenta,
    brightMagenta: brighter(magenta),
    cyan,
    brightCyan: brighter(cyan),
    white: dark ? color("--text-muted", "#9aa3b2") : pressed,
    brightWhite: dark ? text : color("--surface-raised", "#ffffff"),
  };
}
