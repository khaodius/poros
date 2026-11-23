import { useMemo } from "react";
import type { PaneController } from "../hooks/usePane";
import type { FileSource } from "../lib/fileSource";
import { listsExactTimes, paneStorage, subjectOf } from "../lib/previewPlan";
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
  // A listing remembered from before, on show while the server is asked again or when it could
  // not be: its sizes and times decide nothing until a read compares them with the file's.
  const remembered = pane.stale !== null;
  const place = useMemo<PreviewPlace>(
    () => ({
      connection: source.key,
      sessionId: remoteSession,
      storage: paneStorage(remoteSession !== undefined, protocol),
      // Not compared with the files' until the session is known here.
      exactTimes: remoteSession ? protocol !== undefined && listsExactTimes(protocol) : true,
      inVault,
      sizes: remembered ? "remembered" : remoteSession ? "listed" : "exact",
    }),
    [source.key, remoteSession, protocol, inVault, remembered],
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
