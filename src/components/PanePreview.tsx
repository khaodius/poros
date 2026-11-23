import { useMemo } from "react";
import type { PaneController } from "../hooks/usePane";
import type { FileSource } from "../lib/fileSource";
import { listsExactTimes, storageKind, subjectOf } from "../lib/previewPlan";
import type { DateFormat } from "../lib/settings";
import type { FileEntry } from "../lib/types";
import type { PreviewPlace } from "../state/previewReads";
import { serverEntry, useSessionStore } from "../state/sessionStore";
import { PreviewPane } from "./PreviewPane";

interface PanePreviewProps {
  pane: PaneController;
  source: FileSource;
  /** For server panes. */
  sessionId?: string;
  dateFormat: DateFormat;
  onOpenInEditor: (entry: FileEntry) => void;
  onSwitchPane: (from: HTMLElement) => void;
  onLeave: () => void;
}

/** The preview of a file pane: what is under the pane's cursor, read from where the pane is. */
export function PanePreview({
  pane,
  source,
  sessionId,
  dateFormat,
  onOpenInEditor,
  onSwitchPane,
  onLeave,
}: PanePreviewProps) {
  const remoteSession = source.kind === "remote" ? sessionId : undefined;
  // A vault's bytes come over the connection of the server it is on.
  const protocol = useSessionStore((state) =>
    remoteSession ? serverEntry(state.sessions, remoteSession)?.info.protocol : undefined,
  );
  const inVault = useSessionStore((state) =>
    Boolean(remoteSession && state.sessions[remoteSession]?.info.vault),
  );
  const place = useMemo<PreviewPlace>(
    () => ({
      connection: source.key,
      sessionId: remoteSession,
      // A session not known here is treated as a server's until it is.
      storage: remoteSession ? storageKind(protocol ?? "sftp") : "local",
      exactTimes: listsExactTimes(protocol),
      inVault,
    }),
    [source.key, remoteSession, protocol, inVault],
  );
  const { visibleEntries, cursorPath, selectedEntries, refresh } = pane;
  const subject = useMemo(
    () =>
      subjectOf(
        visibleEntries.find((entry) => entry.path === cursorPath),
        selectedEntries(),
      ),
    [visibleEntries, cursorPath, selectedEntries],
  );
  return (
    <PreviewPane
      place={place}
      subject={subject}
      dateFormat={dateFormat}
      onOpenInEditor={onOpenInEditor}
      onRefresh={() => void refresh()}
      onSwitchPane={onSwitchPane}
      onLeave={onLeave}
    />
  );
}
