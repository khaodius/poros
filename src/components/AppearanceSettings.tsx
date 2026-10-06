import { useEffect, useState } from "react";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { Check, Download, FolderOpen, RotateCcw, Trash2, Upload, X } from "lucide-react";
import { useInstalledFonts } from "../hooks/useInstalledFonts";
import { RECOMMENDED_FONTS, type FontKind, type FontOption } from "../lib/fonts";
import { files, themeFiles, toAppError } from "../lib/ipc";
import {
  DEFAULT_SETTINGS,
  FONT_SIZE_LIMITS,
  RADIUS_LIMITS,
  ROW_HEIGHT_LIMITS,
  type AppearanceSettings as Appearance,
} from "../lib/settings";
import {
  ACCENT_PRESETS,
  COLOR_GROUPS,
  COLOR_TOKENS,
  SIMPLE_COLOR_KEYS,
  isColor,
  keepAlpha,
  previewColors,
  serializeTheme,
  toInputColor,
  withColor,
  withoutColor,
  type ColorToken,
  type ThemeBase,
  type ThemeEntry,
} from "../lib/theme";
import {
  previewSettingsSection,
  saveSettingsSection,
  useSettingsStore,
} from "../state/settingsStore";
import { allThemes, editActiveTheme, findTheme, useThemeStore } from "../state/themeStore";
import { FontPicker } from "./FontPicker";
import {
  RangeSetting,
  SelectSetting,
  SettingGroup,
  SettingRow,
  SwitchSetting,
} from "./settingsFields";

const TOKENS = new Map(COLOR_TOKENS.map((token) => [token.key, token]));
const BASES: { value: ThemeBase; label: string }[] = [
  { value: "dark", label: "Dark" },
  { value: "light", label: "Light" },
  { value: "system", label: "Follow the system" },
];
/** The radius the stylesheet uses when neither the theme nor the settings set one. */
const DEFAULT_RADIUS = 6;
const pixels = (value: number) => `${value} px`;
/** Everything on this page that is not part of the theme. */
const LOOK_KEYS = (Object.keys(DEFAULT_SETTINGS.appearance) as (keyof Appearance)[]).filter(
  (key) => key !== "theme",
);
const DEFAULT_LOOK: Partial<Appearance> = Object.fromEntries(
  LOOK_KEYS.map((key) => [key, DEFAULT_SETTINGS.appearance[key]]),
);

function prefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Every color token's value on the page right now, after the theme is applied. */
function useAppliedColors(dependency: unknown): Record<string, string> {
  const [applied, setApplied] = useState<Record<string, string>>({});
  useEffect(() => {
    // The theme is applied in an effect of the app, which runs after this page's effects.
    const frame = requestAnimationFrame(() => {
      const style = getComputedStyle(document.documentElement);
      setApplied(
        Object.fromEntries(
          COLOR_TOKENS.map((token) => [token.key, style.getPropertyValue(`--${token.key}`).trim()]),
        ),
      );
    });
    return () => cancelAnimationFrame(frame);
  }, [dependency]);
  return applied;
}

function missingPick(fonts: FontOption[] | null, kind: FontKind): string | undefined {
  const pick = RECOMMENDED_FONTS[kind][0];
  if (!fonts || fonts.some((option) => option.name.toLowerCase() === pick.toLowerCase())) {
    return undefined;
  }
  return `${pick} is free and made for screens; install it to see it here.`;
}

