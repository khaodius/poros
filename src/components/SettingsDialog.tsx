import { useState } from "react";
import {
  ArrowLeftRight,
  CircleArrowUp,
  Cloud,
  FileText,
  FolderSync,
  LayoutPanelLeft,
  Network,
  Package,
  Palette,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import { COLUMN_LABELS, DETAIL_COLUMNS } from "../lib/columns";
import { formatDate, formatPercent, formatTime, formatVersion } from "../lib/format";
import { settingsStore, toAppError, updates } from "../lib/ipc";
import {
  DATE_FORMATS,
  LOG_LINE_LIMITS,
  MAX_RECONNECT_MINUTES,
  MAX_WORKERS,
  SOCKET_BUFFER_LIMITS,
  defaultProxyPort,
  parseMinisignKeys,
  type ProxyKind,
  type ProxySettings,
  type Settings,
} from "../lib/settings";
import type { ExistsAction } from "../lib/types";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";
import { useUiStore, type SettingsSection } from "../state/uiStore";
import {
  checkForUpdates,
  installVersion,
  isInstalling,
  OLDEST_INSTALLABLE,
  percentDownloaded,
  restartToUpdate,
  useRestartWarnings,
  useUpdateStore,
  type InstallStage,
} from "../state/updateStore";
import { AppearanceSettings } from "./AppearanceSettings";
import { ArchiveSettingsPage } from "./ArchiveSettings";
import { AutomationSettingsPage } from "./AutomationSettings";
import { CloudSettingsPage } from "./CloudSettings";
import { Dialog } from "./Dialog";
import { ExternalEditorSettings } from "./ExternalEditorSettings";
import { LimitScheduleSettings } from "./LimitScheduleSettings";
import { PreviewSettingsGroup } from "./PreviewSettings";
import { SyncSettingsPage } from "./SyncSettings";
import {
  NumberSetting,
  SelectSetting,
  SettingGroup,
  SettingRow,
  SwitchSetting,
  TextSetting,
} from "./settingsFields";
import { useT, t } from "../i18n";
import { LanguagePicker } from "./LanguagePicker";

const SECTIONS: { id: SettingsSection; label: string; icon: LucideIcon }[] = [
  {
    id: "transfers",
    get label() {
      return t("settings.transfers");
    },
    icon: ArrowLeftRight,
  },
  {
    id: "sync",
    get label() {
      return t("settings.sync");
    },
    icon: FolderSync,
  },
  {
    id: "connection",
    get label() {
      return t("settings.connection");
    },
    icon: Network,
  },
  {
    id: "cloud",
    get label() {
      return t("settings.cloudAccounts");
    },
    icon: Cloud,
  },
  {
    id: "automation",
    get label() {
      return t("settings.automation");
    },
    icon: Workflow,
  },
  {
    id: "archives",
    get label() {
      return t("settings.archives");
    },
    icon: Package,
  },
  {
    id: "interface",
    get label() {
      return t("settings.interface");
    },
    icon: LayoutPanelLeft,
  },
  {
    id: "appearance",
    get label() {
      return t("settings.appearance");
    },
    icon: Palette,
  },
  {
    id: "log",
    get label() {
      return t("settings.log");
    },
    icon: FileText,
  },
  {
    id: "updates",
    get label() {
      return t("settings.updates");
    },
    icon: CircleArrowUp,
  },
];

/** 2026-10-06 14:05:09 local time, to show what each date format looks like. */
const DATE_SAMPLE = new Date(2026, 9, 6, 14, 5, 9).getTime() / 1000;

const EXISTS_ACTIONS: { value: ExistsAction; label: string }[] = [
  {
    value: "ask",
    get label() {
      return t("common.askMe");
    },
  },
  {
    value: "overwrite",
    get label() {
      return t("common.overwrite");
    },
  },
  {
    value: "overwriteIfNewer",
    get label() {
      return t("settings.overwriteIfTheSourceIs");
    },
  },
  {
    value: "overwriteIfDifferent",
    get label() {
      return t("settings.overwriteIfSizeOrDate");
    },
  },
  {
    value: "resume",
    get label() {
      return t("common.resume");
    },
  },
  {
    value: "rename",
    get label() {
      return t("settings.keepBoth");
    },
  },
  {
    value: "skip",
    get label() {
      return t("common.skip");
    },
  },
];

export function SettingsDialog({ section }: { section: SettingsSection }) {
  const t = useT();
  const open = useUiStore((state) => state.open);
  const close = useUiStore((state) => state.close);
  const settings = useSettingsStore((state) => state.settings);

  return (
    <Dialog title={t("common.settings")} onClose={close} width={860}>
      <div className="settings">
        <nav className="settings-nav" aria-label={t("settings.settingsSections")}>
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
          {section === "sync" && <SyncSettingsPage settings={settings} />}
          {section === "connection" && <ConnectionSettingsPage settings={settings} />}
          {section === "cloud" && <CloudSettingsPage settings={settings} />}
          {section === "automation" && <AutomationSettingsPage settings={settings} />}
          {section === "archives" && <ArchiveSettingsPage settings={settings} />}
          {section === "interface" && <InterfaceSettingsPage settings={settings} />}
          {section === "appearance" && <AppearanceSettings />}
          {section === "log" && <LogSettingsPage settings={settings} />}
          {section === "updates" && <UpdateSettingsPage settings={settings} />}
        </div>
      </div>
    </Dialog>
  );
}

function TransferSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const transfers = settings.transfers;
  const set = (change: Partial<Settings["transfers"]>) =>
    void saveSettingsSection("transfers", change);
  return (
    <>
      <SettingGroup title={t("settings.queue")}>
        <NumberSetting
          label={t("settings.simultaneousTransfers")}
          hint={t("settings.simultaneousTransfersHint")}
          value={transfers.workers}
          min={1}
          max={MAX_WORKERS}
          onChange={(workers) => set({ workers })}
        />
        <SwitchSetting
          label={t("settings.startTransfersAsSoonAs")}
          hint={t("settings.startTransfersAsSoonAsHint")}
          checked={transfers.autoStart}
          onChange={(autoStart) => set({ autoStart })}
        />
        <SelectSetting
          label={t("settings.whenTheTargetFileExists")}
          value={transfers.existsAction}
          options={EXISTS_ACTIONS}
          onChange={(existsAction) => set({ existsAction })}
        />
        <SwitchSetting
          label={t("settings.keepFinishedTransfersInThe")}
          checked={transfers.keepCompleted}
          onChange={(keepCompleted) => set({ keepCompleted })}
        />
        <SwitchSetting
          label={t("settings.keepUnfinishedTransfersWhenPoros")}
          hint={t("settings.keepUnfinishedTransfersWhenPorosHint")}
          checked={transfers.keepQueue}
          onChange={(keepQueue) => set({ keepQueue })}
        />
      </SettingGroup>

      <SettingGroup title={t("settings.connections")}>
        <SwitchSetting
          label={t("settings.giveEachTransferItsOwn")}
          hint={t("settings.giveEachTransferItsOwnHint")}
          checked={transfers.separateConnections}
          onChange={(separateConnections) => set({ separateConnections })}
        />
        <SwitchSetting
          label={t("settings.splitLargeFilesAcrossConnections")}
          hint={t("settings.splitLargeFilesAcrossConnectionsHint")}
          checked={transfers.segmented}
          onChange={(segmented) => set({ segmented })}
        />
        <NumberSetting
          label={t("settings.splitFilesLargerThan")}
          value={transfers.segmentThresholdMib}
          min={1}
          max={1024 * 1024}
          unit="MiB"
          disabled={!transfers.segmented}
          onChange={(segmentThresholdMib) => set({ segmentThresholdMib })}
        />
        <NumberSetting
          label={t("settings.connectionsPerFile")}
          value={transfers.maxSegments}
          min={2}
          max={MAX_WORKERS}
          disabled={!transfers.segmented}
          onChange={(maxSegments) => set({ maxSegments })}
        />
        <SwitchSetting
          label={t("settings.copyDirectlyBetweenFtpServers")}
          hint={t("settings.copyDirectlyBetweenFtpServersHint")}
          checked={transfers.fxp}
          onChange={(fxp) => set({ fxp })}
        />
      </SettingGroup>

      <SettingGroup title={t("settings.bandwidth")}>
        <NumberSetting
          label={t("settings.uploadLimit")}
          hint={t("settings.uploadLimitHint")}
          value={transfers.uploadLimitKib}
          min={0}
          max={10 * 1024 * 1024}
          unit="KiB/s"
          onChange={(uploadLimitKib) => set({ uploadLimitKib })}
        />
        <NumberSetting
          label={t("settings.downloadLimit")}
          hint={t("settings.uploadLimitHint")}
          value={transfers.downloadLimitKib}
          min={0}
          max={10 * 1024 * 1024}
          unit="KiB/s"
          onChange={(downloadLimitKib) => set({ downloadLimitKib })}
        />
      </SettingGroup>

      <LimitScheduleSettings transfers={transfers} />

      <SettingGroup title={t("settings.buffers")}>
        <NumberSetting
          label={t("settings.requestSize")}
          hint={t("settings.requestSizeHint")}
          value={transfers.requestSizeKib}
          min={4}
          max={255}
          unit="KiB"
          onChange={(requestSizeKib) => set({ requestSizeKib })}
        />
        <NumberSetting
          label={t("settings.requestsInFlight")}
          hint={t("settings.requestsInFlightHint")}
          value={transfers.requestsInFlight}
          min={1}
          max={256}
          onChange={(requestsInFlight) => set({ requestsInFlight })}
        />
        <SocketBufferSettings connection={settings.connection} />
      </SettingGroup>

      <SettingGroup title={t("settings.files")}>
        <SwitchSetting
          label={t("settings.keepModificationTimes")}
          checked={transfers.preserveTimestamps}
          onChange={(preserveTimestamps) => set({ preserveTimestamps })}
        />
        <SwitchSetting
          label={t("settings.keepPermissions")}
          hint={t("settings.keepPermissionsHint")}
          checked={transfers.preservePermissions}
          onChange={(preservePermissions) => set({ preservePermissions })}
        />
      </SettingGroup>

      <SettingGroup title={t("settings.integrity")}>
        <SwitchSetting
          label={t("settings.writeToATemporaryName")}
          hint={t("settings.writeToATemporaryNameHint")}
          checked={transfers.temporaryFiles}
          onChange={(temporaryFiles) => set({ temporaryFiles })}
        />
        <SwitchSetting
          label={t("settings.verifyChecksums")}
          hint={t("settings.verifyChecksumsHint")}
          checked={transfers.verifyChecksums}
          onChange={(verifyChecksums) => set({ verifyChecksums })}
        />
        <SwitchSetting
          label={t("settings.readFilesBackWhenThe")}
          hint={t("settings.readFilesBackWhenTheHint")}
          checked={transfers.readBack}
          disabled={!transfers.verifyChecksums}
          onChange={(readBack) => set({ readBack })}
        />
        <SwitchSetting
          label={t("settings.repairOnlyTheDamagedParts")}
          hint={t("settings.repairOnlyTheDamagedPartsHint")}
          checked={transfers.repairParts}
          disabled={!transfers.verifyChecksums}
          onChange={(repairParts) => set({ repairParts })}
        />
        <SwitchSetting
          label={t("settings.checkTheTransferredPartBefore")}
          hint={t("settings.checkTheTransferredPartBeforeHint")}
          checked={transfers.verifyResume}
          onChange={(verifyResume) => set({ verifyResume })}
        />
        <SwitchSetting
          label={t("settings.flushToDiskBeforeFinishing")}
          hint={t("settings.flushToDiskBeforeFinishingHint")}
          checked={transfers.flushToDisk}
          onChange={(flushToDisk) => set({ flushToDisk })}
        />
        <SwitchSetting
          label={t("settings.checkTimesAndPermissionsAfter")}
          hint={t("settings.checkTimesAndPermissionsAfterHint")}
          checked={transfers.checkMetadata}
          onChange={(checkMetadata) => set({ checkMetadata })}
        />
        <SwitchSetting
          label={t("settings.checkPublishedChecksumsAndSignatures")}
          hint={t("settings.checkPublishedChecksumsAndSignaturesHint")}
          checked={transfers.checkPublished}
          onChange={(checkPublished) => set({ checkPublished })}
        />
        <MinisignKeysSetting
          keys={transfers.minisignKeys}
          disabled={!transfers.checkPublished}
          onCommit={(minisignKeys) => set({ minisignKeys })}
        />
      </SettingGroup>

      <SettingGroup title={t("settings.errors")}>
        <NumberSetting
          label={t("settings.retries")}
          hint={t("settings.retriesHint")}
          value={transfers.retryAttempts}
          min={0}
          max={20}
          onChange={(retryAttempts) => set({ retryAttempts })}
        />
        <NumberSetting
          label={t("settings.waitBetweenRetries")}
          value={transfers.retryDelaySecs}
          min={0}
          max={600}
          unit={t("common.secondsUnit")}
          onChange={(retryDelaySecs) => set({ retryDelaySecs })}
        />
      </SettingGroup>
    </>
  );
}

