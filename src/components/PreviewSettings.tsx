import {
  PREVIEW_LIMITS,
  type FilePreviewSettings,
  type PreviewLoad,
  type StorageKind,
} from "../lib/previewSettings";
import type { Settings } from "../lib/settings";
import { saveSettingsSection } from "../state/settingsStore";
import { NumberSetting, SelectSetting, SettingGroup } from "./settingsFields";
import { useT, t } from "../i18n";

const LOAD_OPTIONS: { value: PreviewLoad; label: string }[] = [
  {
    value: "rest",
    get label() {
      return t("preview.whenTheCursorRestsOn");
    },
  },
  {
    value: "click",
    get label() {
      return t("preview.onAClickOfShow");
    },
  },
];

const STORAGE_ROWS: { kind: StorageKind; label: string; hint?: string }[] = [
  {
    kind: "local",
    get label() {
      return t("preview.filesOnThisComputerLoad");
    },
  },
  {
    kind: "server",
    get label() {
      return t("preview.filesOnServersLoad");
    },
    get hint() {
      return t("preview.sftpFtpAndWebdav");
    },
  },
  {
    kind: "metered",
    get label() {
      return t("preview.filesOnStorageThatCharges");
    },
    get hint() {
      return t("preview.s3BackblazeB2GoogleDrive");
    },
  },
];

/** Settings > Interface: what the preview pane reads, and when. */
export function PreviewSettingsGroup({ settings }: { settings: Settings }) {
  const t = useT();
  const preview = settings.interface.preview;
  const set = (change: Partial<FilePreviewSettings>) =>
    void saveSettingsSection("interface", { preview: { ...preview, ...change } });
  return (
    <SettingGroup title={t("preview.preview")}>
      {STORAGE_ROWS.map(({ kind, label, hint }) => (
        <SelectSetting
          key={kind}
          label={label}
          hint={hint}
          value={preview.load[kind]}
          options={LOAD_OPTIONS}
          onChange={(load) => set({ load: { ...preview.load, [kind]: load } })}
        />
      ))}
      <NumberSetting
        label={t("preview.largestPictureLoaded")}
        hint={t("preview.largestPictureLoadedHint")}
        value={preview.maxImageMib}
        min={PREVIEW_LIMITS.maxImageMib.min}
        max={PREVIEW_LIMITS.maxImageMib.max}
        unit="MiB"
        onChange={(maxImageMib) => set({ maxImageMib })}
      />
      <NumberSetting
        label={t("preview.textReadFromAFile")}
        hint={t("preview.textReadFromAFileHint")}
        value={preview.textKib}
        min={PREVIEW_LIMITS.textKib.min}
        max={PREVIEW_LIMITS.textKib.max}
        unit="KiB"
        onChange={(textKib) => set({ textKib })}
      />
      <NumberSetting
        label={t("preview.thumbnailSize")}
        value={preview.thumbnailSize}
        min={PREVIEW_LIMITS.thumbnailSize.min}
        max={PREVIEW_LIMITS.thumbnailSize.max}
        unit="px"
        onChange={(thumbnailSize) => set({ thumbnailSize })}
      />
    </SettingGroup>
  );
}