export function AppearanceSettings() {
  const themeFilesList = useThemeStore((state) => state.files);
  const loadThemes = useThemeStore((state) => state.load);
  const appearance = useSettingsStore((state) => state.settings.appearance);
  const [error, setError] = useState<string | null>(null);
  const fonts = useInstalledFonts();
  const active = findTheme(themeFilesList, appearance.theme);
  const colors = active.theme.colors;
  const applied = useAppliedColors(active.theme);

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
  const resetColor = (key: string) =>
    void run(() => editActiveTheme((theme) => withoutColor(theme, key)));
  const setAppearance = (change: Partial<Appearance>) =>
    void saveSettingsSection("appearance", change);
  const previewAppearance = (change: Partial<Appearance>) =>
    previewSettingsSection("appearance", change);

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

  const lookChanged = LOOK_KEYS.some((key) => appearance[key] !== DEFAULT_LOOK[key]);

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
              value={toInputColor(colors.accent ?? applied.accent ?? "") ?? "#000000"}
              onChange={(event) => setColor("accent", event.target.value)}
            />
          </div>
        </SettingRow>
        <div className="color-grid">
          {SIMPLE_COLOR_KEYS.map((key) => (
            <label key={key} className="color-field">
              <input
                type="color"
                value={toInputColor(colors[key] ?? applied[key] ?? "") ?? "#000000"}
                onChange={(event) => setColor(key, event.target.value)}
              />
              <span>{TOKENS.get(key)?.label}</span>
              {colors[key] && <span className="color-set" title="Set by this theme" />}
            </label>
          ))}
        </div>
        <details className="all-colors">
          <summary>All colors</summary>
          {COLOR_GROUPS.map((group) => (
            <div key={group} className="color-token-group">
              <h4>{group}</h4>
              {COLOR_TOKENS.filter((token) => token.group === group).map((token) => (
                <ColorTokenRow
                  key={`${active.id}-${token.key}`}
                  token={token}
                  value={colors[token.key]}
                  applied={applied[token.key] ?? ""}
                  onChange={(value) => setColor(token.key, value)}
                  onReset={() => resetColor(token.key)}
                />
              ))}
            </div>
          ))}
        </details>
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
      </SettingGroup>

      <SettingGroup title="Text">
        <SettingRow label="Interface font" hint={missingPick(fonts, "ui")}>
          <FontPicker
            kind="ui"
            label="Interface font"
            value={appearance.uiFont}
            fonts={fonts}
            onChange={(uiFont) => setAppearance({ uiFont })}
          />
        </SettingRow>
        <SettingRow
          label="Monospace font"
          hint={missingPick(fonts, "mono") ?? "Used for paths, permissions and the log."}
        >
          <FontPicker
            kind="mono"
            label="Monospace font"
            value={appearance.monoFont}
            fonts={fonts}
            onChange={(monoFont) => setAppearance({ monoFont })}
          />
        </SettingRow>
        <RangeSetting
          label="Font size"
          hint="Ctrl and + or -, or Ctrl and the mouse wheel, change it anywhere."
          value={appearance.fontSize}
          min={FONT_SIZE_LIMITS.min}
          max={FONT_SIZE_LIMITS.max}
          format={pixels}
          onPreview={(fontSize) => previewAppearance({ fontSize })}
          onCommit={(fontSize) => setAppearance({ fontSize })}
        />
      </SettingGroup>

      <SettingGroup title="Shape">
        <RangeSetting
          label="Corner roundness"
          hint={
            appearance.radius === null
              ? `Following the theme${active.theme.radius === undefined ? "" : ` (${active.theme.radius} px)`}.`
              : undefined
          }
          value={appearance.radius ?? active.theme.radius ?? DEFAULT_RADIUS}
          min={RADIUS_LIMITS.min}
          max={RADIUS_LIMITS.max}
          format={pixels}
          onPreview={(radius) => previewAppearance({ radius })}
          onCommit={(radius) => setAppearance({ radius })}
          onReset={appearance.radius === null ? undefined : () => setAppearance({ radius: null })}
          resetLabel="Use the theme's roundness"
        />
        <RangeSetting
          label="File list row height"
          value={appearance.rowHeight}
          min={ROW_HEIGHT_LIMITS.min}
          max={ROW_HEIGHT_LIMITS.max}
          format={pixels}
          onPreview={(rowHeight) => previewAppearance({ rowHeight })}
          onCommit={(rowHeight) => setAppearance({ rowHeight })}
        />
        <SwitchSetting
          label="Striped rows"
          hint="Shades every other row in file lists."
          checked={appearance.stripedRows}
          onChange={(stripedRows) => setAppearance({ stripedRows })}
        />
        <div className="setting-actions">
          <button
            type="button"
            className="button"
            disabled={!lookChanged}
            onClick={() => setAppearance(DEFAULT_LOOK)}
          >
            <RotateCcw size={14} />
            Reset fonts, sizes and shape
          </button>
        </div>
      </SettingGroup>
    </>
  );
}

interface ColorTokenRowProps {
  token: ColorToken;
  /** What the theme sets, if anything. */
  value: string | undefined;
  /** What the page shows now. */
  applied: string;
  onChange: (value: string) => void;
  onReset: () => void;
}

function ColorTokenRow({ token, value, applied, onChange, onReset }: ColorTokenRowProps) {
  const [typed, setTyped] = useState<string | null>(null);
  const shown = value ?? applied;
  const commitTyped = () => {
    const next = typed?.trim();
    setTyped(null);
    if (next === undefined || next === (value ?? "")) return;
    if (next === "") onReset();
    else if (isColor(next)) onChange(next);
  };
  return (
    <div className="color-token">
      <input
        type="color"
        aria-label={token.label}
        value={toInputColor(shown) ?? "#000000"}
        onChange={(event) =>
          onChange(token.translucent ? keepAlpha(event.target.value, shown) : event.target.value)
        }
      />
      <span className="color-token-label">{token.label}</span>
      <input
        className={`color-token-value ${typed !== null && typed.trim() && !isColor(typed.trim()) ? "has-error" : ""}`}
        value={typed ?? value ?? ""}
        placeholder={applied}
        spellCheck={false}
        aria-label={`${token.label} value`}
        onChange={(event) => setTyped(event.target.value)}
        onBlur={commitTyped}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          else if (event.key === "Escape" && typed !== null) {
            event.preventDefault();
            event.stopPropagation();
            setTyped(null);
          }
        }}
      />
      <button
        type="button"
        className="icon-button"
        title="Use the base color"
        aria-label={`Reset ${token.label.toLowerCase()}`}
        disabled={value === undefined}
        onClick={onReset}
      >
        <X size={13} />
      </button>
    </div>
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
      <span
        className="theme-preview"
        style={{
          background: colors["surface-base"],
          borderRadius: entry.theme.radius === undefined ? undefined : entry.theme.radius,
        }}
      >
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
