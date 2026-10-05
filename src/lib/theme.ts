// Theme files: JSON in the `themes` folder of the app config folder. Each color key sets the
// CSS custom property of the same name (`accent` sets `--accent`) over the base palette in
// `styles/tokens.css`. Built-in themes use the same format.

export type ThemeBase = "dark" | "light" | "system";

export interface Theme {
  name: string;
  base: ThemeBase;
  colors: Record<string, string>;
  fonts?: { ui?: string; mono?: string };
  /** Corner radius in pixels for panes and controls. */
  radius?: number;
}

export interface ColorToken {
  key: string;
  label: string;
  group: string;
  /** Holds a solid color the simple editor can pick; others take alpha or are derived. */
  editable: boolean;
}

const token = (key: string, label: string, group: string, editable = true): ColorToken => ({
  key,
  label,
  group,
  editable,
});

export const COLOR_TOKENS: ColorToken[] = [
  token("accent", "Accent", "Accent"),
  token("accent-hover", "Accent hover", "Accent"),
  token("accent-soft", "Accent tint", "Accent", false),
  token("accent-softer", "Faint accent tint", "Accent", false),
  token("text-on-accent", "Text on accent", "Accent"),
  token("surface-base", "Window background", "Surfaces"),
  token("surface-raised", "Panels", "Surfaces"),
  token("surface-sunken", "Inputs and lists", "Surfaces"),
  token("surface-overlay", "Menus and dialogs", "Surfaces"),
  token("surface-hover", "Hover", "Surfaces"),
  token("surface-pressed", "Pressed", "Surfaces"),
  token("border", "Borders", "Borders"),
  token("border-strong", "Strong borders", "Borders"),
  token("border-focus", "Focus ring", "Borders"),
  token("text", "Text", "Text"),
  token("text-muted", "Secondary text", "Text"),
  token("text-faint", "Faint text", "Text"),
  token("danger", "Danger", "Status"),
  token("danger-hover", "Danger hover", "Status"),
  token("danger-soft", "Danger tint", "Status", false),
  token("warning", "Warning", "Status"),
  token("success", "Success", "Status"),
  token("icon-folder", "Folders", "File icons"),
  token("icon-image", "Images", "File icons"),
  token("icon-video", "Video", "File icons"),
  token("icon-audio", "Audio", "File icons"),
  token("icon-archive", "Archives", "File icons"),
  token("icon-data", "Data", "File icons"),
  token("icon-key", "Keys", "File icons"),
  token("icon-code", "Code", "File icons"),
  token("icon-text", "Documents", "File icons"),
  token("icon-file", "Other files", "File icons"),
  token("row-selected", "Selected row", "Lists", false),
  token("row-selected-inactive", "Selected row, inactive pane", "Lists", false),
  token("row-hover", "Row hover", "Lists", false),
  token("backdrop", "Dialog backdrop", "Lists", false),
];

const TOKEN_KEYS = new Set(COLOR_TOKENS.map((entry) => entry.key));

/** The colors the settings page offers as plain pickers. */
export const SIMPLE_COLOR_KEYS = [
  "surface-base",
  "surface-raised",
  "surface-sunken",
  "border",
  "text",
  "text-muted",
  "icon-folder",
  "success",
  "warning",
  "danger",
];

export const ACCENT_PRESETS = [
  "#3d7eff",
  "#8a63e6",
  "#d1478f",
  "#e5484d",
  "#f0a33a",
  "#3dbb76",
  "#14a8b8",
  "#8b95a5",
];

export const BUILTIN_PREFIX = "builtin:";

export interface ThemeEntry {
  id: string;
  theme: Theme;
  builtin: boolean;
  path?: string;
  /** Why the file could not be used, or which parts of it were ignored. */
  problems: string[];
}

