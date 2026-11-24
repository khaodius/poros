import {
  PREVIEW_LIMITS,
  type FilePreviewSettings,
  type PreviewLoad,
  type StorageKind,
} from "../lib/previewSettings";
import type { Settings } from "../lib/settings";
import { saveSettingsSection } from "../state/settingsStore";
import { NumberSetting, SelectSetting, SettingGroup } from "./settingsFields";

const LOAD_OPTIONS: { value: PreviewLoad; label: string }[] = [
  { value: "rest", label: "When the cursor rests on a file" },
  { value: "click", label: "On a click of Show preview" },
];

const STORAGE_ROWS: { kind: StorageKind; label: string; hint?: string }[] = [
  { kind: "local", label: "Files on this computer load" },
  { kind: "server", label: "Files on servers load", hint: "SFTP, FTP and WebDAV." },
  {
    kind: "metered",
    label: "Files on storage that charges for reads load",
    hint: "S3, Backblaze B2, Google Drive, OneDrive and Box, and vaults kept on them.",
  },
];

/** Settings > Interface: what the preview pane reads, and when. */
export function PreviewSettingsGroup({ settings }: { settings: Settings }) {
  const preview = settings.interface.preview;
  const set = (change: Partial<FilePreviewSettings>) =>
    void saveSettingsSection("interface", { preview: { ...preview, ...change } });
  return (
    <SettingGroup title="Preview">
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
        label="Largest picture loaded"
        hint="Larger pictures show their details and a Load anyway button."
        value={preview.maxImageMib}
        min={PREVIEW_LIMITS.maxImageMib.min}
        max={PREVIEW_LIMITS.maxImageMib.max}
        unit="MiB"
        onChange={(maxImageMib) => set({ maxImageMib })}
      />
      <NumberSetting
        label="Text read from a file"
        hint="A longer file shows its beginning."
        value={preview.textKib}
        min={PREVIEW_LIMITS.textKib.min}
        max={PREVIEW_LIMITS.textKib.max}
        unit="KiB"
        onChange={(textKib) => set({ textKib })}
      />
      <NumberSetting
        label="Thumbnail size"
        value={preview.thumbnailSize}
        min={PREVIEW_LIMITS.thumbnailSize.min}
        max={PREVIEW_LIMITS.thumbnailSize.max}
        unit="px"
        onChange={(thumbnailSize) => set({ thumbnailSize })}
      />
    </SettingGroup>
  );
}
