import { useEffect, useRef } from "react";
import { useOptionCheck } from "../hooks/useOptionCheck";
import { commandPreview, withOption } from "../lib/rsyncOptions";
import type { Settings, SyncSettings } from "../lib/settings";
import { DIRECTION_OPTIONS } from "../lib/sync";
import { previewSettingsSection, saveSettingsSection } from "../state/settingsStore";
import { OptionCheckView, RsyncArgumentsField, RsyncOptionsEditor } from "./RsyncOptionsEditor";
import {
  NumberSetting,
  SelectSetting,
  SettingGroup,
  SwitchSetting,
  TextSetting,
} from "./settingsFields";
import { useT } from "../i18n";

/** Typing pauses this long before the defaults are saved. */
const SAVE_DELAY_MS = 500;
const MAX_DELTA_THRESHOLD_KIB = 1024 * 1024;

export function SyncSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const transfers = settings.transfers;
  const defaults = settings.sync;
  const setTransfers = (change: Partial<Settings["transfers"]>) =>
    void saveSettingsSection("transfers", change);
  const saveTimer = useRef<number | undefined>(undefined);
  // Shown at once and saved once typing pauses, so each key press is not a write to disk.
  const setDefaults = (change: Partial<SyncSettings>) => {
    previewSettingsSection("sync", change);
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(
      () => void saveSettingsSection("sync", {}),
      SAVE_DELAY_MS,
    );
  };
  useEffect(
    () => () => {
      if (saveTimer.current !== undefined) {
        window.clearTimeout(saveTimer.current);
        void saveSettingsSection("sync", {});
      }
    },
    [],
  );
  const check = useOptionCheck(defaults.options, [defaults.arguments], defaults.direction);

  return (
    <>
      <SettingGroup title={t("sync.rsync2")}>
        <p className="setting-note">{t("sync.rsyncRunsOnTheServer")}</p>
        <SwitchSetting
          label={t("sync.useRsync")}
          hint={t("sync.useRsyncHint")}
          checked={transfers.deltaTransfers}
          onChange={(deltaTransfers) => setTransfers({ deltaTransfers })}
        />
        <NumberSetting
          label={t("sync.forFilesLargerThan")}
          hint={t("sync.forFilesLargerThanHint")}
          value={transfers.deltaThresholdKib}
          min={0}
          max={MAX_DELTA_THRESHOLD_KIB}
          unit="KiB"
          disabled={!transfers.deltaTransfers}
          onChange={(deltaThresholdKib) => setTransfers({ deltaThresholdKib })}
        />
        <TextSetting
          label={t("sync.rsyncOnTheServer")}
          hint={t("sync.rsyncOnTheServerHint")}
          value={transfers.rsyncPath}
          placeholder={t("sync.rsync2")}
          disabled={!transfers.deltaTransfers}
          onCommit={(rsyncPath) => setTransfers({ rsyncPath })}
        />
      </SettingGroup>

      <SettingGroup title={t("sync.folderSynchronization")}>
        <p className="setting-note">{t("sync.eachSynchronizationStartsWithThese")}</p>
        <SelectSetting
          label={t("sync.direction")}
          value={defaults.direction}
          options={DIRECTION_OPTIONS}
          onChange={(direction) =>
            setDefaults({
              direction,
              options: withOption(defaults.options, "compare", defaults.options.compare, direction),
            })
          }
        />
        <RsyncOptionsEditor
          options={defaults.options}
          direction={defaults.direction}
          onChange={(options) => setDefaults({ options })}
        />
        <RsyncArgumentsField
          value={defaults.arguments}
          hint={t("sync.typedAsOnRsyncsCommand")}
          onChange={(value) => setDefaults({ arguments: value })}
        />
        <OptionCheckView
          check={check}
          command={check ? commandPreview(check.arguments, defaults.direction, "", "") : undefined}
        />
      </SettingGroup>
    </>
  );
}