const BUILTIN_DEFINITIONS: Theme[] = [
  { name: "System", base: "system", colors: {} },
  { name: "Dark", base: "dark", colors: {} },
  { name: "Light", base: "light", colors: {} },
  {
    name: "Dusk",
    base: "dark",
    colors: {
      accent: "#8a63e6",
      "surface-base": "#141219",
      "surface-sunken": "#0f0d13",
      "surface-raised": "#1b1822",
      "surface-overlay": "#221e2b",
      "surface-hover": "#2a2534",
      "surface-pressed": "#332d40",
      border: "#2a2533",
      "border-strong": "#3b3447",
      text: "#ebe7f3",
      "text-muted": "#a69eb7",
      "text-faint": "#746b86",
    },
  },
  {
    name: "High contrast",
    base: "dark",
    colors: {
      accent: "#ffd400",
      "accent-hover": "#ffe14d",
      "text-on-accent": "#000000",
      "surface-base": "#000000",
      "surface-sunken": "#000000",
      "surface-raised": "#0b0b0b",
      "surface-overlay": "#141414",
      "surface-hover": "#262626",
      "surface-pressed": "#333333",
      border: "#707070",
      "border-strong": "#a0a0a0",
      text: "#ffffff",
      "text-muted": "#dddddd",
      "text-faint": "#b0b0b0",
    },
  },
];

export const BUILTIN_THEMES: ThemeEntry[] = BUILTIN_DEFINITIONS.map((theme) => ({
  id: BUILTIN_PREFIX + theme.name.toLowerCase().replace(/\s+/g, "-"),
  theme,
  builtin: true,
  problems: [],
}));

const HEX_COLOR = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
const FUNCTION_COLOR = /^(rgb|rgba|hsl|hsla)\(\s*[\d.%\s,/+-]+\)$/i;

export function isColor(value: unknown): value is string {
  return typeof value === "string" && (HEX_COLOR.test(value) || FUNCTION_COLOR.test(value.trim()));
}

/** Reads a theme file's contents, keeping what is valid and listing what was ignored. */
export function parseTheme(
  raw: unknown,
  fallbackName: string,
): { theme: Theme; problems: string[] } {
  const problems: string[] = [];
  const source =
    raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const name =
    typeof source.name === "string" && source.name.trim() ? source.name.trim() : fallbackName;
  const base: ThemeBase =
    source.base === "light" || source.base === "system" ? source.base : "dark";
  if (source.base !== undefined && source.base !== base)
    problems.push(`Unknown base "${String(source.base)}"`);

  const colors: Record<string, string> = {};
  const rawColors = source.colors;
  if (rawColors && typeof rawColors === "object" && !Array.isArray(rawColors)) {
    for (const [key, value] of Object.entries(rawColors)) {
      if (!TOKEN_KEYS.has(key)) problems.push(`Unknown color "${key}"`);
      else if (!isColor(value)) problems.push(`"${key}" is not a color`);
      else colors[key] = value.trim();
    }
  } else if (rawColors !== undefined) {
    problems.push(`"colors" must be an object`);
  }

  const theme: Theme = { name, base, colors };
  if (source.fonts && typeof source.fonts === "object") {
    const fonts = source.fonts as Record<string, unknown>;
    theme.fonts = {
      ui: typeof fonts.ui === "string" ? fonts.ui : undefined,
      mono: typeof fonts.mono === "string" ? fonts.mono : undefined,
    };
  }
  if (typeof source.radius === "number" && source.radius >= 0 && source.radius <= 16) {
    theme.radius = source.radius;
  } else if (source.radius !== undefined) {
    problems.push(`"radius" must be a number from 0 to 16`);
  }
  return { theme, problems };
}

function hexChannels(color: string): [number, number, number] | null {
  if (!HEX_COLOR.test(color)) return null;
  let digits = color.slice(1);
  if (digits.length <= 4) digits = [...digits].map((digit) => digit + digit).join("");
  return [0, 2, 4].map((offset) => parseInt(digits.slice(offset, offset + 2), 16)) as [
    number,
    number,
    number,
  ];
}

