import { useMemo, useState } from "react";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { Check, Download, FolderOpen, RotateCcw, Trash2, Upload } from "lucide-react";
import { files, themeFiles, toAppError } from "../lib/ipc";
import { FONT_SIZES } from "../lib/settings";
import {
  ACCENT_PRESETS,
  COLOR_TOKENS,
  SIMPLE_COLOR_KEYS,
  previewColors,
  serializeTheme,
  toInputColor,
  withColor,
  type ThemeBase,
  type ThemeEntry,
} from "../lib/theme";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { allThemes, editActiveTheme, findTheme, useThemeStore } from "../state/themeStore";
import { SelectSetting, SettingGroup, SettingRow } from "./settingsFields";

const TOKEN_LABELS = new Map(COLOR_TOKENS.map((token) => [token.key, token.label]));
const BASES: { value: ThemeBase; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "Follow the system" },
];

function prefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** The color a token has right now, for color pickers of tokens the theme leaves alone. */
function currentColor(key: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(`--${key}`);
  return toInputColor(value) ?? "#000000";
}

export function AppearanceSettings() {
  const themeFilesList = useThemeStore((state) => state.files);
  const loadThemes = useThemeStore((state) => state.load);
  const appearance = useSettingsStore((state) => state.settings.appearance);
  const [error, setError] = useState<string | null>(null);
  const active = findTheme(themeFilesList, appearance.theme);
  const colors = active.theme.colors;
  const shownColors = useMemo(
    () =>
      Object.fromEntries(
        SIMPLE_COLOR_KEYS.map((key) => [key, toInputColor(colors[key] ?? "") ?? currentColor(key)]),
      ),
    [colors],
  );

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
    } catch (caught) {
      setError(toAppError(caught).message);
    }
  };

  const select = (id: string) => void saveSettingsSection("appearance", { theme: id });
  const setColor = (key: string, value: string) =>
    void run(() => editActiveTheme((theme) => withColor(theme, key, value)));

  const importTheme = () =>
    run(async () => {
      const path = await openFileDialog({
        title: "Import theme",
        multiple: false,
        directory: false,
        filters: [{ name: "Poros theme", extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      const id = await themeFiles.importFile(path);
      await loadThemes();
      select(id);
    });

  const exportTheme = () =>
    run(async () => {
      const path = await saveFileDialog({
        title: "Export theme",
        defaultPath: `${active.theme.name.replace(/[^\w -]+/g, "").trim() || "theme"}.json`,
        filters: [{ name: "Poros theme", extensions: ["json"] }],
      });
      if (path) {
        await files.saveText(path, `${JSON.stringify(serializeTheme(active.theme), null, 2)}\n`);
      }
    });

  const deleteTheme = () =>
    run(async () => {
      await themeFiles.remove(active.id);
      await loadThemes();
      select("builtin:system");
    });

  return (
    <>
      <SettingGroup title="Theme">
        <div className="theme-grid">
          {allThemes(themeFilesList).map((entry) => (
            <ThemeCard
              key={entry.id}
              entry={entry}
              selected={entry.id === active.id}
              onSelect={() => select(entry.id)}
            />
          ))}
        </div>
        <div className="theme-actions">
          <button type="button" className="button" onClick={() => void importTheme()}>
            <Upload size={14} />
            Import theme file
          </button>
          <button type="button" className="button" onClick={() => void exportTheme()}>
            <Download size={14} />
            Export
          </button>
          <button
            type="button"
            className="button"
            onClick={() => void run(() => themeFiles.openFolder())}
          >
            <FolderOpen size={14} />
            Open themes folder
          </button>
          {!active.builtin && (
            <button type="button" className="button" onClick={() => void deleteTheme()}>
              <Trash2 size={14} />
              Delete
            </button>
          )}
        </div>
        {active.problems.length > 0 && (
          <ul className="theme-problems">
            {active.problems.map((problem) => (
              <li key={problem}>{problem}</li>
            ))}
          </ul>
        )}
        {error && <p className="form-error">{error}</p>}
      </SettingGroup>

      <SettingGroup title="Colors">
        {active.builtin && (
          <p className="setting-note">
            Changing a color saves a copy of {active.theme.name} as your own theme file.
          </p>
        )}
        {!active.builtin && (
          <SettingRow label="Name">
            <input
              key={active.id}
              defaultValue={active.theme.name}
              spellCheck={false}
              onBlur={(event) => {
                const name = event.target.value.trim();
                if (name && name !== active.theme.name) {
                  void run(() => editActiveTheme((theme) => ({ ...theme, name })));
                }
              }}
            />
          </SettingRow>
        )}
        {!active.builtin && (
          <SelectSetting
            label="Based on"
            value={active.theme.base}
            options={BASES}
            onChange={(base) => void run(() => editActiveTheme((theme) => ({ ...theme, base })))}
          />
        )}
        <SettingRow label="Accent">
          <div className="swatches">
            {ACCENT_PRESETS.map((preset) => (
              <button
                key={preset}
                type="button"
                className="swatch"
                style={{ background: preset }}
                aria-label={`Accent ${preset}`}
                aria-pressed={colors.accent === preset}
                onClick={() => setColor("accent", preset)}
              >
                {colors.accent === preset && <Check size={12} />}
              </button>
            ))}
            <input
              type="color"
              className="swatch-picker"
              aria-label="Custom accent"
              value={toInputColor(colors.accent ?? "") ?? currentColor("accent")}
              onChange={(event) => setColor("accent", event.target.value)}
            />
          </div>
        </SettingRow>
        <div className="color-grid">
          {SIMPLE_COLOR_KEYS.map((key) => (
            <label key={key} className="color-field">
              <input
                type="color"
                value={shownColors[key]}
                onChange={(event) => setColor(key, event.target.value)}
              />
              <span>{TOKEN_LABELS.get(key)}</span>
              {colors[key] && <span className="color-set" title="Set by this theme" />}
            </label>
          ))}
        </div>
        {!active.builtin && Object.keys(colors).length > 0 && (
          <button
            type="button"
            className="button"
            onClick={() => void run(() => editActiveTheme((theme) => ({ ...theme, colors: {} })))}
          >
            <RotateCcw size={14} />
            Reset colors to {active.theme.base === "light" ? "Light" : "Dark"}
          </button>
        )}
        <p className="setting-note">
          Every color, fonts and corner radius can be set in the theme file. The README lists the
          keys.
        </p>
      </SettingGroup>

      <SettingGroup title="Text">
        <SelectSetting
          label="Font size"
          value={String(appearance.fontSize)}
          options={FONT_SIZES.map((size) => ({ value: String(size), label: `${size} px` }))}
          onChange={(size) => void saveSettingsSection("appearance", { fontSize: Number(size) })}
        />
      </SettingGroup>
    </>
  );
}

function ThemeCard({
  entry,
  selected,
  onSelect,
}: {
  entry: ThemeEntry;
  selected: boolean;
  onSelect: () => void;
}) {
  const colors = previewColors(entry.theme, prefersDark());
  return (
    <button
      type="button"
      className={`theme-card ${selected ? "is-selected" : ""}`}
      aria-pressed={selected}
      onClick={onSelect}
    >
      <span className="theme-preview" style={{ background: colors["surface-base"] }}>
        <span className="theme-preview-panel" style={{ background: colors["surface-raised"] }}>
          <span className="theme-preview-line" style={{ background: colors.text }} />
          <span className="theme-preview-line short" style={{ background: colors.text }} />
          <span className="theme-preview-accent" style={{ background: colors.accent }} />
        </span>
      </span>
      <span className="theme-card-name">
        {entry.theme.name}
        {entry.problems.length > 0 && <span className="theme-card-warning"> (has problems)</span>}
      </span>
    </button>
  );
}