/** How every minisign public key starts, as a hint of what to paste. */
const MINISIGN_KEY_START = "RWQ...";

/** One minisign public key per line, saved when the box loses focus. */
function MinisignKeysSetting({
  keys,
  disabled,
  onCommit,
}: {
  keys: string[];
  disabled: boolean;
  onCommit: (keys: string[]) => void;
}) {
  const t = useT();
  const [typed, setTyped] = useState<string | null>(null);
  const text = keys.join("\n");
  const commit = () => {
    if (typed !== null && typed !== text) onCommit(parseMinisignKeys(typed));
    setTyped(null);
  };
  return (
    <div className={`setting-textarea ${disabled ? "is-disabled" : ""}`}>
      <div className="setting-text">
        <span className="setting-label">{t("settings.trustedMinisignKeys")}</span>
        <span className="setting-hint">{t("settings.onePublicKeyPerLine")}</span>
      </div>
      <textarea
        value={typed ?? text}
        aria-label={t("settings.trustedMinisignKeys")}
        placeholder={MINISIGN_KEY_START}
        rows={3}
        spellCheck={false}
        disabled={disabled}
        onChange={(event) => setTyped(event.target.value)}
        onBlur={commit}
      />
    </div>
  );
}

/** TCP socket buffers, kept with the connection settings since every connection uses them. */
function SocketBufferSettings({ connection }: { connection: Settings["connection"] }) {
  const t = useT();
  const set = (change: Partial<Settings["connection"]>) =>
    void saveSettingsSection("connection", change);
  return (
    <>
      <SwitchSetting
        label={t("settings.autoTuneReceiveBuffer")}
        hint={t("settings.autoTuneReceiveBufferHint")}
        checked={connection.autoTuneReceiveBuffer}
        onChange={(autoTuneReceiveBuffer) => set({ autoTuneReceiveBuffer })}
      />
      <NumberSetting
        label={t("settings.receiveBufferSize")}
        hint={t("settings.receiveBufferSizeHint")}
        value={connection.receiveBufferKib}
        min={SOCKET_BUFFER_LIMITS.min}
        max={SOCKET_BUFFER_LIMITS.max}
        unit="KiB"
        disabled={connection.autoTuneReceiveBuffer}
        onChange={(receiveBufferKib) => set({ receiveBufferKib })}
      />
      <SwitchSetting
        label={t("settings.autoTuneSendBuffer")}
        hint={t("settings.autoTuneSendBufferHint")}
        checked={connection.autoTuneSendBuffer}
        onChange={(autoTuneSendBuffer) => set({ autoTuneSendBuffer })}
      />
      <NumberSetting
        label={t("settings.sendBufferSize")}
        hint={t("settings.sendBufferSizeHint")}
        value={connection.sendBufferKib}
        min={SOCKET_BUFFER_LIMITS.min}
        max={SOCKET_BUFFER_LIMITS.max}
        unit="KiB"
        disabled={connection.autoTuneSendBuffer}
        onChange={(sendBufferKib) => set({ sendBufferKib })}
      />
    </>
  );
}

function ConnectionSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const connection = settings.connection;
  const set = (change: Partial<Settings["connection"]>) =>
    void saveSettingsSection("connection", change);
  return (
    <>
      <SettingGroup title="SSH">
        <NumberSetting
          label={t("settings.connectionTimeout")}
          value={connection.timeoutSecs}
          min={3}
          max={300}
          unit={t("common.secondsUnit")}
          onChange={(timeoutSecs) => set({ timeoutSecs })}
        />
        <NumberSetting
          label={t("settings.keepaliveInterval")}
          hint={t("settings.keepaliveIntervalHint")}
          value={connection.keepaliveSecs}
          min={0}
          max={3600}
          unit={t("common.secondsUnit")}
          onChange={(keepaliveSecs) => set({ keepaliveSecs })}
        />
        <SwitchSetting
          label={t("settings.compressTraffic")}
          hint={t("settings.compressTrafficHint")}
          checked={connection.compression}
          onChange={(compression) => set({ compression })}
        />
      </SettingGroup>
      <SettingGroup title={t("settings.reconnecting")}>
        <SwitchSetting
          label={t("settings.reconnectLostTabsAutomatically")}
          hint={t("settings.reconnectLostTabsAutomaticallyHint")}
          checked={connection.autoReconnect}
          onChange={(autoReconnect) => set({ autoReconnect })}
        />
        <NumberSetting
          label={t("settings.keepTryingFor")}
          hint={t("settings.keepTryingForHint")}
          value={settings.transfers.reconnectMinutes}
          min={0}
          max={MAX_RECONNECT_MINUTES}
          unit={t("common.minutesUnit")}
          onChange={(reconnectMinutes) =>
            void saveSettingsSection("transfers", { reconnectMinutes })
          }
        />
      </SettingGroup>
      <ProxySettingsGroup proxy={connection.proxy} />
      <SettingGroup title={t("settings.savedConnections")}>
        <SwitchSetting
          label={t("settings.saveQuickConnectionsAutomatically")}
          hint={t("settings.saveQuickConnectionsAutomaticallyHint")}
          checked={settings.interface.saveQuickConnections}
          onChange={(saveQuickConnections) =>
            void saveSettingsSection("interface", { saveQuickConnections })
          }
        />
      </SettingGroup>
    </>
  );
}

