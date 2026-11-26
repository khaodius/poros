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
  /** Rows on show that are not there to read: deleted files shown among the others. */
  leftOut?: ReadonlySet<string>;
  /** The picture asked for with Preview in the list's menu, while it still applies. */
  request?: string | null;
  onRequestDone?: () => void;
}

/**
 * The preview of a file pane: what is under the pane's cursor, read from where the pane is. On
 * search results that is the result under the cursor, read by its own path in whatever folder
 * it was found, and Refresh is the pane's own, which searches again.
 */
export function PanePreview({
  pane,
  source,
  sessionId,
  dateFormat,
  onOpenInEditor,
  onSwitchPane,
  onLeave,
  leftOut,
  request,
  onRequestDone,
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
  // Search results are never remembered: they are as the search found them.
  const remembered = pane.stale !== null && !pane.showingResults;
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
        leftOut,
      ),
    [visibleEntries, cursorPath, selectedEntries, leftOut],
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
      request={request}
      onRequestDone={onRequestDone}
    />
  );
}
