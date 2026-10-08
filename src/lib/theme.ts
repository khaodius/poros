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
  /** Usually see-through, so the picker keeps the current transparency. */
  translucent: boolean;
}

const token = (key: string, label: string, group: string, translucent = false): ColorToken => ({
  key,
  label,
  group,
  translucent,
});

export const COLOR_TOKENS: ColorToken[] = [
  token("accent", "Accent", "Accent"),
  token("accent-hover", "Accent hover", "Accent"),
  token("accent-soft", "Accent tint", "Accent", true),
  token("accent-softer", "Faint accent tint", "Accent", true),
  token("text-on-accent", "Text on accent", "Accent"),
  token("surface-base", "Window background", "Surfaces"),
  token("surface-raised", "Panels", "Surfaces"),
  token("surface-sunken", "Inputs and lists", "Surfaces"),
  token("surface-overlay", "Menus and dialogs", "Surfaces"),
  token("surface-hover", "Hover", "Surfaces"),
  token("surface-pressed", "Pressed", "Surfaces"),
  token("titlebar", "Top bar", "Window"),
  token("statusbar", "Status bar", "Window"),
  token("list-header", "Column headers", "Window"),
  token("pane-border-active", "Active pane border", "Window"),
  token("tab-indicator", "Tab indicator", "Window"),
  token("border", "Borders", "Borders"),
  token("border-strong", "Strong borders", "Borders"),
  token("border-focus", "Focus ring", "Borders"),
  token("text", "Text", "Text"),
  token("text-muted", "Secondary text", "Text"),
  token("text-faint", "Faint text", "Text"),
  token("danger", "Danger", "Status"),
  token("danger-hover", "Danger hover", "Status"),
  token("danger-soft", "Danger tint", "Status", true),
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
  token("row-selected", "Selected row", "Lists", true),
  token("row-selected-inactive", "Selected row, inactive pane", "Lists", true),
  token("row-hover", "Row hover", "Lists", true),
  token("row-alternate", "Striped rows", "Lists", true),
  token("scrollbar", "Scrollbars", "Lists"),
  token("scrollbar-hover", "Scrollbar hover", "Lists"),
  token("progress", "Progress bars", "Transfers"),
  token("upload", "Uploads", "Transfers"),
  token("download", "Downloads", "Transfers"),
  token("diff-added", "Added lines", "Compare", true),
  token("diff-added-word", "Added words", "Compare", true),
  token("diff-removed", "Removed lines", "Compare", true),
  token("diff-removed-word", "Removed words", "Compare", true),
  token("backdrop", "Dialog backdrop", "Window", true),
];

