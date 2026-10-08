import { useState } from "react";
import { TIME_TOLERANCE_LIMITS, type Settings, type SyncSettings } from "../lib/settings";
import { COMPARE_OPTIONS, DIRECTION_OPTIONS } from "../lib/sync";
import { saveSettingsSection } from "../state/settingsStore";
import {
  NumberSetting,
  SelectSetting,
  SettingGroup,
  SwitchSetting,
  TextSetting,
} from "./settingsFields";

const MAX_DELTA_THRESHOLD_KIB = 1024 * 1024;

export function SyncSettingsPage({ settings }: { settings: Settings }) {
  const transfers = settings.transfers;
  const defaults = settings.sync;
  const setTransfers = (change: Partial<Settings["transfers"]>) =>
    void saveSettingsSection("transfers", change);
  const setDefaults = (change: Partial<SyncSettings>) => void saveSettingsSection("sync", change);
  const both = defaults.direction === "both";

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
          Synchronize folders starts with these, and remembers the options you compare with.
        </p>
        <SelectSetting
          label="Direction"
          value={defaults.direction}
          options={DIRECTION_OPTIONS}
          onChange={(direction) =>
            setDefaults(
              direction === "both" && defaults.compare === "always"
                ? { direction, compare: "sizeAndTime" }
                : { direction },
            )
          }
        />
        <SelectSetting
          label="Compare files by"
          value={defaults.compare}
          options={COMPARE_OPTIONS.filter((option) => !both || option.value !== "always")}
          onChange={(compare) => setDefaults({ compare })}
        />
        <NumberSetting
          label="Times this close count as equal"
          hint="Some file systems, such as FAT, keep times to 2 seconds."
          value={defaults.timeToleranceSecs}
          min={TIME_TOLERANCE_LIMITS.min}
          max={TIME_TOLERANCE_LIMITS.max}
          unit="s"
          onChange={(timeToleranceSecs) => setDefaults({ timeToleranceSecs })}
        />
        <SwitchSetting
          label="Delete files the source does not have"
          hint="Not available both ways."
          checked={defaults.deleteExtraneous}
          disabled={both}
          onChange={(deleteExtraneous) => setDefaults({ deleteExtraneous })}
        />
        <SwitchSetting
          label="Skip files that are newer on the target"
          checked={defaults.skipNewerOnTarget}
          disabled={both}
          onChange={(skipNewerOnTarget) => setDefaults({ skipNewerOnTarget })}
        />
        <SwitchSetting
          label="Skip files that already exist on the target"
          checked={defaults.ignoreExisting}
          onChange={(ignoreExisting) => setDefaults({ ignoreExisting })}
        />
        <ExcludeSetting
          value={defaults.excludes}
          onCommit={(excludes) => setDefaults({ excludes })}
        />
      </SettingGroup>
    </>
  );
}

function ExcludeSetting({ value, onCommit }: { value: string; onCommit: (value: string) => void }) {
  const [typed, setTyped] = useState<string | null>(null);
  return (
    <label className="field setting-textarea">
      <span>Exclude</span>
      <textarea
        value={typed ?? value}
        rows={4}
        spellCheck={false}
        placeholder={"*.tmp\nnode_modules/\n/build"}
        onChange={(event) => setTyped(event.target.value)}
        onBlur={() => {
          if (typed !== null && typed !== value) onCommit(typed);
          setTyped(null);
        }}
      />
      <small className="field-hint">One rsync pattern per line.</small>
    </label>
  );
}
