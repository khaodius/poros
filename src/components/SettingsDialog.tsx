import {
  ArrowLeftRight,
  FileText,
  LayoutPanelLeft,
  Network,
  Palette,
  type LucideIcon,
} from "lucide-react";
import { MAX_WORKERS, LOG_LINE_LIMITS, type Settings } from "../lib/settings";
import type { ExistsAction } from "../lib/types";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useUiStore, type SettingsSection } from "../state/uiStore";
import { AppearanceSettings } from "./AppearanceSettings";
import { Dialog } from "./Dialog";
import { NumberSetting, SelectSetting, SettingGroup, SwitchSetting } from "./settingsFields";

const SECTIONS: { id: SettingsSection; label: string; icon: LucideIcon }[] = [
  { id: "transfers", label: "Transfers", icon: ArrowLeftRight },
  { id: "connection", label: "Connection", icon: Network },
  { id: "interface", label: "Interface", icon: LayoutPanelLeft },
  { id: "appearance", label: "Appearance", icon: Palette },
  { id: "log", label: "Log", icon: FileText },
];

const EXISTS_ACTIONS: { value: ExistsAction; label: string }[] = [
  { value: "ask", label: "Ask me" },
  { value: "overwrite", label: "Overwrite" },
  { value: "overwriteIfNewer", label: "Overwrite if the source is newer" },
  { value: "overwriteIfDifferent", label: "Overwrite if size or date differ" },
  { value: "resume", label: "Resume" },
  { value: "rename", label: "Keep both" },
  { value: "skip", label: "Skip" },
];

export function SettingsDialog({ section }: { section: SettingsSection }) {
  const open = useUiStore((state) => state.open);
  const close = useUiStore((state) => state.close);
  const settings = useSettingsStore((state) => state.settings);

  return (
    <Dialog title="Settings" onClose={close} width={860}>
      <div className="settings">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              className={`settings-nav-item ${section === id ? "is-selected" : ""}`}
              aria-current={section === id}
              onClick={() => open({ kind: "settings", section: id })}
            >
              <Icon size={15} />
              {label}
            </button>
          ))}
        </nav>
        <div className="settings-page">
          {section === "transfers" && <TransferSettingsPage settings={settings} />}
          {section === "connection" && <ConnectionSettingsPage settings={settings} />}
          {section === "interface" && <InterfaceSettingsPage settings={settings} />}
          {section === "appearance" && <AppearanceSettings />}
          {section === "log" && <LogSettingsPage settings={settings} />}
        </div>
      </div>
    </Dialog>
  );
}

function TransferSettingsPage({ settings }: { settings: Settings }) {
  const transfers = settings.transfers;
  const set = (change: Partial<Settings["transfers"]>) =>
    void saveSettingsSection("transfers", change);
  return (
    <>
      <SettingGroup title="Queue">
        <NumberSetting
          label="Simultaneous transfers"
          hint="How many files move at once."
          value={transfers.workers}
          min={1}
          max={MAX_WORKERS}
          onChange={(workers) => set({ workers })}
        />
        <SwitchSetting
          label="Start transfers as soon as they are queued"
          hint="Off: the queue waits until you press Start queue."
          checked={transfers.autoStart}
          onChange={(autoStart) => set({ autoStart })}
        />
        <SelectSetting
          label="When the target file exists"
          value={transfers.existsAction}
          options={EXISTS_ACTIONS}
          onChange={(existsAction) => set({ existsAction })}
        />
        <SwitchSetting
          label="Keep finished transfers in the list"
          checked={transfers.keepCompleted}
          onChange={(keepCompleted) => set({ keepCompleted })}
        />
      </SettingGroup>

      <SettingGroup title="Connections">
        <SwitchSetting
          label="Give each transfer its own connection"
          hint="Off: transfers share the browsing connection, for servers that limit logins."
          checked={transfers.separateConnections}
          onChange={(separateConnections) => set({ separateConnections })}
        />
        <SwitchSetting
          label="Split large files across connections"
          hint="Idle transfer slots join a large file and move different parts of it."
          checked={transfers.segmented}
          onChange={(segmented) => set({ segmented })}
        />
        <NumberSetting
          label="Split files larger than"
          value={transfers.segmentThresholdMib}
          min={1}
          max={1024 * 1024}
          unit="MiB"
          disabled={!transfers.segmented}
          onChange={(segmentThresholdMib) => set({ segmentThresholdMib })}
        />
        <NumberSetting
          label="Connections per file"
          value={transfers.maxSegments}
          min={2}
          max={MAX_WORKERS}
          disabled={!transfers.segmented}
          onChange={(maxSegments) => set({ maxSegments })}
        />
      </SettingGroup>

      <SettingGroup title="Bandwidth">
        <NumberSetting
          label="Upload limit"
          hint="0 means unlimited. Shared by all transfers."
          value={transfers.uploadLimitKib}
          min={0}
          max={10 * 1024 * 1024}
          unit="KiB/s"
          onChange={(uploadLimitKib) => set({ uploadLimitKib })}
        />
        <NumberSetting
          label="Download limit"
          hint="0 means unlimited. Shared by all transfers."
          value={transfers.downloadLimitKib}
          min={0}
          max={10 * 1024 * 1024}
          unit="KiB/s"
          onChange={(downloadLimitKib) => set({ downloadLimitKib })}
        />
      </SettingGroup>

      <SettingGroup title="Buffers">
        <NumberSetting
          label="Request size"
          hint="Bytes per SFTP read or write. Most servers accept up to 255 KiB; some cap at 32."
          value={transfers.requestSizeKib}
          min={4}
          max={255}
          unit="KiB"
          onChange={(requestSizeKib) => set({ requestSizeKib })}
        />
        <NumberSetting
          label="Requests in flight"
          hint="Outstanding requests per connection. Higher hides more network latency."
          value={transfers.requestsInFlight}
          min={1}
          max={256}
          onChange={(requestsInFlight) => set({ requestsInFlight })}
        />
      </SettingGroup>

      <SettingGroup title="Files">
        <SwitchSetting
          label="Keep modification times"
          checked={transfers.preserveTimestamps}
          onChange={(preserveTimestamps) => set({ preserveTimestamps })}
        />
        <SwitchSetting
          label="Keep permissions"
          hint="Copies Unix permission bits. Has no effect for files from Windows."
          checked={transfers.preservePermissions}
          onChange={(preservePermissions) => set({ preservePermissions })}
        />
      </SettingGroup>

      <SettingGroup title="Errors">
        <NumberSetting
          label="Retries"
          hint="For timeouts and dropped connections."
          value={transfers.retryAttempts}
          min={0}
          max={20}
          onChange={(retryAttempts) => set({ retryAttempts })}
        />
        <NumberSetting
          label="Wait between retries"
          value={transfers.retryDelaySecs}
          min={0}
          max={600}
          unit="s"
          onChange={(retryDelaySecs) => set({ retryDelaySecs })}
        />
      </SettingGroup>
    </>
  );
}

