import { ArrowDown, ArrowUp, CircleArrowUp } from "lucide-react";
import {
  formatDuration,
  formatPercent,
  formatSize,
  formatSpeed,
  formatVersion,
} from "../lib/format";
import { findGroup } from "../lib/layout";
import { secondsRemaining, showsPaused } from "../lib/transfers";
import { useLayoutStore } from "../state/layoutStore";
import { serverEntry, useSessionStore } from "../state/sessionStore";
import { useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import { useUiStore } from "../state/uiStore";
import { ExternalEditsStatus } from "./ExternalEditsStatus";
import {
  percentDownloaded,
  showUpdateNotice,
  useUpdateStore,
  type InstallStage,
} from "../state/updateStore";
import { DriveStatus } from "./DriveStatus";
import { LimitScheduleStatus } from "./LimitScheduleStatus";
import { t, useT } from "../i18n";

const APP_VERSION = formatVersion(__APP_VERSION__);

function useActiveSessionText(): { text: string; tone: "connected" | "lost" | "disconnected" } {
  const t = useT();
  const activeSessionId = useLayoutStore((state) => {
    const group = findGroup(state.root, state.activeGroupId);
    const tab = group?.tabs.find((candidate) => candidate.id === group.activeTabId);
    return tab?.kind === "remote" ? tab.sessionId : null;
  });
  // A tab in an unlocked vault is connected to the vault's server.
  const entry = useSessionStore((state) =>
    activeSessionId ? serverEntry(state.sessions, activeSessionId) : undefined,
  );
  const count = useSessionStore(
    (state) => Object.values(state.sessions).filter((session) => !session.info.vault).length,
  );
  if (entry?.status === "lost") {
    return { text: t("workspace.connectionToLost", { label: entry.info.label }), tone: "lost" };
  }
  if (entry)
    return { text: t("workspace.connectedTo", { label: entry.info.label }), tone: "connected" };
  if (count > 0) return { text: t("workspace.serversConnected", { count }), tone: "connected" };
  return { text: t("workspace.notConnected"), tone: "disconnected" };
}

/** The newer version found by the last check, unless the user skipped it. */
function useOfferedUpdate(): string | null {
  const version = useUpdateStore((state) => state.update?.version ?? null);
  const skipped = useSettingsStore((state) => state.settings.updates.skippedVersion);
  return version !== null && version !== skipped ? version : null;
}

/** The update the last check found, or how an install goes; opens the dialog or the notice. */
function UpdateStatus() {
  const t = useT();
  const offeredUpdate = useOfferedUpdate();
  const version = useUpdateStore((state) => state.target?.version ?? null);
  const install = useUpdateStore((state) => state.install);
  const openDialog = useUiStore((state) => state.open);
  if (version !== null && install) {
    return (
      <button type="button" className="status-item status-update" onClick={showUpdateNotice}>
        <CircleArrowUp size={12} />
        {describeInstall(install, formatVersion(version))}
      </button>
    );
  }
  if (!offeredUpdate) return null;
  return (
    <button
      type="button"
      className="status-item status-update"
      onClick={() => openDialog({ kind: "update" })}
    >
      <CircleArrowUp size={12} />
      {t("workspace.updateTo", { version: formatVersion(offeredUpdate) })}
    </button>
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
      return t("updates.restartToUse", { version });
    case "restarting":
      return t("updates.restarting");
    case "failed":
      return t("updates.couldNotInstallVersion", { version });
  }
}

/** `3 transfers left, 1.2 GB, about 4m 10s`, with `(paused)` after it while the queue waits. */
function queueStatus(
  pending: number,
  remainingBytes: number,
  remainingSeconds: number | null,
  paused: boolean,
): string {
  const facts = [
    t("workspace.transfersLeft", { count: pending }),
    remainingBytes > 0 ? formatSize(remainingBytes) : null,
    remainingSeconds !== null
      ? t("workspace.aboutDuration", { duration: formatDuration(remainingSeconds) })
      : null,
  ].filter((fact) => fact !== null);
  const status = facts.join(t("common.listSeparator"));
  return paused ? t("workspace.queueStatusPaused", { status }) : status;
}

export function StatusBar() {
  const t = useT();
  const session = useActiveSessionText();
  const stats = useTransferStore((state) => state.snapshot.stats);
  const pending = stats.counts.queued + stats.counts.running;
  const remaining = secondsRemaining(stats);

  return (
    <footer className="statusbar">
      <span className="status-item">
        <span className={`status-dot status-${session.tone}`} />
        {session.text}
      </span>
      <span className="status-right">
        <DriveStatus />
        <ExternalEditsStatus />
        {pending > 0 && (
          <span className="status-item status-queue">
            {queueStatus(pending, stats.remainingBytes, remaining, showsPaused(stats))}
          </span>
        )}
        <LimitScheduleStatus />
        <span className="status-item status-speed" title={t("workspace.uploadSpeed")}>
          <ArrowUp size={12} className={stats.uploadSpeed > 0 ? "is-moving" : ""} />
          {formatSpeed(stats.uploadSpeed)}
        </span>
        <span className="status-item status-speed" title={t("workspace.downloadSpeed")}>
          <ArrowDown size={12} className={stats.downloadSpeed > 0 ? "is-moving" : ""} />
          {formatSpeed(stats.downloadSpeed)}
        </span>
        <UpdateStatus />
        <span className="status-item status-version">
          {t("workspace.appVersion", { version: APP_VERSION })}
        </span>
      </span>
    </footer>
  );
}
