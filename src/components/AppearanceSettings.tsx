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
  themeName,
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
import {
  allThemes,
  editActiveTheme,
  findTheme,
  previewColor,
  useThemeStore,
} from "../state/themeStore";
import { ColorInput } from "./ColorInput";
import { FontPicker } from "./FontPicker";
import { AnimationGroup, ProgressBarGroup } from "./TransferLookSettings";
import {
  RangeSetting,
  SelectSetting,
  SettingGroup,
  SettingRow,
  SwitchSetting,
} from "./settingsFields";
import { t, useT, type MessageKey } from "../i18n";

const TOKENS = new Map(COLOR_TOKENS.map((token) => [token.key, token]));
const BASES: { value: ThemeBase; label: string }[] = [
  {
    value: "dark",
    get label() {
      return t("appearance.dark");
    },
  },
  {
    value: "light",
    get label() {
      return t("appearance.light");
    },
  },
  {
    value: "system",
    get label() {
      return t("appearance.followTheSystem");
    },
  },
];
/** The radius the stylesheet uses when neither the theme nor the settings set one. */
const DEFAULT_RADIUS = 6;
const pixels = (value: number) => `${value} px`;
/** The fonts, sizes and shape on this page, which one button puts back. */
const LOOK_KEYS = (Object.keys(DEFAULT_SETTINGS.appearance) as (keyof Appearance)[]).filter(
  (key) =>
    ![
      "theme",
      "systemIcons",
      "smoothScrolling",
      "groupLocalByDate",
      "groupRemoteByDate",
      "progressBars",
      "transferAnimation",
    ].includes(key),
);
/** How the system's icons are described: whose they are depends on the system. */
const SYSTEM_ICONS_HINT: MessageKey = navigator.userAgent.includes("Windows")
  ? "appearance.systemIconsHintWindows"
  : "appearance.systemIconsHintLinux";
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
  return t("appearance.recommendedFontMissing", { font: pick });
}