function ConnectionSettingsPage({ settings }: { settings: Settings }) {
  const connection = settings.connection;
  const set = (change: Partial<Settings["connection"]>) =>
    void saveSettingsSection("connection", change);
  return (
    <>
      <SettingGroup title="SSH">
        <NumberSetting
          label="Connection timeout"
          value={connection.timeoutSecs}
          min={3}
          max={300}
          unit="s"
          onChange={(timeoutSecs) => set({ timeoutSecs })}
        />
        <NumberSetting
          label="Keepalive interval"
          hint="0 turns keepalives off. Applies to new connections."
          value={connection.keepaliveSecs}
          min={0}
          max={3600}
          unit="s"
          onChange={(keepaliveSecs) => set({ keepaliveSecs })}
        />
        <SwitchSetting
          label="Compress traffic"
          hint="Helps text over slow links; slows already compressed files. Applies to new connections."
          checked={connection.compression}
          onChange={(compression) => set({ compression })}
        />
      </SettingGroup>
      <SettingGroup title="Saved connections">
        <SwitchSetting
          label="Save quick connections automatically"
          hint="Adds each new server you reach from the quick connect bar, without its password."
          checked={settings.interface.saveQuickConnections}
          onChange={(saveQuickConnections) =>
            void saveSettingsSection("interface", { saveQuickConnections })
          }
        />
      </SettingGroup>
    </>
  );
}

function InterfaceSettingsPage({ settings }: { settings: Settings }) {
  const options = settings.interface;
  const set = (change: Partial<Settings["interface"]>) =>
    void saveSettingsSection("interface", change);
  return (
    <>
      <SettingGroup title="Files">
        <SelectSetting
          label="Double-clicking a file"
          value={options.doubleClickFile}
          options={[
            { value: "transfer", label: "Transfers it to the other side" },
            { value: "nothing", label: "Does nothing" },
          ]}
          onChange={(doubleClickFile) => set({ doubleClickFile })}
        />
        <SwitchSetting
          label="Show hidden files in new tabs"
          checked={options.showHiddenFiles}
          onChange={(showHiddenFiles) => set({ showHiddenFiles })}
        />
        <SwitchSetting
          label="Compact rows"
          checked={settings.appearance.compactRows}
          onChange={(compactRows) => void saveSettingsSection("appearance", { compactRows })}
        />
      </SettingGroup>
      <SettingGroup title="Tabs and windows">
        <SwitchSetting
          label="Ask before closing a tab with queued transfers"
          checked={options.confirmCloseWithTransfers}
          onChange={(confirmCloseWithTransfers) => set({ confirmCloseWithTransfers })}
        />
        <SwitchSetting
          label="Remember tabs and layout"
          hint="Reopens local tabs and the arrangement of panes on the next start."
          checked={options.rememberLayout}
          onChange={(rememberLayout) => set({ rememberLayout })}
        />
        <SwitchSetting
          label="Use the system title bar"
          hint="Off: the top bar holds the window buttons and drags the window."
          checked={options.systemTitleBar}
          onChange={(systemTitleBar) => set({ systemTitleBar })}
        />
      </SettingGroup>
    </>
  );
}

function LogSettingsPage({ settings }: { settings: Settings }) {
  const log = settings.log;
  const set = (change: Partial<Settings["log"]>) => void saveSettingsSection("log", change);
  return (
    <SettingGroup title="Log">
      <NumberSetting
        label="Lines to keep"
        value={log.maxLines}
        min={LOG_LINE_LIMITS.min}
        max={LOG_LINE_LIMITS.max}
        onChange={(maxLines) => set({ maxLines })}
      />
      <SwitchSetting
        label="Show times"
        checked={log.timestamps}
        onChange={(timestamps) => set({ timestamps })}
      />
      <SwitchSetting
        label="Wrap long lines"
        checked={log.wrapLines}
        onChange={(wrapLines) => set({ wrapLines })}
      />
      <SwitchSetting
        label="Log every transferred file"
        hint="Off: one summary line when the queue finishes."
        checked={settings.transfers.logEachFile}
        onChange={(logEachFile) => void saveSettingsSection("transfers", { logEachFile })}
      />
    </SettingGroup>
  );
}
