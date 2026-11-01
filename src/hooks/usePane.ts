import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FileSource } from "../lib/fileSource";
import { entryLeadingTo } from "../lib/cursor";
import {
  dateGroupFinder,
  firstDayOfWeek,
  groupEntriesByDate,
  type DateGroup,
  type EntryGroup,
} from "../lib/dateGroups";
import { toAppError } from "../lib/ipc";
import { ListingLoads } from "../lib/listingLoads";
import { filterEntries, isDirLike, sortEntries, type SortKey, type SortSpec } from "../lib/sort";
import type { AppError, DirListing, FileEntry } from "../lib/types";
import { useToday } from "./useToday";

const WEEK_STARTS_ON = firstDayOfWeek(navigator.language);

interface HistoryEntry {
  path: string;
  /** Where the cursor was on leaving the folder, to put it back on returning. */
  cursor: string | null;
}

interface History {
  entries: HistoryEntry[];
  index: number;
}

type NavigationMode = "push" | "replace" | "travel";

export interface PaneController {
  listing: DirListing | null;
  /** The entries on show, in the order shown; none from collapsed groups. */
  visibleEntries: FileEntry[];
  /** Entries the filter and the hidden files setting let through, collapsed groups included. */
  entryCount: number;
  /** The entries in date groups, in the order shown, while grouping by date; null otherwise. */
  groups: EntryGroup[] | null;
  collapsedGroups: ReadonlySet<DateGroup>;
  /** Collapses or expands one group; collapsing it deselects what it hides. */
  toggleGroup: (group: DateGroup) => void;
  setAllGroupsCollapsed: (collapsed: boolean) => void;
  hiddenCount: number;
  loading: boolean;
  error: AppError | null;
  selection: ReadonlySet<string>;
  cursorPath: string | null;
  sort: SortSpec;
  filter: string;
  showHidden: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  canGoUp: boolean;
  navigate: (path: string) => Promise<boolean>;
  refresh: (focusPath?: string) => Promise<boolean>;
  goUp: () => void;
  goBack: () => void;
  goForward: () => void;
  goHome: () => void;
  open: (entry: FileEntry) => void;
  /** Sorts by `key`, or flips the direction when already sorted by it. */
  setSortKey: (key: SortKey) => void;
  setSort: (sort: SortSpec) => void;
  setFilter: (filter: string) => void;
  toggleHidden: () => void;
  selectOnly: (path: string) => void;
  toggleSelected: (path: string) => void;
  selectRangeTo: (path: string) => void;
  /** Adds the rows from the anchor to `path` and keeps the rest of the selection. */
  addRangeTo: (path: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  moveCursor: (delta: number, extendSelection: boolean) => void;
  moveCursorTo: (index: number, extendSelection: boolean) => void;
  /** Moves the cursor without changing the selection. */
  stepCursor: (delta: number) => void;
  /** Selects exactly `paths`, and moves the anchor and cursor where given. */
  selectPaths: (paths: ReadonlySet<string>, anchor?: string, cursor?: string) => void;
  selectedEntries: () => FileEntry[];
}

export interface PaneOptions {
  showHiddenByDefault?: boolean;
  initialSort?: SortSpec;
  foldersFirst?: boolean;
  /** Groups entries as Windows Explorer does by date modified: Today, Yesterday and so on. */
  groupByDate?: boolean;
  /** Called with each sort order picked in the pane. */
  onSortChange?: (sort: SortSpec) => void;
}

export function usePane(
  source: FileSource,
  {
    showHiddenByDefault = false,
    initialSort = { key: "name", direction: 1 },
    foldersFirst = true,
    groupByDate = false,
    onSortChange,
  }: PaneOptions = {},
): PaneController {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [history, setHistory] = useState<History>({ entries: [], index: -1 });
  const [selection, setSelection] = useState<ReadonlySet<string>>(new Set());
  const [anchorPath, setAnchorPath] = useState<string | null>(null);
  const [cursorPath, setCursorPath] = useState<string | null>(null);
  const [sort, setSortState] = useState<SortSpec>(initialSort);
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(showHiddenByDefault);
  const [collapsedGroups, setCollapsedGroups] = useState<ReadonlySet<DateGroup>>(new Set());
  // Grouping turned off and on again starts with every group open.
  const [groupedBefore, setGroupedBefore] = useState(groupByDate);
  if (groupedBefore !== groupByDate) {
    setGroupedBefore(groupByDate);
    setCollapsedGroups(new Set());
  }
  const today = useToday();
  const loads = useRef(new ListingLoads());
  const listedPath = useRef<string | null>(null);
  const cursorRef = useRef(cursorPath);
  useEffect(() => {
    cursorRef.current = cursorPath;
  }, [cursorPath]);
  // Where Back or Forward is taking the pane, so pressing it again before the folder loads
  // steps on from there.
  const travelIndex = useRef<number | null>(null);

  const sortedEntries = useMemo(
    () =>
      sortEntries(
        filterEntries(listing?.entries ?? [], { showHidden, query: filter }),
        sort,
        foldersFirst,
      ),
    [listing, showHidden, filter, sort, foldersFirst],
  );
  // As in Explorer, the groups run the other way only while sorting oldest first by date.
  const newestFirst = sort.key !== "modified" || sort.direction === -1;
  const groups = useMemo(
    () =>
      groupByDate
        ? groupEntriesByDate(sortedEntries, {
            today: new Date(today),
            weekStartsOn: WEEK_STARTS_ON,
            newestFirst,
          })
        : null,
    [groupByDate, sortedEntries, today, newestFirst],
  );
  const visibleEntries = useMemo(
    () =>
      groups
        ? groups.flatMap((group) => (collapsedGroups.has(group.id) ? [] : group.entries))
        : sortedEntries,
    [groups, collapsedGroups, sortedEntries],
  );
  const hiddenCount = useMemo(
    () => (showHidden ? 0 : (listing?.entries.filter((entry) => entry.hidden).length ?? 0)),
    [listing, showHidden],
  );

  const load = useCallback(
    async (target: string, mode: NavigationMode, focusPath?: string): Promise<boolean> => {
      const request = loads.current.start(mode === "replace" ? "refresh" : "navigation");
      if (request === null) return false;
      if (mode === "push") travelIndex.current = null;
      setLoading(true);
      try {
        const next = await source.list(target);
        if (!loads.current.isCurrent(request)) return false;
        const existing = new Set(next.entries.map((entry) => entry.path));
        const leftPath = listedPath.current;
        const leftCursor = cursorRef.current;
        const sameDirectory = leftPath === next.path;
        listedPath.current = next.path;
        // Back or Forward puts the cursor where it was in the folder, or else on the way back
        // into the folder just left.
        const focus =
          focusPath && existing.has(focusPath)
            ? focusPath
            : mode === "travel" && leftPath
              ? entryLeadingTo(next.entries, leftPath)
              : undefined;
        setListing(next);
        if (!sameDirectory) setCollapsedGroups(new Set());
        const focused = focus ? next.entries.find((entry) => entry.path === focus) : undefined;
        if (focused && sameDirectory) {
          const group = dateGroupFinder(new Date(), WEEK_STARTS_ON)(focused.modified);
          setCollapsedGroups((current) => {
            if (!current.has(group)) return current;
            const expanded = new Set(current);
            expanded.delete(group);
            return expanded;
          });
        }
        setSelection((current) => {
          if (focus) return new Set([focus]);
          return sameDirectory
            ? new Set([...current].filter((path) => existing.has(path)))
            : new Set<string>();
        });
        setCursorPath((current) => {
          if (focus) return focus;
          if (sameDirectory && current && existing.has(current)) return current;
          return null;
        });
        setAnchorPath(focus ?? null);
        setError(null);
        setHistory((current) => {
          const entries = [...current.entries];
          const shown = entries[current.index];
          if (mode === "replace") {
            if (shown) entries[current.index] = { ...shown, path: next.path };
            return { entries, index: current.index };
          }
          if (shown && !sameDirectory) entries[current.index] = { ...shown, cursor: leftCursor };
          const stays = mode === "travel" || shown?.path === next.path;
          if (stays) return { entries, index: current.index };
          entries.splice(current.index + 1, Infinity, { path: next.path, cursor: null });
          return { entries, index: entries.length - 1 };
        });
        return true;
      } catch (caught) {
        if (loads.current.isCurrent(request)) setError(toAppError(caught));
        return false;
      } finally {
        loads.current.finish(request);
        if (loads.current.isCurrent(request)) setLoading(false);
      }
    },
    [source],
  );

  useEffect(() => {
    let cancelled = false;
    source
      .initialPath()
      .then((path) => {
        if (!cancelled) void load(path, "push");
      })
      .catch((caught) => {
        if (!cancelled) setError(toAppError(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [source, load]);

  const navigate = useCallback((path: string) => load(path, "push"), [load]);
  // Reloads the folder last listed, which a navigation sets before the pane renders its listing.
  const refresh = useCallback(
    (focusPath?: string) =>
      listedPath.current !== null
        ? load(listedPath.current, "replace", focusPath)
        : Promise.resolve(false),
    [load],
  );

  const goUp = useCallback(() => {
    if (listing?.parent) void load(listing.parent, "push", listing.path);
  }, [listing, load]);

  const travel = useCallback(
    (offset: number) => {
      const index = (travelIndex.current ?? history.index) + offset;
      const target = history.entries[index];
      if (target === undefined) return;
      travelIndex.current = index;
      void load(target.path, "travel", target.cursor ?? undefined).then((loaded) => {
        if (travelIndex.current === index) travelIndex.current = null;
        if (loaded) setHistory((current) => ({ ...current, index }));
      });
    },
    [history, load],
  );

  const goHome = useCallback(() => {
    source
      .home()
      .then((home) => load(home, "push"))
      .catch((caught) => setError(toAppError(caught)));
  }, [source, load]);

  const open = useCallback(
    (entry: FileEntry) => {
      if (isDirLike(entry)) void navigate(entry.path);
    },
    [navigate],
  );

  const setSort = useCallback(
    (next: SortSpec) => {
      setSortState(next);
      onSortChange?.(next);
    },
    [onSortChange],
  );

  const setSortKey = useCallback(
    (key: SortKey) =>
      setSort(
        sort.key === key
          ? { key, direction: sort.direction === 1 ? -1 : 1 }
          : { key, direction: 1 },
      ),
    [sort, setSort],
  );

  const toggleGroup = useCallback(
    (group: DateGroup) => {
      const collapsing = !collapsedGroups.has(group);
      setCollapsedGroups((current) => {
        const next = new Set(current);
        if (collapsing) next.add(group);
        else next.delete(group);
        return next;
      });
      const hidden = groups?.find((candidate) => candidate.id === group)?.entries ?? [];
      if (!collapsing || hidden.length === 0) return;
      const hiddenPaths = new Set(hidden.map((entry) => entry.path));
      setSelection((current) => new Set([...current].filter((path) => !hiddenPaths.has(path))));
    },
    [collapsedGroups, groups],
  );

  const setAllGroupsCollapsed = useCallback(
    (collapsed: boolean) => {
      setCollapsedGroups(new Set(collapsed ? (groups ?? []).map((group) => group.id) : []));
      if (collapsed) setSelection(new Set());
    },
    [groups],
  );

  const selectOnly = useCallback((path: string) => {
    setSelection(new Set([path]));
    setAnchorPath(path);
    setCursorPath(path);
  }, []);

  const toggleSelected = useCallback((path: string) => {
    setSelection((current) => {
      const next = new Set(current);
      if (!next.delete(path)) next.add(path);
      return next;
    });
    setAnchorPath(path);
    setCursorPath(path);
  }, []);

  const rangeBetween = useCallback(
    (fromPath: string | null, toPath: string): Set<string> => {
      const paths = visibleEntries.map((entry) => entry.path);
      const toIndex = paths.indexOf(toPath);
      const fromIndex = fromPath ? paths.indexOf(fromPath) : -1;
      if (fromIndex < 0) return new Set([toPath]);
      const [start, end] = fromIndex < toIndex ? [fromIndex, toIndex] : [toIndex, fromIndex];
      return new Set(paths.slice(start, end + 1));
    },
    [visibleEntries],
  );

  const selectRangeTo = useCallback(
    (path: string) => {
      setSelection(rangeBetween(anchorPath, path));
      if (!anchorPath) setAnchorPath(path);
      setCursorPath(path);
    },
    [anchorPath, rangeBetween],
  );

  const addRangeTo = useCallback(
    (path: string) => {
      const anchor = anchorPath ?? cursorPath ?? path;
      const range = rangeBetween(anchor, path);
      setSelection((current) => new Set([...current, ...range]));
      setAnchorPath(anchor);
      setCursorPath(path);
    },
    [anchorPath, cursorPath, rangeBetween],
  );

  const selectAll = useCallback(() => {
    setSelection(new Set(visibleEntries.map((entry) => entry.path)));
  }, [visibleEntries]);

  const clearSelection = useCallback(() => {
    setSelection(new Set());
    setAnchorPath(null);
  }, []);

  const moveCursorTo = useCallback(
    (index: number, extendSelection: boolean) => {
      if (visibleEntries.length === 0) return;
      const clamped = Math.max(0, Math.min(visibleEntries.length - 1, index));
      const path = visibleEntries[clamped].path;
      if (extendSelection) {
        const anchor = anchorPath ?? cursorPath ?? path;
        setSelection(rangeBetween(anchor, path));
        setAnchorPath(anchor);
        setCursorPath(path);
      } else {
        selectOnly(path);
      }
    },
    [visibleEntries, anchorPath, cursorPath, rangeBetween, selectOnly],
  );

  const moveCursor = useCallback(
    (delta: number, extendSelection: boolean) => {
      const current = visibleEntries.findIndex((entry) => entry.path === cursorPath);
      const start = current < 0 ? (delta > 0 ? -1 : visibleEntries.length) : current;
      moveCursorTo(start + delta, extendSelection);
    },
    [visibleEntries, cursorPath, moveCursorTo],
  );

  const stepCursor = useCallback(
    (delta: number) => {
      if (visibleEntries.length === 0) return;
      const current = visibleEntries.findIndex((entry) => entry.path === cursorPath);
      const start = current < 0 ? (delta > 0 ? -1 : visibleEntries.length) : current;
      const index = Math.max(0, Math.min(visibleEntries.length - 1, start + delta));
      setCursorPath(visibleEntries[index].path);
    },
    [visibleEntries, cursorPath],
  );

  const selectPaths = useCallback(
    (paths: ReadonlySet<string>, anchor?: string, cursor?: string) => {
      setSelection(paths);
      if (anchor !== undefined) setAnchorPath(anchor);
      if (cursor !== undefined) setCursorPath(cursor);
    },
    [],
  );

  const selectedEntries = useCallback(
    () => visibleEntries.filter((entry) => selection.has(entry.path)),
    [visibleEntries, selection],
  );

  return {
    listing,
    visibleEntries,
    entryCount: sortedEntries.length,
    groups,
    collapsedGroups,
    toggleGroup,
    setAllGroupsCollapsed,
    hiddenCount,
    loading,
    error,
    selection,
    cursorPath,
    sort,
    filter,
    showHidden,
    canGoBack: history.index > 0,
    canGoForward: history.index < history.entries.length - 1,
    canGoUp: Boolean(listing?.parent),
    navigate,
    refresh,
    goUp,
    goBack: () => travel(-1),
    goForward: () => travel(1),
    goHome,
    open,
    setSortKey,
    setSort,
    setFilter,
    toggleHidden: () => setShowHidden((current) => !current),
    selectOnly,
    toggleSelected,
    selectRangeTo,
    addRangeTo,
    selectAll,
    clearSelection,
    moveCursor,
    moveCursorTo,
    stepCursor,
    selectPaths,
    selectedEntries,
  };
}