const PROXY_KINDS: { value: ProxyKind; label: string }[] = [
  {
    value: "none",
    get label() {
      return t("common.none");
    },
  },
  { value: "socks5", label: "SOCKS5" },
  { value: "socks4", label: "SOCKS4" },
  { value: "http", label: "HTTP (CONNECT)" },
];

function ProxySettingsGroup({ proxy }: { proxy: ProxySettings }) {
  const t = useT();
  const set = (change: Partial<ProxySettings>) =>
    void saveSettingsSection("connection", { proxy: { ...proxy, ...change } });
  const off = proxy.kind === "none";
  const setPassword = (password: string) =>
    void settingsStore
      .setProxyPassword(password)
      .catch((caught) => useToastStore.getState().show("error", toAppError(caught).message));
  return (
    <SettingGroup title={t("settings.proxy")}>
      <p className="setting-note">{t("settings.newSftpConnectionsReachTheir")}</p>
      <SelectSetting
        label={t("settings.type")}
        value={proxy.kind}
        options={PROXY_KINDS}
        onChange={(kind) =>
          set(
            proxy.port === defaultProxyPort(proxy.kind)
              ? { kind, port: defaultProxyPort(kind) }
              : { kind },
          )
        }
      />
      <TextSetting
        label={t("settings.host")}
        value={proxy.host}
        placeholder="proxy.example.com"
        disabled={off}
        onCommit={(host) => set({ host })}
      />
      <NumberSetting
        label={t("settings.port")}
        value={proxy.port}
        min={1}
        max={65535}
        disabled={off}
        onChange={(port) => set({ port })}
      />
      <TextSetting
        label={t("settings.username")}
        hint={t("settings.usernameHint")}
        value={proxy.username}
        disabled={off}
        onCommit={(username) => set({ username })}
      />
      <SettingRow label={t("settings.password")} hint={t("settings.passwordHint")} disabled={off}>
        <span className="input-with-button">
          <ProxyPasswordInput hasPassword={proxy.hasPassword} disabled={off} onSave={setPassword} />
          {proxy.hasPassword && (
            <button type="button" className="button" disabled={off} onClick={() => setPassword("")}>
              {t("settings.forget")}
            </button>
          )}
        </span>
      </SettingRow>
      <SwitchSetting
        label={t("settings.lookUpServerNamesThrough")}
        hint={t("settings.lookUpServerNamesThroughHint")}
        checked={proxy.kind === "http" || proxy.remoteDns}
        disabled={off || proxy.kind === "http"}
        onChange={(remoteDns) => set({ remoteDns })}
      />
    </SettingGroup>
  );
}

