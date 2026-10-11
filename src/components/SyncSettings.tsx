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

/** Typing pauses this long before the defaults are saved. */
const SAVE_DELAY_MS = 500;
const MAX_DELTA_THRESHOLD_KIB = 1024 * 1024;

export function SyncSettingsPage({ settings }: { settings: Settings }) {
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
      <SettingGroup title="rsync">
        <p className="setting-note">
          rsync runs on the server through your SSH connection, so the server needs rsync installed.
          rsync daemons (rsync:// addresses on port 873) are not supported.
        </p>
        <SwitchSetting
          label="Use rsync"
          hint="Files that already exist on the other side are updated by sending only the parts that changed. When this is off, or the server has no rsync, files are copied whole over SFTP."
          checked={transfers.deltaTransfers}
          onChange={(deltaTransfers) => setTransfers({ deltaTransfers })}
        />
        <NumberSetting
          label="For files larger than"
          hint="Smaller files are quicker to copy whole."
          value={transfers.deltaThresholdKib}
          min={0}
          max={MAX_DELTA_THRESHOLD_KIB}
          unit="KiB"
          disabled={!transfers.deltaTransfers}
          onChange={(deltaThresholdKib) => setTransfers({ deltaThresholdKib })}
        />
        <TextSetting
          label="rsync on the server"
          hint="The command that starts it, for servers where it is not on the PATH."
          value={transfers.rsyncPath}
          placeholder="rsync"
          disabled={!transfers.deltaTransfers}
          onCommit={(rsyncPath) => setTransfers({ rsyncPath })}
        />
      </SettingGroup>

      <SettingGroup title="Folder synchronization">
        <p className="setting-note">
          Each synchronization starts with these options. A saved site&apos;s own rsync arguments
          apply on top, and Synchronize folders can change any of them for one run.
        </p>
        <SelectSetting
          label="Direction"
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
          hint="Typed as on rsync's command line; they apply over the options above for every synchronization."
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
