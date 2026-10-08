import { ArrowDown, ArrowUp, CircleArrowUp } from "lucide-react";
import { formatDuration, formatSize, formatSpeed, formatVersion, pluralize } from "../lib/format";
import { findGroup } from "../lib/layout";
import { secondsRemaining } from "../lib/transfers";
import { useLayoutStore } from "../state/layoutStore";
import { useSessionStore } from "../state/sessionStore";
import { useSettingsStore } from "../state/settingsStore";
import { useTransferStore } from "../state/transferStore";
import { useUiStore } from "../state/uiStore";
import { useUpdateStore } from "../state/updateStore";

const APP_VERSION = formatVersion(__APP_VERSION__);

function useActiveSessionText(): { text: string; tone: "connected" | "lost" | "disconnected" } {
  const activeSessionId = useLayoutStore((state) => {
    const group = findGroup(state.root, state.activeGroupId);
    const tab = group?.tabs.find((candidate) => candidate.id === group.activeTabId);
    return tab?.kind === "remote" ? tab.sessionId : null;
  });
  const entry = useSessionStore((state) =>
    activeSessionId ? state.sessions[activeSessionId] : undefined,
  );
  const count = useSessionStore((state) => Object.keys(state.sessions).length);
  if (entry?.status === "lost") {
    return { text: `Connection to ${entry.info.label} lost`, tone: "lost" };
  }
  if (entry) return { text: `Connected to ${entry.info.label}`, tone: "connected" };
  if (count > 0) return { text: `${pluralize(count, "server")} connected`, tone: "connected" };
  return { text: "Not connected", tone: "disconnected" };
}

/** The newer version found by the last check, unless the user skipped it. */
function useOfferedUpdate(): string | null {
  const version = useUpdateStore((state) => state.update?.version ?? null);
  const skipped = useSettingsStore((state) => state.settings.updates.skippedVersion);
  return version !== null && version !== skipped ? version : null;
}

export function StatusBar() {
  const session = useActiveSessionText();
  const stats = useTransferStore((state) => state.snapshot.stats);
  const pending = stats.counts.queued + stats.counts.running;
  const remaining = secondsRemaining(stats);
  const offeredUpdate = useOfferedUpdate();
  const openDialog = useUiStore((state) => state.open);

  return (
    <footer className="statusbar">
      <span className="status-item">
        <span className={`status-dot status-${session.tone}`} />
        {session.text}
      </span>
      <span className="status-right">
        {pending > 0 && (
          <span className="status-item status-queue">
            {pluralize(pending, "transfer")} left
            {stats.remainingBytes > 0 && `, ${formatSize(stats.remainingBytes)}`}
            {remaining !== null && `, about ${formatDuration(remaining)}`}
            {stats.queuePaused && " (paused)"}
          </span>
        )}
        <span className="status-item status-speed" title="Upload speed">
          <ArrowUp size={12} className={stats.uploadSpeed > 0 ? "is-moving" : ""} />
          {formatSpeed(stats.uploadSpeed)}
        </span>
        <span className="status-item status-speed" title="Download speed">
          <ArrowDown size={12} className={stats.downloadSpeed > 0 ? "is-moving" : ""} />
          {formatSpeed(stats.downloadSpeed)}
        </span>
        {offeredUpdate && (
          <button
            type="button"
            className="status-item status-update"
            onClick={() => openDialog({ kind: "update" })}
          >
            <CircleArrowUp size={12} />
            Update to {formatVersion(offeredUpdate)}
          </button>
        )}
        <span className="status-item status-version">Poros {APP_VERSION}</span>
      </span>
    </footer>
  );
}