function withAlpha(color: string, alpha: number): string | null {
  const channels = hexChannels(color);
  return channels ? `rgb(${channels.join(" ")} / ${alpha})` : null;
}

/** Moves a color toward white or black by `amount` (0 to 1). */
export function mixHex(color: string, toward: "white" | "black", amount: number): string {
  const channels = hexChannels(color);
  if (!channels) return color;
  const target = toward === "white" ? 255 : 0;
  return `#${channels
    .map((channel) => Math.round(channel + (target - channel) * amount))
    .map((channel) => channel.toString(16).padStart(2, "0"))
    .join("")}`;
}

/** Fills tokens that follow from the ones a theme sets, such as tints of its accent. */
/** Colors computed from another color unless a theme sets them itself. */
const DERIVED_COLORS: Record<string, readonly string[]> = {
  accent: ["accent-hover", "accent-soft", "accent-softer", "border-focus", "row-selected"],
  danger: ["danger-hover", "danger-soft"],
};

/** Sets one color, dropping colors derived from it so they follow the new value. */
export function withColor(theme: Theme, key: string, value: string): Theme {
  const colors = { ...theme.colors, [key]: value };
  for (const derived of DERIVED_COLORS[key] ?? []) delete colors[derived];
  return { ...theme, colors };
}

export function deriveColors(theme: Theme, prefersDark: boolean): Record<string, string> {
  const colors = { ...theme.colors };
  const dark = theme.base === "dark" || (theme.base === "system" && prefersDark);
  const derive = (key: string, value: string | null) => {
    if (value && !(key in theme.colors)) colors[key] = value;
  };
  const accent = theme.colors.accent;
  if (accent) {
    derive("accent-hover", mixHex(accent, dark ? "white" : "black", 0.12));
    derive("accent-soft", withAlpha(accent, dark ? 0.16 : 0.14));
    derive("accent-softer", withAlpha(accent, dark ? 0.09 : 0.07));
    derive("border-focus", accent);
    derive("row-selected", withAlpha(accent, dark ? 0.22 : 0.18));
  }
  const danger = theme.colors.danger;
  if (danger) {
    derive("danger-hover", mixHex(danger, dark ? "white" : "black", 0.1));
    derive("danger-soft", withAlpha(danger, dark ? 0.14 : 0.1));
  }
  return colors;
}

const BASE_PREVIEW = {
  dark: {
    "surface-base": "#111317",
    "surface-raised": "#181b20",
    text: "#e3e7ee",
    accent: "#3d7eff",
  },
  light: {
    "surface-base": "#f6f7f9",
    "surface-raised": "#ffffff",
    text: "#1b1f26",
    accent: "#2f6fea",
  },
};

/** The handful of colors a theme card shows. */
export function previewColors(theme: Theme, prefersDark: boolean): Record<string, string> {
  const dark = theme.base === "dark" || (theme.base === "system" && prefersDark);
  return { ...BASE_PREVIEW[dark ? "dark" : "light"], ...theme.colors };
}

/** `#abc` -> `#aabbcc`, for color inputs, which only take six digits. */
export function toInputColor(color: string): string | null {
  const channels = hexChannels(color.trim());
  if (channels)
    return `#${channels.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
  const match = color.trim().match(/^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i);
  if (!match) return null;
  return `#${match
    .slice(1, 4)
    .map((channel) => Math.min(255, Number(channel)).toString(16).padStart(2, "0"))
    .join("")}`;
}

/** A theme as written to its file, without empty optional parts. */
export function serializeTheme(theme: Theme): Record<string, unknown> {
  const output: Record<string, unknown> = {
    name: theme.name,
    base: theme.base,
    colors: theme.colors,
  };
  if (theme.fonts && (theme.fonts.ui || theme.fonts.mono)) output.fonts = theme.fonts;
  if (theme.radius !== undefined) output.radius = theme.radius;
  return output;
}
