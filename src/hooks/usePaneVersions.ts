import { useCallback, useEffect, useRef, useState } from "react";
import type { FileSource } from "../lib/fileSource";
import { toAppError, versions } from "../lib/ipc";
import type { Versioning } from "../lib/types";
import { hasVersions, keepsVersions, mergeDeleted } from "../lib/versions";
import { useSessionStore } from "../state/sessionStore";
import { useToastStore } from "../state/toastStore";
import { learnVersioning, useVersioningStore, versioningKey } from "../state/versionActions";
import type { Handover } from "./usePane";

export interface PaneVersions {
  /** The pane is on a server that keeps histories of files, outside any vault. */
  available: boolean;
  showDeleted: boolean;
  /** Shows or hides deleted files; the pane lists the folder again to apply it. */
  setShowDeleted: (shown: boolean) => void;
  /** Lists a folder with what was deleted from it while deleted files are shown. */
  deletedView: (path: string) => Handover | null;
}

/**
 * Deleted files in a server pane: while they are shown, folders in buckets that keep versions
 * are listed with them, and otherwise as usual, so a pane that does not show them asks for no
 * more than it did.
 */
export function usePaneVersions(source: FileSource, sessionId: string | undefined): PaneVersions {
  const available = useSessionStore((state) => {
    const info = sessionId ? state.sessions[sessionId]?.info : undefined;
    return Boolean(info && !info.vault && hasVersions(info.protocol));
  });
  const [showDeleted, setShown] = useState(false);
  // Read as the pane lists, which can be before the next render.
  const shownRef = useRef(false);
  const views = useRef(0);

  const setShowDeleted = useCallback((shown: boolean) => {
    shownRef.current = shown;
    setShown(shown);
  }, []);

  const deletedView = useCallback(
    (path: string): Handover | null => {
      if (!available || !sessionId || !shownRef.current) return null;
      const key = versioningKey(sessionId, path);
      if (key === null || !keepsVersions(useVersioningStore.getState().known[key])) return null;
      views.current += 1;
      return {
        source: {
          ...source,
          // A pane takes a listing from the same source as a newer state of the one on show and
          // keeps the rows that look the same, which would keep a restored folder marked
          // deleted: each listing with deleted files is one of its own.
          key: `${source.key}#deleted-${views.current}`,
          // Never remembered as the folder's listing, nor shown in place of one remembered.
          listingScope: undefined,
          list: async (folder) => {
            try {
              const found = await versions.listWithDeleted(sessionId, folder);
              return mergeDeleted(found.listing, found.deleted);
            } catch (caught) {
              // The folder still shows; only what was deleted from it is missing.
              setShowDeleted(false);
              const { message } = toAppError(caught);
              useToastStore.getState().show("error", `Deleted files cannot be shown: ${message}`);
              return source.list(folder);
            }
          },
        },
        take: () => undefined,
      };
    },
    [available, sessionId, source, setShowDeleted],
  );

  return { available, showDeleted, setShowDeleted, deletedView };
}

/** Whether the bucket of the folder on show keeps versions, asked once for each bucket. */
export function useBucketVersioning(
  sessionId: string | undefined,
  path: string | undefined,
  available: boolean,
): Versioning | undefined {
  const key = sessionId && path && available ? versioningKey(sessionId, path) : null;
  const versioning = useVersioningStore((state) => (key ? state.known[key] : undefined));
  useEffect(() => {
    if (sessionId && path && available) learnVersioning(sessionId, path);
  }, [sessionId, path, available]);
  return versioning;
}
