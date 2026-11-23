import { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from "react";
import type { FileSource } from "../lib/fileSource";
import {
  legendCounts,
  outdatedNote,
  namesIn,
  sizeOnlyIn,
  type LegendCounts,
  type MarkedState,
  type RowCompare,
} from "../lib/folderCompare";
import type { FolderCompareSettings } from "../lib/settings";
import { isDirLike } from "../lib/sort";
import { paneKey, snapshotOf, type DeepState, type PaneSnapshot } from "../state/folderCompare";
import {
  WINDOW_LABEL,
  connectFolderCompare,
  folderCompare,
  useFolderCompareState,
} from "../state/folderCompareStore";
import { serverEntry, useSessionStore } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";
import { useUiStore } from "../state/uiStore";
import type { PaneController } from "./usePane";

/** What the bar over a compared pane shows and does. */
export interface FolderCompareBarModel {
  partner: { label: string; path: string | null; otherWindow: boolean };
  /** Rows per state among those the pane can show; null while there is nothing to compare. */
  counts: LegendCounts | null;
  deep: DeepState | undefined;
  /** Why the two panes stopped browsing together. */
  note: string | undefined;
  /** Why the marks may be out of date: a listing on show could not be refreshed. */
  outdated: string | undefined;
  options: FolderCompareSettings;
  /** Size alone decides, by the bar's option or by Settings > Sync. */
  sizeOnly: boolean;
  setOptions: (change: Partial<FolderCompareSettings>) => void;
  startDeep: () => void;
  stopDeep: () => void;
  dismissDeep: () => void;
  /** Selects the rows in these states. */
  select: (states: readonly MarkedState[]) => void;
  /** Opens Synchronize folders with the two folders, or says why it cannot. */
  synchronize: { open: () => void } | { unavailable: string };
  close: () => void;
}

export interface FolderCompareControls {
  comparing: boolean;
  /** Compares this pane with the other one looked at last, or stops comparing both. */
  toggle: () => void;
  /** The mark of each compared row, by name. */
  marks: ReadonlyMap<string, RowCompare> | undefined;
  bar: FolderCompareBarModel | null;
}

/** The marks of a pane's rows while it is compared and both panes show a folder. */
function usePaneMarks(tabId: string): ReadonlyMap<string, RowCompare> | undefined {
  return useFolderCompareState((state) => {
    const pairId = state.pairOf[tabId];
    const pair = pairId ? state.pairs[pairId] : undefined;
    const found = pairId ? state.comparisons[pairId] : undefined;
    if (!pair || !found) return undefined;
    const left = pair.left.window === WINDOW_LABEL && pair.left.tabId === tabId;
    return left ? found.left : found.right;
  });
}

/** The names a compared pane leaves out while identical items are hidden. */
export function useHiddenIdentical(tabId: string): ReadonlySet<string> | undefined {
  const hide = useSettingsStore((state) => state.settings.interface.folderCompare.hideIdentical);
  const marks = usePaneMarks(tabId);
  return useMemo(() => (hide && marks ? namesIn(marks, ["identical"]) : undefined), [hide, marks]);
}

interface ComparedPane {
  tabId: string;
  source: FileSource;
  sessionId?: string;
  pane: PaneController;
  /** Something stands in for the file list, as a locked vault's card does. */
  covered: boolean;
  /** Gives the keyboard back to the file list. */
  focusList: () => void;
}

/**
 * A file pane's part in Compare folders: it tells the comparison what it shows, goes where its
 * partner goes while the two browse together, and gets back the marks of its rows and what its
 * compare bar needs.
 */
export function useFolderCompare({
  tabId,
  source,
  sessionId,
  pane,
  covered,
  focusList,
}: ComparedPane): FolderCompareControls {
  const pairId = useFolderCompareState((state) => state.pairOf[tabId]);
  const pair = useFolderCompareState((state) => (pairId ? state.pairs[pairId] : undefined));
  const deep = useFolderCompareState((state) => (pairId ? state.deep[pairId] : undefined));
  const note = useFolderCompareState((state) => (pairId ? state.notes[pairId] : undefined));
  const marks = usePaneMarks(tabId);
  const onLeft = pair?.left.window === WINDOW_LABEL && pair.left.tabId === tabId;
  const partner = pair ? (onLeft ? pair.right : pair.left) : undefined;
  const partnerKey = partner ? paneKey(partner) : undefined;
  const partnerSnapshot = useFolderCompareState((state) =>
    partnerKey ? state.snapshots[partnerKey] : undefined,
  );
  const options = useSettingsStore((state) => state.settings.interface.folderCompare);
  const syncCompare = useSettingsStore((state) => state.settings.sync.options.compare);
  const info = useSessionStore((state) =>
    sessionId ? state.sessions[sessionId]?.info : undefined,
  );
  // A vault's files come from its server, so they are out of reach while the server is.
  const lost = useSessionStore((state) =>
    sessionId ? serverEntry(state.sessions, sessionId)?.status === "lost" : false,
  );
  const showToast = useToastStore((state) => state.show);
  const openDialog = useUiStore((state) => state.open);

  useEffect(connectFolderCompare, []);
  useEffect(() => () => folderCompare.leave(tabId), [tabId]);

  const latest = useRef({ pane, covered });
  useLayoutEffect(() => {
    latest.current = { pane, covered };
  });

  const comparing = pairId !== undefined;
  const listing = pane.listing;
  const outdated = pane.stale === "outdated";
  const snapshot = useMemo<PaneSnapshot | null>(
    () =>
      comparing
        ? snapshotOf({
            listing,
            location:
              source.kind === "remote" && sessionId
                ? { kind: "remote", sessionId }
                : { kind: "local" },
            pathStyle: source.pathStyle,
            info,
            lost,
            covered,
            outdated,
          })
        : null,
    [comparing, listing, covered, source.kind, source.pathStyle, sessionId, info, lost, outdated],
  );
  // Before the browser paints, so a folder never shows with the marks of the one before it.
  useLayoutEffect(() => {
    if (pairId) folderCompare.publish(tabId, snapshot);
  }, [tabId, pairId, snapshot]);

  useEffect(
    () =>
      folderCompare.attach(tabId, {
        follow: (move, ignoreCase) => {
          const { pane: current, covered: hidden } = latest.current;
          if (!current.listing || hidden) return null;
          if (move.kind === "up") {
            const parent = current.listing.parent;
            return parent ? { path: parent, done: current.navigate(parent) } : null;
          }
          const folders = current.listing.entries.filter(isDirLike);
          const wanted = move.name.toLowerCase();
          const target =
            folders.find((entry) => entry.name === move.name) ??
            (ignoreCase ? folders.find((entry) => entry.name.toLowerCase() === wanted) : undefined);
          return target ? { path: target.path, done: current.navigate(target.path) } : null;
        },
      }),
    [tabId],
  );

  const toggle = useCallback(() => {
    void folderCompare.toggle(tabId).then((reason) => {
      if (reason) showToast("info", `${reason}.`);
    });
  }, [tabId, showToast]);

  const countable = useMemo(
    () => (pane.showHidden ? listing?.entries : listing?.entries.filter((entry) => !entry.hidden)),
    [listing, pane.showHidden],
  );
  const counts = useMemo(
    () => (marks && countable ? legendCounts(countable, marks) : null),
    [marks, countable],
  );

  const [local, remote] =
    snapshot?.location.kind === "local" ? [snapshot, partnerSnapshot] : [partnerSnapshot, snapshot];
  const remoteSession =
    remote?.location.kind === "remote" && remote.protocol === "sftp"
      ? remote.location.sessionId
      : undefined;
  const sessionHere = useSessionStore((state) =>
    remoteSession ? remoteSession in state.sessions : false,
  );

  if (!pair || !partner) return { comparing: false, toggle, marks: undefined, bar: null };

  const synchronize: FolderCompareBarModel["synchronize"] =
    local?.location.kind !== "local" || !remote || !remoteSession
      ? { unavailable: "between this computer and an SFTP server only" }
      : !sessionHere
        ? { unavailable: "from the window with the server's tab" }
        : {
            open: () =>
              openDialog({
                kind: "sync",
                localPath: local.path,
                sessionId: remoteSession,
                remotePath: remote.path,
              }),
          };

  const select = (states: readonly MarkedState[]) => {
    if (!marks) return;
    const paths = pane.visibleEntries
      .filter((entry) => {
        const mark = marks.get(entry.name);
        return mark !== undefined && states.includes(mark.state);
      })
      .map((entry) => entry.path);
    pane.selectPaths(new Set(paths), paths[0], paths[0]);
    focusList();
  };

  return {
    comparing: true,
    toggle,
    marks,
    bar: {
      partner: {
        label: partner.label,
        path: partnerSnapshot?.path ?? null,
        otherWindow: partner.window !== WINDOW_LABEL,
      },
      counts,
      deep,
      note,
      outdated: outdatedNote(
        snapshot?.outdated === true,
        partnerSnapshot?.outdated === true,
        partner.label,
      ),
      options,
      sizeOnly: sizeOnlyIn(options, syncCompare),
      setOptions: (change) =>
        void saveSettingsSection("interface", { folderCompare: { ...options, ...change } }),
      startDeep: () => void folderCompare.startDeep(tabId),
      stopDeep: () => folderCompare.stopDeep(tabId),
      dismissDeep: () => folderCompare.dismissDeep(tabId),
      select,
      synchronize,
      // The bar and its buttons go away, so the keyboard goes back to the rows.
      close: () => {
        toggle();
        focusList();
      },
    },
  };
}