function ProxyPasswordInput({
  hasPassword,
  disabled,
  onSave,
}: {
  hasPassword: boolean;
  disabled: boolean;
  onSave: (password: string) => void;
}) {
  const t = useT();
  const [typed, setTyped] = useState("");
  const save = () => {
    if (typed) onSave(typed);
    setTyped("");
  };
  return (
    <input
      type="password"
      value={typed}
      aria-label={t("settings.proxyPassword")}
      placeholder={hasPassword ? t("settings.saved") : ""}
      disabled={disabled}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={save}
      onKeyDown={(event) => {
        if (event.key === "Enter") save();
      }}
    />
  );
}

function InterfaceSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const options = settings.interface;
  const set = (change: Partial<Settings["interface"]>) =>
    void saveSettingsSection("interface", change);
  return (
    <>
      <SettingGroup title={t("settings.language")}>
        <SettingRow label={t("settings.language")} hint={t("settings.languageHint")}>
          <LanguagePicker
            label={t("settings.language")}
            value={options.language}
            onChange={(language) => set({ language })}
          />
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={t("settings.files")}>
        <SelectSetting
          label={t("settings.doubleClickingAFile")}
          value={options.doubleClickFile}
          options={[
            { value: "transfer", label: t("settings.transfersItToTheOther") },
            { value: "edit", label: t("settings.opensItInTheBuilt") },
            { value: "open", label: t("settings.opensItInItsProgram") },
            { value: "ask", label: t("settings.asksHowToOpenIt") },
            { value: "nothing", label: t("settings.doesNothing") },
          ]}
          onChange={(doubleClickFile) => set({ doubleClickFile })}
        />
        <SwitchSetting
          label={t("settings.showHiddenFilesInNew")}
          checked={options.showHiddenFiles}
          onChange={(showHiddenFiles) => set({ showHiddenFiles })}
        />
        <SwitchSetting
          label={t("settings.foldersFirst")}
          hint={t("settings.foldersFirstHint")}
          checked={options.foldersFirst}
          onChange={(foldersFirst) => set({ foldersFirst })}
        />
        <SelectSetting
          label={t("settings.dates")}
          value={options.dateFormat}
          options={DATE_FORMATS.map((format) => ({
            value: format,
            label: formatDate(DATE_SAMPLE, format),
          }))}
          onChange={(dateFormat) => set({ dateFormat })}
        />
        <SettingRow label={t("settings.columns")} hint={t("settings.columnsHint")}>
          <div className="column-toggles">
            {DETAIL_COLUMNS.map((column) => {
              const shown = !options.hiddenColumns.includes(column);
              return (
                <button
                  key={column}
                  type="button"
                  className={`toggle ${shown ? "is-on" : ""}`}
                  aria-pressed={shown}
                  onClick={() =>
                    set({
                      hiddenColumns: shown
                        ? [...options.hiddenColumns, column]
                        : options.hiddenColumns.filter((hidden) => hidden !== column),
                    })
                  }
                >
                  {COLUMN_LABELS[column]}
                </button>
              );
            })}
          </div>
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={t("settings.serverFolders")}>
        <SwitchSetting
          label={t("settings.showCachedListingsWhileRefreshing")}
          hint={t("settings.showCachedListingsWhileRefreshingHint")}
          checked={options.cachedListings}
          onChange={(cachedListings) => set({ cachedListings })}
        />
        <SwitchSetting
          label={t("settings.loadNearbyFoldersAhead")}
          hint={t("settings.loadNearbyFoldersAheadHint")}
          checked={options.prefetchListings}
          disabled={!options.cachedListings}
          onChange={(prefetchListings) => set({ prefetchListings })}
        />
        <SwitchSetting
          label={t("settings.alsoOnCloudStorage")}
          hint={t("settings.alsoOnCloudStorageHint")}
          checked={options.prefetchOnCloud}
          disabled={!options.cachedListings || !options.prefetchListings}
          onChange={(prefetchOnCloud) => set({ prefetchOnCloud })}
        />
      </SettingGroup>
      <ExternalEditorSettings settings={settings} />
      <PreviewSettingsGroup settings={settings} />
      <SettingGroup title={t("settings.tabsAndWindows")}>
        <SwitchSetting
          label={t("settings.askBeforeClosingATab")}
          checked={options.confirmCloseWithTransfers}
          onChange={(confirmCloseWithTransfers) => set({ confirmCloseWithTransfers })}
        />
        <SwitchSetting
          label={t("settings.rememberTabsAndLayout")}
          hint={t("settings.rememberTabsAndLayoutHint")}
          checked={options.rememberLayout}
          onChange={(rememberLayout) => set({ rememberLayout })}
        />
        <SwitchSetting
          label={t("settings.reconnectToServersOnStart")}
          hint={t("settings.reconnectToServersOnStartHint")}
          checked={options.reconnectOnStart}
          disabled={!options.rememberLayout}
          onChange={(reconnectOnStart) => set({ reconnectOnStart })}
        />
        <SwitchSetting
          label={t("settings.useTheSystemTitleBar")}
          hint={t("settings.useTheSystemTitleBarHint")}
          checked={options.systemTitleBar}
          onChange={(systemTitleBar) => set({ systemTitleBar })}
        />
      </SettingGroup>
    </>
  );
}

function LogSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const log = settings.log;
  const set = (change: Partial<Settings["log"]>) => void saveSettingsSection("log", change);
  return (
    <SettingGroup title={t("settings.log")}>
      <NumberSetting
        label={t("settings.linesToKeep")}
        value={log.maxLines}
        min={LOG_LINE_LIMITS.min}
        max={LOG_LINE_LIMITS.max}
        onChange={(maxLines) => set({ maxLines })}
      />
      <SwitchSetting
        label={t("settings.showTimes")}
        checked={log.timestamps}
        onChange={(timestamps) => set({ timestamps })}
      />
      <SwitchSetting
        label={t("settings.wrapLongLines")}
        checked={log.wrapLines}
        onChange={(wrapLines) => set({ wrapLines })}
      />
      <SwitchSetting
        label={t("settings.logEveryTransferredFile")}
        hint={t("settings.logEveryTransferredFileHint")}
        checked={settings.transfers.logEachFile}
        onChange={(logEachFile) => void saveSettingsSection("transfers", { logEachFile })}
      />
    </SettingGroup>
  );
}

function UpdateSettingsPage({ settings }: { settings: Settings }) {
  const t = useT();
  const openDialog = useUiStore((state) => state.open);
  const update = useUpdateStore((state) => state.update);
  const target = useUpdateStore((state) => state.target);
  const checking = useUpdateStore((state) => state.checking);
  const checkedAt = useUpdateStore((state) => state.checkedAt);
  const checkError = useUpdateStore((state) => state.checkError);
  const install = useUpdateStore((state) => state.install);
  const restartWarnings = useRestartWarnings();

  const status = checking
    ? t("updates.checkingForUpdates")
    : target && install
      ? [
          describeInstall(install, formatVersion(target.version)),
          ...(install.stage === "ready" ? restartWarnings : []),
        ].join(t("common.sentenceSeparator"))
      : (checkError ??
        (update
          ? t("updates.versionIsAvailable", { version: formatVersion(update.version) })
          : checkedAt !== null
            ? t("updates.upToDateCheckedAt", { time: formatTime(checkedAt) })
            : t("updates.theVersionYouAreRunning")));

  return (
    <SettingGroup title={t("settings.updates")}>
      <SwitchSetting
        label={t("settings.checkForUpdatesWhenPoros")}
        hint={t("settings.checkForUpdatesWhenPorosHint")}
        checked={settings.updates.checkOnStart}
        onChange={(checkOnStart) => void saveSettingsSection("updates", { checkOnStart })}
      />
      <SettingRow
        label={t("settings.poros", { version: formatVersion(__APP_VERSION__) })}
        hint={status}
      >
        {install?.stage === "ready" ? (
          <button type="button" className="button" onClick={() => void restartToUpdate()}>
            {t("settings.restartNow")}
          </button>
        ) : isInstalling(install) ? (
          <button type="button" className="button" disabled>
            {t("settings.updating")}
          </button>
        ) : update && !checking ? (
          <button
            type="button"
            className="button"
            onClick={() =>
              openDialog({ kind: "update", returnTo: { kind: "settings", section: "updates" } })
            }
          >
            {t("settings.showUpdate")}
          </button>
        ) : (
          <button
            type="button"
            className="button"
            disabled={checking}
            onClick={() => void checkForUpdates("manual")}
          >
            {checking ? t("settings.checking") : t("settings.checkForUpdates")}
          </button>
        )}
      </SettingRow>
      <OtherVersionSetting />
    </SettingGroup>
  );
}