export function AppearanceSettings() {
  const t = useT();
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
        title: t("appearance.importTheme"),
        multiple: false,
        directory: false,
        filters: [{ name: t("appearance.themeFileType"), extensions: ["json"] }],
      });
      if (typeof path !== "string") return;
      const id = await themeFiles.importFile(path);
      await loadThemes();
      select(id);
    });

  const exportTheme = () =>
    run(async () => {
      const path = await saveFileDialog({
        title: t("appearance.exportTheme"),
        defaultPath: `${active.theme.name.replace(/[^\w -]+/g, "").trim() || "theme"}.json`,
        filters: [{ name: t("appearance.themeFileType"), extensions: ["json"] }],
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
      <SettingGroup title={t("appearance.theme")}>
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
            {t("appearance.importThemeFile")}
          </button>
          <button type="button" className="button" onClick={() => void exportTheme()}>
            <Download size={14} />
            {t("appearance.export")}
          </button>
          <button
            type="button"
            className="button"
            onClick={() => void run(() => themeFiles.openFolder())}
          >
            <FolderOpen size={14} />
            {t("appearance.openThemesFolder")}
          </button>
          {!active.builtin && (
            <button type="button" className="button" onClick={() => void deleteTheme()}>
              <Trash2 size={14} />
              {t("common.delete")}
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

      <SettingGroup title={t("appearance.colors")}>
        {active.builtin && (
          <p className="setting-note">
            {t("appearance.changingAColorSavesA", { name: themeName(active) })}
          </p>
        )}
        {!active.builtin && (
          <SettingRow label={t("common.name")}>
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
            label={t("appearance.basedOn")}
            value={active.theme.base}
            options={BASES}
            onChange={(base) => void run(() => editActiveTheme((theme) => ({ ...theme, base })))}
          />
        )}
        <SettingRow label={t("appearance.accent")}>
          <div className="swatches">
            {ACCENT_PRESETS.map((preset) => (
              <button
                key={preset}
                type="button"
                className="swatch"
                style={{ background: preset }}
                aria-label={t("appearance.accent2", { preset })}
                aria-pressed={colors.accent === preset}
                onClick={() => setColor("accent", preset)}
              >
                {colors.accent === preset && <Check size={12} />}
              </button>
            ))}
            <ColorInput
              className="swatch-picker"
              aria-label={t("appearance.customAccent")}
              value={toInputColor(colors.accent ?? applied.accent ?? "") ?? "#000000"}
              onPreview={(color) => previewColor("accent", color)}
              onCommit={(color) => setColor("accent", color)}
            />
          </div>
        </SettingRow>
        <div className="color-grid">
          {SIMPLE_COLOR_KEYS.map((key) => (
            <label key={key} className="color-field">
              <ColorInput
                value={toInputColor(colors[key] ?? applied[key] ?? "") ?? "#000000"}
                onPreview={(color) => previewColor(key, color)}
                onCommit={(color) => setColor(key, color)}
              />
              <span>{t(TOKENS.get(key)!.label)}</span>
              {colors[key] && <span className="color-set" title={t("appearance.setByThisTheme")} />}
            </label>
          ))}
        </div>
        <details className="all-colors">
          <summary>{t("appearance.allColors")}</summary>
          {COLOR_GROUPS.map((group) => (
            <div key={group} className="color-token-group">
              <h4>{t(group)}</h4>
              {COLOR_TOKENS.filter((token) => token.group === group).map((token) => (
                <ColorTokenRow
                  key={`${active.id}-${token.key}`}
                  token={token}
                  value={colors[token.key]}
                  applied={applied[token.key] ?? ""}
                  onPreview={(value) => previewColor(token.key, value)}
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
            {active.theme.base === "light"
              ? t("appearance.resetColorsToLight")
              : t("appearance.resetColorsToDark")}
          </button>
        )}
      </SettingGroup>

      <SettingGroup title={t("appearance.text")}>
        <SettingRow label={t("appearance.interfaceFont")} hint={missingPick(fonts, "ui")}>
          <FontPicker
            kind="ui"
            label={t("appearance.interfaceFont")}
            value={appearance.uiFont}
            fonts={fonts}
            onChange={(uiFont) => setAppearance({ uiFont })}
          />
        </SettingRow>
        <SettingRow
          label={t("appearance.monospaceFont")}
          hint={missingPick(fonts, "mono") ?? t("appearance.usedForPathsPermissionsAnd")}
        >
          <FontPicker
            kind="mono"
            label={t("appearance.monospaceFont")}
            value={appearance.monoFont}
            fonts={fonts}
            onChange={(monoFont) => setAppearance({ monoFont })}
          />
        </SettingRow>
        <RangeSetting
          label={t("appearance.fontSize")}
          hint={t("appearance.fontSizeHint")}
          value={appearance.fontSize}
          min={FONT_SIZE_LIMITS.min}
          max={FONT_SIZE_LIMITS.max}
          format={pixels}
          onPreview={(fontSize) => previewAppearance({ fontSize })}
          onCommit={(fontSize) => setAppearance({ fontSize })}
        />
      </SettingGroup>

      <SettingGroup title={t("appearance.shape")}>
        <RangeSetting
          label={t("appearance.cornerRoundness")}
          hint={
            appearance.radius === null
              ? active.theme.radius === undefined
                ? t("appearance.followingTheTheme")
                : t("appearance.followingTheThemeRadius", { radius: active.theme.radius })
              : undefined
          }
          value={appearance.radius ?? active.theme.radius ?? DEFAULT_RADIUS}
          min={RADIUS_LIMITS.min}
          max={RADIUS_LIMITS.max}
          format={pixels}
          onPreview={(radius) => previewAppearance({ radius })}
          onCommit={(radius) => setAppearance({ radius })}
          onReset={appearance.radius === null ? undefined : () => setAppearance({ radius: null })}
          resetLabel={t("appearance.useTheThemesRoundness")}
        />
        <RangeSetting
          label={t("appearance.fileListRowHeight")}
          value={appearance.rowHeight}
          min={ROW_HEIGHT_LIMITS.min}
          max={ROW_HEIGHT_LIMITS.max}
          format={pixels}
          onPreview={(rowHeight) => previewAppearance({ rowHeight })}
          onCommit={(rowHeight) => setAppearance({ rowHeight })}
        />
        <SwitchSetting
          label={t("appearance.stripedRows")}
          hint={t("appearance.stripedRowsHint")}
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
            {t("appearance.resetFontsSizesAndShape")}
          </button>
        </div>
      </SettingGroup>

      <SettingGroup title={t("appearance.iconsAndScrolling")}>
        <SwitchSetting
          label={t("appearance.systemFileIcons")}
          hint={t(SYSTEM_ICONS_HINT)}
          checked={appearance.systemIcons}
          onChange={(systemIcons) => setAppearance({ systemIcons })}
        />
        <SwitchSetting
          label={t("appearance.smoothScrolling")}
          hint={t("appearance.smoothScrollingHint")}
          checked={appearance.smoothScrolling}
          onChange={(smoothScrolling) => setAppearance({ smoothScrolling })}
        />
      </SettingGroup>

      <SettingGroup title={t("appearance.grouping")}>
        <SwitchSetting
          label={t("appearance.groupLocalFilesByDate")}
          hint={t("appearance.groupLocalFilesByDateHint")}
          checked={appearance.groupLocalByDate}
          onChange={(groupLocalByDate) => setAppearance({ groupLocalByDate })}
        />
        <SwitchSetting
          label={t("appearance.groupServerFilesByDate")}
          hint={t("appearance.groupServerFilesByDateHint")}
          checked={appearance.groupRemoteByDate}
          onChange={(groupRemoteByDate) => setAppearance({ groupRemoteByDate })}
        />
      </SettingGroup>

      <ProgressBarGroup />
      <AnimationGroup />
    </>
  );
}

interface ColorTokenRowProps {
  token: ColorToken;
  /** What the theme sets, if anything. */
  value: string | undefined;
  /** What the page shows now. */
  applied: string;
  onPreview: (value: string) => void;
  onChange: (value: string) => void;
  onReset: () => void;
}

function ColorTokenRow({
  token,
  value,
  applied,
  onPreview,
  onChange,
  onReset,
}: ColorTokenRowProps) {
  const t = useT();
  const [typed, setTyped] = useState<string | null>(null);
  const shown = value ?? applied;
  const picked = (color: string) => (token.translucent ? keepAlpha(color, shown) : color);
  const commitTyped = () => {
    const next = typed?.trim();
    setTyped(null);
    if (next === undefined || next === (value ?? "")) return;
    if (next === "") onReset();
    else if (isColor(next)) onChange(next);
  };
  return (
    <div className="color-token">
      <ColorInput
        aria-label={t(token.label)}
        value={toInputColor(shown) ?? "#000000"}
        onPreview={(color) => onPreview(picked(color))}
        onCommit={(color) => onChange(picked(color))}
      />
      <span className="color-token-label">{t(token.label)}</span>
      <input
        className={`color-token-value ${typed !== null && typed.trim() && !isColor(typed.trim()) ? "has-error" : ""}`}
        value={typed ?? value ?? ""}
        placeholder={applied}
        spellCheck={false}
        aria-label={t("appearance.value", { label: t(token.label) })}
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
        title={t("appearance.useTheBaseColor")}
        aria-label={t("appearance.resetColor", { label: t(token.label) })}
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
  const t = useT();
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
        {themeName(entry)}
        {entry.problems.length > 0 && (
          <span className="theme-card-warning"> {t("appearance.hasProblems")}</span>
        )}
      </span>
    </button>
  );
}
