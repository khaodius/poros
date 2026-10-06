import { create } from "zustand";
import { themeFiles } from "../lib/ipc";
import {
  BUILTIN_THEMES,
  deriveColors,
  parseTheme,
  serializeTheme,
  COLOR_TOKENS,
  type Theme,
  type ThemeEntry,
} from "../lib/theme";
import { quoteFamily, type FontKind } from "../lib/fonts";
import type { AppearanceSettings } from "../lib/settings";
import { saveSettingsSection, useSettingsStore } from "./settingsStore";

const SAVE_DELAY_MILLIS = 300;

interface ThemeState {
  /** User themes, read from the themes folder. */
  files: ThemeEntry[];
  load: () => Promise<void>;
  /** Shows an edit at once and writes it to the theme file shortly after. */
  replaceFile: (id: string, theme: Theme) => void;
}

const pendingSaves = new Map<string, number>();

export const useThemeStore = create<ThemeState>((set, get) => ({
  files: [],
  load: async () => {
    const listed = await themeFiles.list().catch(() => []);
    const current = new Map(get().files.map((entry) => [entry.id, entry]));
    set({
      files: listed.map((file) => {
        // An edit not yet written is newer than the file.
        const unsaved = pendingSaves.has(file.id) ? current.get(file.id) : undefined;
        if (unsaved) return unsaved;
        if (file.error) {
          return {
            id: file.id,
            path: file.path,
            builtin: false,
            theme: { name: file.id, base: "dark", colors: {} },
            problems: [file.error],
          };
        }
        const { theme, problems } = parseTheme(file.theme, file.id);
        return { id: file.id, path: file.path, builtin: false, theme, problems };
      }),
    });
  },
  replaceFile: (id, theme) => {
    set((state) => ({
      files: state.files.map((entry) =>
        entry.id === id ? { ...entry, theme, problems: [] } : entry,
      ),
    }));
    window.clearTimeout(pendingSaves.get(id));
    pendingSaves.set(
      id,
      window.setTimeout(() => {
        pendingSaves.delete(id);
        void themeFiles.save(id, serializeTheme(theme));
      }, SAVE_DELAY_MILLIS),
    );
  },
}));

export function allThemes(files: ThemeEntry[]): ThemeEntry[] {
  return [...BUILTIN_THEMES, ...files];
}

export function findTheme(files: ThemeEntry[], id: string): ThemeEntry {
  return allThemes(files).find((entry) => entry.id === id) ?? BUILTIN_THEMES[0];
}

let copying: Promise<void> | null = null;

/** Applies an edit to the active theme. Built-in themes are copied to a new file first. */
export async function editActiveTheme(change: (theme: Theme) => Theme): Promise<void> {
  // Edits made while a built-in theme is being copied go to the copy.
  while (copying) await copying;
  const { files, replaceFile, load } = useThemeStore.getState();
  const active = findTheme(files, useSettingsStore.getState().settings.appearance.theme);
  if (!active.builtin) {
    replaceFile(active.id, change(active.theme));
    return;
  }
  const copy = change({ ...active.theme, name: `${active.theme.name} (custom)` });
  copying = (async () => {
    const id = await themeFiles.save(null, serializeTheme(copy));
    await load();
    await saveSettingsSection("appearance", { theme: id });
  })();
  try {
    await copying;
  } finally {
    copying = null;
  }
}

const FONT_SIZE_STEPS = { small: 1, tiny: 2 };

/** A font family name in front of the default stack, so missing glyphs still show. */
export function fontStack(family: string, kind: FontKind): string {
  return `${quoteFamily(family)}, var(--font-${kind}-default)`;
}

/**
 * Sets the theme's custom properties on the document, clearing the previous theme's. Fonts and
 * corner radius picked in the settings win over the theme's.
 */
export function applyTheme(theme: Theme, appearance: AppearanceSettings): void {
  const root = document.documentElement;
  if (theme.base === "system") delete root.dataset.theme;
  else root.dataset.theme = theme.base;

  const style = root.style;
  for (const entry of COLOR_TOKENS) style.removeProperty(`--${entry.key}`);
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  for (const [key, value] of Object.entries(deriveColors(theme, prefersDark))) {
    style.setProperty(`--${key}`, value);
  }

  const setOptional = (property: string, value: string | undefined) => {
    if (value) style.setProperty(property, value);
    else style.removeProperty(property);
  };
  setOptional(
    "--font-ui",
    appearance.uiFont ? fontStack(appearance.uiFont, "ui") : theme.fonts?.ui,
  );
  setOptional(
    "--font-mono",
    appearance.monoFont ? fontStack(appearance.monoFont, "mono") : theme.fonts?.mono,
  );
  const radius = appearance.radius ?? theme.radius;
  if (radius === undefined) {
    for (const property of ["--radius-small", "--radius", "--radius-large"]) {
      style.removeProperty(property);
    }
  } else {
    style.setProperty("--radius-small", `${Math.round(radius * 0.67)}px`);
    style.setProperty("--radius", `${radius}px`);
    style.setProperty("--radius-large", `${Math.round(radius * 1.67)}px`);
  }
  const { fontSize } = appearance;
  style.setProperty("--font-size", `${fontSize}px`);
  style.setProperty("--font-size-small", `${fontSize - FONT_SIZE_STEPS.small}px`);
  style.setProperty("--font-size-tiny", `${fontSize - FONT_SIZE_STEPS.tiny}px`);
}