type VersionList =
  | { status: "unlisted" }
  | { status: "listing" }
  | { status: "listed"; versions: string[] }
  | { status: "failed"; message: string };

/** Lists the releases only when asked, since that asks GitHub, then installs the one picked. */
function OtherVersionSetting() {
  const t = useT();
  const install = useUpdateStore((state) => state.install);
  const closeDialogs = useUiStore((state) => state.close);
  const [list, setList] = useState<VersionList>({ status: "unlisted" });
  const [picked, setPicked] = useState("");
  const [finding, setFinding] = useState(false);
  const [findError, setFindError] = useState<string | null>(null);

  const listVersions = async () => {
    setList({ status: "listing" });
    try {
      const versions = await updates.versions();
      setList({ status: "listed", versions });
      setPicked(versions[0] ?? "");
    } catch (caught) {
      setList({ status: "failed", message: toAppError(caught).message });
    }
  };
  const installPicked = async () => {
    setFinding(true);
    setFindError(null);
    try {
      await installVersion(picked);
      // The notice in the corner shows the download, and dialogs cover it. One opened since
      // Settings was closed stays open.
      if (useUiStore.getState().dialog?.kind === "settings") closeDialogs();
    } catch (caught) {
      setFindError(
        t("settings.isNotAvailable", {
          version: formatVersion(picked),
          message: toAppError(caught).message,
        }),
      );
      setFinding(false);
    }
  };

  const hint =
    list.status === "failed"
      ? t("updates.couldNotListTheReleases", { message: list.message })
      : list.status === "listed" && list.versions.length === 0
        ? t("updates.noOtherReleaseToInstall", { version: formatVersion(OLDEST_INSTALLABLE) })
        : (findError ??
          t("updates.switchToAnotherRelease", { version: formatVersion(OLDEST_INSTALLABLE) }));

  return (
    <SettingRow label={t("settings.installAnotherVersion")} hint={hint}>
      {list.status === "listed" ? (
        list.versions.length > 0 && (
          <div className="version-picker">
            <select
              value={picked}
              aria-label={t("settings.version")}
              onChange={(event) => setPicked(event.target.value)}
            >
              {list.versions.map((version) => (
                <option key={version} value={version}>
                  {formatVersion(version)}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="button"
              disabled={finding || isInstalling(install)}
              onClick={() => void installPicked()}
            >
              {t("settings.install")}
            </button>
          </div>
        )
      ) : (
        <button
          type="button"
          className="button"
          disabled={list.status === "listing"}
          onClick={() => void listVersions()}
        >
          {list.status === "listing" ? t("settings.listing") : t("settings.showVersions")}
        </button>
      )}
    </SettingRow>
  );
}

function describeInstall(install: InstallStage, version: string): string {
  switch (install.stage) {
    case "downloading": {
      const percent = percentDownloaded(install.received, install.total);
      return percent === null
        ? t("updates.downloadingVersion", { version })
        : t("updates.downloadingVersionPercent", { version, percent: formatPercent(percent) });
    }
    case "installing":
      return t("updates.installingVersion", { version });
    case "ready":
      return t("updates.versionIsReadyRestart", { version });
    case "restarting":
      return t("updates.restartingInto", { version });
    case "failed":
      return t("updates.couldNotInstallVersionBecause", { version, message: install.message });
  }
}