export const COLOR_GROUPS = [...new Set(COLOR_TOKENS.map((entry) => entry.group))];

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
  "#5b6cf0",
  "#8a63e6",
  "#b05ce0",
  "#d1478f",
  "#e5484d",
  "#f07040",
  "#f0a33a",
  "#c9b21c",
  "#7cb342",
  "#3dbb76",
  "#14a8b8",
  "#3fa7ff",
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
    name: "Midnight",
    base: "dark",
    colors: {
      accent: "#3fa7ff",
      "surface-base": "#0b1220",
      "surface-sunken": "#080e19",
      "surface-raised": "#101a2c",
      "surface-overlay": "#152238",
      "surface-hover": "#1b2a44",
      "surface-pressed": "#22344f",
      border: "#1c2a40",
      "border-strong": "#2b3d59",
      text: "#dfe8f5",
      "text-muted": "#8fa3bf",
      "text-faint": "#5d708c",
      titlebar: "#0e1727",
    },
  },
  {
    name: "Forest",
    base: "dark",
    colors: {
      accent: "#4cc38a",
      "text-on-accent": "#06140d",
      "surface-base": "#0f1512",
      "surface-sunken": "#0b100d",
      "surface-raised": "#151d18",
      "surface-overlay": "#1a241e",
      "surface-hover": "#202c25",
      "surface-pressed": "#27362d",
      border: "#213028",
      "border-strong": "#2f4237",
      text: "#e2ebe5",
      "text-muted": "#98ab9f",
      "text-faint": "#66786c",
      "icon-folder": "#d9b25a",
      upload: "#e0b85a",
    },
    radius: 8,
  },
  {
    name: "Ember",
    base: "dark",
    colors: {
      accent: "#f08a4b",
      "text-on-accent": "#1a0d05",
      "surface-base": "#16120f",
      "surface-sunken": "#110e0b",
      "surface-raised": "#1d1814",
      "surface-overlay": "#241e19",
      "surface-hover": "#2b241e",
      "surface-pressed": "#352c25",
      border: "#2c241e",
      "border-strong": "#3e342b",
      text: "#f0e7df",
      "text-muted": "#b3a497",
      "text-faint": "#7f7064",
      "icon-folder": "#e8a64b",
    },
  },
  {
    name: "Paper",
    base: "light",
    colors: {
      accent: "#0f8b8d",
      "surface-base": "#f5f1ea",
      "surface-sunken": "#ebe5db",
      "surface-raised": "#fbf8f3",
      "surface-overlay": "#ffffff",
      "surface-hover": "#ece6dc",
      "surface-pressed": "#e1d9cc",
      border: "#e0d8ca",
      "border-strong": "#cbbfad",
      text: "#2b2620",
      "text-muted": "#6b6156",
      "text-faint": "#9a8f82",
      titlebar: "#efe9df",
      "icon-folder": "#c48a1c",
    },
    radius: 8,
  },
  {
    name: "Graphite",
    base: "dark",
    colors: {
      accent: "#a0a8b8",
      "text-on-accent": "#111316",
      "surface-base": "#151618",
      "surface-sunken": "#101113",
      "surface-raised": "#1b1c1f",
      "surface-overlay": "#222326",
      "surface-hover": "#28292d",
      "surface-pressed": "#303136",
      border: "#2a2b2f",
      "border-strong": "#3a3b40",
      text: "#e6e6e8",
      "text-muted": "#a2a3a8",
      "text-faint": "#6e6f75",
      progress: "#3dbb76",
      upload: "#6aa6ff",
    },
    radius: 2,
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

const SLASH_ALPHA = /\/\s*([\d.]+)(%?)\s*\)$/;
const COMMA_ALPHA = /^(?:rgb|hsl)a\((?:[^,]+,){3}\s*([\d.]+)(%?)\s*\)$/i;

/** The opacity of a color written with an alpha channel, or 1. */
export function alphaOf(color: string): number {
  const value = color.trim();
  if (HEX_COLOR.test(value) && (value.length === 5 || value.length === 9)) {
    const digits = value.length === 5 ? value[4] + value[4] : value.slice(7, 9);
    return Math.round((parseInt(digits, 16) / 255) * 100) / 100;
  }
  const match = value.match(SLASH_ALPHA) ?? value.match(COMMA_ALPHA);
  if (!match) return 1;
  const alpha = Number(match[1]) / (match[2] ? 100 : 1);
  return Number.isFinite(alpha) ? Math.min(1, Math.max(0, alpha)) : 1;
}

/** A picked solid color with the transparency `previous` had. */
export function keepAlpha(picked: string, previous: string): string {
  const alpha = alphaOf(previous);
  return alpha < 1 ? (withAlpha(picked, alpha) ?? picked) : picked;
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

/** Removes a color so the base palette, or the color it derives from, decides it. */
export function withoutColor(theme: Theme, key: string): Theme {
  const colors = { ...theme.colors };
  delete colors[key];
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
    derive("diff-removed", withAlpha(danger, dark ? 0.12 : 0.09));
    derive("diff-removed-word", withAlpha(danger, dark ? 0.32 : 0.22));
  }
  const success = theme.colors.success;
  if (success) {
    derive("diff-added", withAlpha(success, dark ? 0.12 : 0.1));
    derive("diff-added-word", withAlpha(success, dark ? 0.32 : 0.24));
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

/** The red, green and blue of a computed style color such as `rgb(17, 19, 23)`. */
export function rgbChannels(color: string): [number, number, number] | null {
  const match = color.trim().match(/^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i);
  if (!match) return null;
  return match.slice(1, 4).map((channel) => Math.min(255, Math.round(Number(channel)))) as [
    number,
    number,
    number,
  ];
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
