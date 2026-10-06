import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { FileSource } from "../lib/fileSource";
import { toAppError } from "../lib/ipc";
import { filterEntries, isDirLike, sortEntries, type SortKey, type SortSpec } from "../lib/sort";
import type { AppError, DirListing, FileEntry } from "../lib/types";

interface History {
  paths: string[];
  index: number;
}

type NavigationMode = "push" | "replace" | "travel";

export interface PaneController {
  listing: DirListing | null;
  visibleEntries: FileEntry[];
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
  selectAll: () => void;
  clearSelection: () => void;
  moveCursor: (delta: number, extendSelection: boolean) => void;
  moveCursorTo: (index: number, extendSelection: boolean) => void;
  selectedEntries: () => FileEntry[];
}

export interface PaneOptions {
  showHiddenByDefault?: boolean;
  initialSort?: SortSpec;
  foldersFirst?: boolean;
  /** Called with each sort order picked in the pane. */
  onSortChange?: (sort: SortSpec) => void;
}

export function usePane(
  source: FileSource,
  {
    showHiddenByDefault = false,
    initialSort = { key: "name", direction: 1 },
    foldersFirst = true,
    onSortChange,
  }: PaneOptions = {},
): PaneController {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [history, setHistory] = useState<History>({ paths: [], index: -1 });
  const [selection, setSelection] = useState<ReadonlySet<string>>(new Set());
  const [anchorPath, setAnchorPath] = useState<string | null>(null);
  const [cursorPath, setCursorPath] = useState<string | null>(null);
  const [sort, setSortState] = useState<SortSpec>(initialSort);
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(showHiddenByDefault);
  const latestRequest = useRef(0);
  const listedPath = useRef<string | null>(null);

  const visibleEntries = useMemo(
    () =>
      sortEntries(
        filterEntries(listing?.entries ?? [], { showHidden, query: filter }),
        sort,
        foldersFirst,
      ),
    [listing, showHidden, filter, sort, foldersFirst],
  );
  const hiddenCount = useMemo(
    () => (showHidden ? 0 : (listing?.entries.filter((entry) => entry.hidden).length ?? 0)),
    [listing, showHidden],
  );

  const load = useCallback(
    async (target: string, mode: NavigationMode, focusPath?: string): Promise<boolean> => {
      const request = ++latestRequest.current;
      setLoading(true);
      try {
        const next = await source.list(target);
        if (request !== latestRequest.current) return false;
        const existing = new Set(next.entries.map((entry) => entry.path));
        const sameDirectory = listedPath.current === next.path;
        listedPath.current = next.path;
        setListing(next);
        setSelection((current) =>
          sameDirectory
            ? new Set([...current].filter((path) => existing.has(path)))
            : new Set(focusPath && existing.has(focusPath) ? [focusPath] : []),
        );
        setCursorPath((current) => {
          if (focusPath && existing.has(focusPath)) return focusPath;
          if (sameDirectory && current && existing.has(current)) return current;
          return null;
        });
        setAnchorPath(focusPath ?? null);
        setError(null);
        setHistory((current) => {
          if (mode === "travel") return current;
          if (mode === "replace" && current.index >= 0) {
            const paths = [...current.paths];
            paths[current.index] = next.path;
            return { paths, index: current.index };
          }
          if (current.paths[current.index] === next.path) return current;
          const paths = [...current.paths.slice(0, current.index + 1), next.path];
          return { paths, index: paths.length - 1 };
        });
        return true;
      } catch (caught) {
        if (request === latestRequest.current) setError(toAppError(caught));
        return false;
      } finally {
        if (request === latestRequest.current) setLoading(false);
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
  const refresh = useCallback(
    (focusPath?: string) =>
      listing ? load(listing.path, "replace", focusPath) : Promise.resolve(false),
    [listing, load],
  );

  const goUp = useCallback(() => {
    if (listing?.parent) void load(listing.parent, "push", listing.path);
  }, [listing, load]);

  const travel = useCallback(
    (offset: number) => {
      const index = history.index + offset;
      const target = history.paths[index];
      if (target === undefined) return;
      void load(target, "travel").then((loaded) => {
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

  const selectedEntries = useCallback(
    () => visibleEntries.filter((entry) => selection.has(entry.path)),
    [visibleEntries, selection],
  );

  return {
    listing,
    visibleEntries,
    hiddenCount,
    loading,
    error,
    selection,
    cursorPath,
    sort,
    filter,
    showHidden,
    canGoBack: history.index > 0,
    canGoForward: history.index < history.paths.length - 1,
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
    selectAll,
    clearSelection,
    moveCursor,
    moveCursorTo,
    selectedEntries,
  };
}
