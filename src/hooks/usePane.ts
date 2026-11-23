import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
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
import { showsHiddenItems } from "../lib/search";
import {
  freshenListing,
  heldEntries,
  folderUnavailable,
  withHeldEntries,
  type StoredListing,
} from "../lib/listingCache";
import { ListingLoads } from "../lib/listingLoads";
import { filterEntries, isDirLike, sortEntries, type SortKey, type SortSpec } from "../lib/sort";
import type { AppError, DirListing, FileEntry } from "../lib/types";
import { useToday } from "./useToday";

const WEEK_STARTS_ON = firstDayOfWeek(navigator.language);

interface HistoryEntry {
  path: string;
  /** Where the cursor was on leaving the folder, to put it back on returning. */
  cursor: string | null;
  /** The pane showed the results it was given there, such as a search's, not the folder. */
  results?: boolean;
}

interface History {
  entries: HistoryEntry[];
  index: number;
}

type NavigationMode = "push" | "replace" | "travel";

/** What a reload selects: one path, or several, as an extraction places. */
export type Focus = string | readonly string[];

function focusList(focus: Focus | undefined): readonly string[] {
  if (focus === undefined) return [];
  return typeof focus === "string" ? [focus] : focus;
}

function sameSet(first: ReadonlySet<string>, second: ReadonlySet<string>): boolean {
  return first.size === second.size && [...first].every((path) => second.has(path));
}

/** Names whose files a source lists: sessions on one connection list the same ones. */
function filesOf(source: FileSource): string {
  return source.listingScope ?? source.key;
}

const NO_PATHS: ReadonlySet<string> = new Set();
const NO_ENTRIES: readonly FileEntry[] = [];

/**
 * Another source that shows a folder in place of the pane's own, as an unlocked vault does for
 * the paths inside it, or its server for those outside.
 */
export interface Handover {
  source: FileSource;
  /** Makes `source` the pane's own, once it has listed the folder at `path`. */
  take: (path: string) => void;
}

/**
 * The listings remembered for the server a pane is on, so that a folder seen before shows at
 * once. `state/listings.ts` gives the panes of a window theirs.
 */
export interface PaneListings {
  /** The folder's last known listing, to show while the server is asked for it again. */
  recall: (source: FileSource, path: string) => DirListing | null;
  /**
   * Notes that `by` asks the server for `path`. The function returned takes the answer, to
   * remember it, or null when the folder could not be listed.
   */
  asking: (source: FileSource, path: string, by: unknown) => (answer: DirListing | null) => void;
  /** Forgets a folder that can no longer be listed, with the folders inside it. */
  forget: (source: FileSource, path: string) => void;
  /** Calls `listener` with every listing remembered from now on, whoever asked for it. */
  watch: (listener: (stored: StoredListing) => void) => () => void;
}

/**
 * What a remembered listing on show is waiting for: "refreshing" while the server is asked for
 * the folder again, "outdated" when the server could not be reached to ask. Null for a listing
 * the server just gave.
 */
export type Staleness = "refreshing" | "outdated" | null;

/** A folder on its way into the pane. */
interface Load {
  /** Whether the folder is on show already, from a remembered listing. */
  shown: boolean;
  /** Whether the server listed the folder and the pane shows it, once that is known. */
  answered: Promise<boolean>;
}

const NOT_STARTED: Load = { shown: false, answered: Promise.resolve(false) };

export interface PaneController {
  /** What the pane shows: the folder's listing, or the results while they are on show. */
  listing: DirListing | null;
  /** The folder's listing, which stays behind the results while they are on show. */
  folder: DirListing | null;
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
  /** Set while the listing on show is a remembered one the server has not confirmed. */
  stale: Staleness;
  /** The folder that was on show until it could no longer be listed, which emptied the pane. */
  failedPath: string | null;
  selection: ReadonlySet<string>;
  cursorPath: string | null;
  sort: SortSpec;
  filter: string;
  showHidden: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  canGoUp: boolean;
  /** The pane shows the results it was given in place of the folder's listing. */
  showingResults: boolean;
  /** Shows the results given to the pane, found in or below `path`, as a step in its history. */
  showResults: (path: string) => void;
  /** Leaves the results for the folder they were found in, and forgets their step in history. */
  closeResults: () => void;
  /**
   * Opens a folder, selecting `focus` there; true once it is on show, which a remembered
   * listing makes it at once.
   */
  navigate: (path: string, focus?: Focus) => Promise<boolean>;
  /**
   * Reloads the folder from the server, never from a remembered listing, selecting `focus` (one
   * path or several) where the listing has it.
   */
  refresh: (focus?: Focus) => Promise<boolean>;
  /** Empties the pane until its next listing arrives, for a listing that must not stay up. */
  clear: () => void;
  /** Whether a listing is on its way. */
  isLoading: () => boolean;
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
  /**
   * Asked before every folder is listed, however the pane gets there: the source to list `path`
   * with when that is not the pane's own, or null. The pane keeps its place and its history.
   */
  handover?: (path: string) => Handover | null;
  /** Entries left out by name, as the identical ones of two compared panes can be. */
  hideNames?: ReadonlySet<string>;
  /**
   * Remembered listings to show folders from at once; without them every folder is awaited, and
   * a folder that fails to reload stays on show as it was.
   */
  listings?: PaneListings;
  /**
   * Rows the user is in the middle of something with, such as a rename. They stay on show when
   * a newer listing of the folder no longer has them, until that is over.
   */
  heldPaths?: ReadonlySet<string>;
  /**
   * Entries from elsewhere, such as search results, shown with sorting, filtering and selection
   * as a listing is while the pane is at the step `showResults` made. Their `path` is the folder
   * they were found in, and they have no parent. They are never remembered as a folder's listing.
   */
  results?: DirListing | null;
}

export function usePane(
  source: FileSource,
  {
    showHiddenByDefault = false,
    initialSort = { key: "name", direction: 1 },
    foldersFirst = true,
    groupByDate = false,
    onSortChange,
    handover,
    hideNames,
    listings,
    heldPaths = NO_PATHS,
    results = null,
  }: PaneOptions = {},
): PaneController {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [staleness, setStaleness] = useState<Staleness>(null);
  /** The folder that could no longer be listed, and the one it is in, to go up to. */
  const [failed, setFailed] = useState<{ path: string; parent: string | null } | null>(null);
  /** Rows held for the user that the listing on show no longer has. */
  const [carried, setCarried] = useState<readonly FileEntry[]>(NO_ENTRIES);
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
  /** The listing on show as the server or the memory gave it, and the source it is from. */
  const shown = useRef<DirListing | null>(null);
  const shownBy = useRef<FileSource | null>(null);
  /** Counts what the loads put on show, so a load can tell whether its listing still is. */
  const view = useRef(0);
  /** Tells this pane's own listings from the ones other panes had remembered. */
  const self = useRef<object>({});
  const cursorRef = useRef(cursorPath);
  const selectionRef = useRef(selection);
  useEffect(() => {
    cursorRef.current = cursorPath;
    selectionRef.current = selection;
  }, [cursorPath, selection]);
  // Where Back or Forward is taking the pane, so pressing it again before the folder loads
  // steps on from there.
  const travelIndex = useRef<number | null>(null);
  const handoverRef = useRef(handover);
  const heldRef = useRef(heldPaths);
  // Read by each load as it starts, so that turning remembered listings on or off does not
  // open the pane's first folder again.
  const listingsRef = useRef(listings);
  useLayoutEffect(() => {
    handoverRef.current = handover;
    heldRef.current = heldPaths;
    listingsRef.current = listings;
  });
  // The source a handover listed the folder on show with, until it is the pane's own.
  const takenOverBy = useRef<string | null>(null);

  const showingResults = history.entries[history.index]?.results === true && results !== null;
  const hasResults = useRef(results !== null);
  // Whether results are on show, for what changes the folder's listing behind them.
  const resultsOnShow = useRef(showingResults);
  useLayoutEffect(() => {
    hasResults.current = results !== null;
    resultsOnShow.current = showingResults;
  });
  const folderDisplayed = useMemo(
    () => withHeldEntries(listing, carried, heldPaths),
    [listing, carried, heldPaths],
  );
  const displayed = showingResults ? results : folderDisplayed;
  const showsHidden = showsHiddenItems(showHidden, showingResults);
  // Names are left out of a folder's listing only: results come from many folders.
  const hiddenNames = showingResults ? undefined : hideNames;
  const sortedEntries = useMemo(() => {
    const shown = filterEntries(displayed?.entries ?? [], {
      showHidden: showsHidden,
      query: filter,
    });
    return sortEntries(
      hiddenNames ? shown.filter((entry) => !hiddenNames.has(entry.name)) : shown,
      sort,
      foldersFirst,
    );
  }, [displayed, showsHidden, filter, sort, foldersFirst, hiddenNames]);
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
    () => (showsHidden ? 0 : (displayed?.entries.filter((entry) => entry.hidden).length ?? 0)),
    [displayed, showsHidden],
  );

  /**
   * Puts a listing on show. One of the folder already on show changes nothing that did not
   * change on the server: the selection, the cursor and the rows that are the same stay as they
   * are. For a folder newly on show, returns the cursor and the selection it starts with.
   */
  const show = useCallback(
    (
      next: DirListing,
      lister: FileSource,
      mode: NavigationMode,
      focusPaths?: Focus,
      travelTo?: number,
    ) => {
      const leftPath = listedPath.current;
      const leftCursor = cursorRef.current;
      const sameDirectory = leftPath === next.path;
      // A vault and its server show one path with different files, so the source counts too.
      const sameFiles =
        sameDirectory && shownBy.current !== null && filesOf(shownBy.current) === filesOf(lister);
      const before = sameFiles ? shown.current : null;
      const fresh = before ? freshenListing(before, next) : next;
      const existing = new Set(fresh.entries.map((entry) => entry.path));
      listedPath.current = fresh.path;
      shown.current = fresh;
      shownBy.current = lister;
      // A reload selects what it was asked to, with the cursor on the first; Back or Forward
      // puts the cursor where it was in the folder, or else on the way back into the folder
      // just left.
      const wanted = focusList(focusPaths).filter((path) => existing.has(path));
      const focus =
        wanted[0] ??
        (mode === "travel" && leftPath ? entryLeadingTo(fresh.entries, leftPath) : undefined);
      const selected = new Set(wanted.length > 0 ? wanted : focus ? [focus] : []);
      const held = heldRef.current;
      setListing(fresh);
      setCarried((current) => {
        const kept = before ? heldEntries([...before.entries, ...current], fresh, held) : [];
        return kept.length === 0 && current.length === 0 ? current : kept;
      });
      // A newer listing of the folder behind search results changes only that listing: the
      // results, their selection and their step in history stay.
      if (mode === "replace" && resultsOnShow.current) return null;
      if (!sameDirectory) setCollapsedGroups(new Set());
      const focused = fresh.entries.filter((entry) => selected.has(entry.path));
      if (focused.length > 0 && sameDirectory) {
        // Items shown in a collapsed date group open it, so what was selected can be seen.
        const groupOf = dateGroupFinder(new Date(), WEEK_STARTS_ON);
        const groups = new Set(focused.map((entry) => groupOf(entry.modified)));
        setCollapsedGroups((current) => {
          if (![...groups].some((group) => current.has(group))) return current;
          const expanded = new Set(current);
          for (const group of groups) expanded.delete(group);
          return expanded;
        });
      }
      const staying = (current: string | null) =>
        sameDirectory && current !== null && existing.has(current) ? current : null;
      setSelection((current) => {
        if (selected.size > 0) return selected;
        const kept = sameDirectory ? [...current].filter((path) => existing.has(path)) : [];
        return kept.length === current.size ? current : new Set(kept);
      });
      setCursorPath((current) => focus ?? staying(current));
      setAnchorPath((current) => focus ?? staying(current));
      setHistory((current) => {
        const entries = [...current.entries];
        const entry = entries[current.index];
        if (mode === "replace") {
          if (entry?.path === fresh.path) return current;
          if (entry) entries[current.index] = { ...entry, path: fresh.path };
          return { entries, index: current.index };
        }
        if (entry && !sameDirectory) entries[current.index] = { ...entry, cursor: leftCursor };
        if (mode === "travel") return { entries, index: travelTo ?? current.index };
        // Leaving results for their own folder is a step of its own.
        if (entry?.path === fresh.path && !entry.results) {
          return { entries, index: current.index };
        }
        entries.splice(current.index + 1, Infinity, { path: fresh.path, cursor: null });
        return { entries, index: entries.length - 1 };
      });
      return sameDirectory ? null : { cursor: focus ?? null, selection: selected };
    },
    [],
  );

  /** Takes off show a listing whose folder can no longer be listed. */
  const drop = useCallback((first: boolean) => {
    const parent = shown.current?.parent ?? null;
    view.current += 1;
    shown.current = null;
    setListing(null);
    setCarried(NO_ENTRIES);
    setSelection(new Set());
    setAnchorPath(null);
    setCursorPath(null);
    setStaleness(null);
    if (first) {
      // As if the folder had never opened, so the pane opens where it does when one cannot.
      listedPath.current = null;
      setHistory({ entries: [], index: -1 });
    } else if (listedPath.current !== null) {
      setFailed({ path: listedPath.current, parent });
    }
  }, []);

  const load = useCallback(
    (target: string, mode: NavigationMode, focusPaths?: Focus, travelTo?: number): Load => {
      const request = loads.current.start(mode === "replace" ? "refresh" : "navigation");
      if (request === null) return NOT_STARTED;
      if (mode === "push") travelIndex.current = null;
      takenOverBy.current = null;
      setLoading(true);
      const first = listedPath.current === null;
      const started = { shown: false };
      const listings = listingsRef.current;
      const answer = async (): Promise<boolean> => {
        let lister = source;
        let remembered: DirListing | null = null;
        let rememberedView = -1;
        let noteAnswer: ((answer: DirListing | null) => void) | undefined;
        try {
          const other = handoverRef.current?.(target) ?? null;
          lister = other?.source ?? source;
          const take = (path: string) => {
            if (!other) return;
            takenOverBy.current = other.source.key;
            other.take(path);
          };
          // A reload asks the server, and shows nothing remembered in the meantime.
          remembered = mode === "replace" ? null : (listings?.recall(lister, target) ?? null);
          let startedWith: ReturnType<typeof show> = null;
          if (remembered) {
            startedWith = show(remembered, lister, mode, focusPaths, travelTo);
            view.current += 1;
            rememberedView = view.current;
            setError(null);
            setFailed(null);
            setStaleness("refreshing");
            // The folder is on show, so a reload asked for from here on is of this folder.
            loads.current.landed(request);
            take(remembered.path);
            started.shown = true;
          }
          noteAnswer = listings?.asking(lister, target, self.current);
          const next = await lister.list(target);
          noteAnswer?.(next);
          noteAnswer = undefined;
          if (remembered) {
            // Still the remembered listing on show: the pane went nowhere else meanwhile.
            if (view.current !== rememberedView) return false;
            const known = new Set(remembered.entries.map((entry) => entry.path));
            const untouched =
              startedWith !== null &&
              cursorRef.current === startedWith.cursor &&
              sameSet(selectionRef.current, startedWith.selection);
            // What was to be selected and is listed only now is, unless the user chose since.
            const selectsNow = untouched && focusList(focusPaths).some((path) => !known.has(path));
            show(next, lister, "replace", selectsNow ? focusPaths : undefined);
            if (next.path !== remembered.path) take(next.path);
          } else {
            if (!loads.current.isCurrent(request)) return false;
            show(next, lister, mode, focusPaths, travelTo);
            take(next.path);
          }
          view.current += 1;
          setError(null);
          setFailed(null);
          setStaleness(null);
          return true;
        } catch (caught) {
          noteAnswer?.(null);
          const failure = toAppError(caught);
          const unavailable = folderUnavailable(failure);
          // The server says the folder cannot be listed, so nothing remembered of it holds,
          // wherever the pane has gone since.
          if (unavailable) listings?.forget(lister, target);
          const current = loads.current.isCurrent(request);
          // Whether the listing on show is one this load was to confirm: the remembered one it
          // put up, or the one it reloads.
          const unconfirmed = remembered
            ? view.current === rememberedView
            : current && mode === "replace" && listings !== undefined && shown.current !== null;
          if (current || unconfirmed) setError(failure);
          if (!unconfirmed) return false;
          if (!unavailable) {
            // The server could not be asked, or could not answer now: nothing says the folder
            // changed, so it stays up, marked as not confirmed.
            setStaleness("outdated");
            return false;
          }
          // The folder on show, which a link asked for as `target` may have listed under its own
          // path.
          if (listedPath.current !== null && listedPath.current !== target) {
            listings?.forget(lister, listedPath.current);
          }
          drop(first && remembered !== null);
          return false;
        } finally {
          loads.current.finish(request);
          if (loads.current.isCurrent(request)) setLoading(false);
        }
      };
      const answered = answer();
      return { shown: started.shown, answered };
    },
    [source, show, drop],
  );

  useEffect(() => {
    // The folder on show came from this source already, in a handover.
    if (takenOverBy.current === source.key) {
      takenOverBy.current = null;
      return;
    }
    let cancelled = false;
    const open = async () => {
      const opened = await load(await source.initialPath(), "push").answered;
      // Nothing listed yet, so the user has not gone anywhere else meanwhile.
      if (opened || cancelled || !source.fallbackPath || listedPath.current !== null) return;
      const fallback = await source.fallbackPath();
      if (!cancelled) await load(fallback, "push").answered;
    };
    open().catch((caught) => {
      if (!cancelled) setError(toAppError(caught));
    });
    return () => {
      cancelled = true;
    };
  }, [source, load]);

  // A listing another pane or window had remembered for the folder on show is newer than the
  // one here, so it is shown here too.
  useEffect(() => {
    if (!listings) return;
    return listings.watch((stored) => {
      const lister = shownBy.current;
      if (stored.by === self.current || !lister || !shown.current) return;
      if (lister.listingScope !== stored.scope || shown.current.path !== stored.listing.path) {
        return;
      }
      show(stored.listing, lister, "replace");
    });
  }, [listings, show]);

  const navigate = useCallback(
    (path: string, focus?: Focus) => {
      const { shown: remembered, answered } = load(path, "push", focus);
      return remembered ? Promise.resolve(true) : answered;
    },
    [load],
  );
  // Reloads the folder last listed, which a navigation sets before the pane renders its listing.
  const refresh = useCallback(
    (focus?: Focus) =>
      listedPath.current !== null
        ? load(listedPath.current, "replace", focus).answered
        : Promise.resolve(false),
    [load],
  );

  const clear = useCallback(() => {
    // A load on its way would bring back what is cleared.
    const superseding = loads.current.start("navigation");
    if (superseding !== null) loads.current.finish(superseding);
    view.current += 1;
    shown.current = null;
    setLoading(false);
    setListing(null);
    setCarried(NO_ENTRIES);
    setSelection(new Set());
    setAnchorPath(null);
    setCursorPath(null);
    setError(null);
    setStaleness(null);
    setFailed(null);
  }, []);

  const isLoading = useCallback(() => loads.current.busy, []);

  // A folder that could no longer be listed is left the way it would have been on show; results
  // have no folder above them.
  const upward = showingResults ? results : (displayed ?? failed);
  const goUp = useCallback(() => {
    if (upward?.parent) void load(upward.parent, "push", upward.path);
  }, [upward, load]);

  /** Stops a listing on its way, which would replace what the pane is about to show. */
  const supersedeLoads = useCallback(() => {
    const superseding = loads.current.start("navigation");
    if (superseding !== null) loads.current.finish(superseding);
    // A remembered listing on show is no longer the one a load is to confirm.
    view.current += 1;
    setLoading(false);
    setStaleness(null);
  }, []);

  const showResults = useCallback(
    (path: string) => {
      supersedeLoads();
      travelIndex.current = null;
      setHistory((current) => {
        const at = current.entries[current.index];
        if (at?.results) {
          const entries = [...current.entries];
          entries[current.index] = { ...at, path, cursor: null };
          return { entries, index: current.index };
        }
        // One step shows results: the pane holds one set of them.
        const before = current.entries
          .slice(0, current.index + 1)
          .map((entry, index) =>
            index === current.index ? { ...entry, cursor: cursorRef.current } : entry,
          )
          .filter((entry) => !entry.results);
        return {
          entries: [...before, { path, cursor: null, results: true }],
          index: before.length,
        };
      });
      setSelection(new Set());
      setAnchorPath(null);
      setCursorPath(null);
      setCollapsedGroups(new Set());
    },
    [supersedeLoads],
  );

  const closeResults = useCallback(() => {
    const at = history.entries[history.index];
    if (!at?.results) return;
    const entries = history.entries.filter((entry) => !entry.results);
    const previous = history.entries[history.index - 1];
    // Back to the folder the results were found in, without a step for it twice.
    const returning = previous?.path === at.path;
    const index = returning ? history.index - 1 : history.index;
    if (!returning) entries.splice(index, 0, { path: at.path, cursor: null });
    travelIndex.current = null;
    resultsOnShow.current = false;
    setHistory({ entries, index });
    void load(at.path, "travel", returning ? (previous.cursor ?? undefined) : undefined).answered;
  }, [history, load]);

  const travel = useCallback(
    (offset: number) => {
      const index = (travelIndex.current ?? history.index) + offset;
      const target = history.entries[index];
      if (target === undefined) return;
      if (target.results && hasResults.current) {
        supersedeLoads();
        travelIndex.current = null;
        setHistory((current) => ({ ...current, index }));
        setSelection(new Set());
        setAnchorPath(null);
        setCursorPath(target.cursor);
        return;
      }
      travelIndex.current = index;
      resultsOnShow.current = false;
      void load(target.path, "travel", target.cursor ?? undefined, index).answered.then(() => {
        if (travelIndex.current === index) travelIndex.current = null;
      });
    },
    [history, load, supersedeLoads],
  );

  const goHome = useCallback(() => {
    source
      .home()
      .then((home) => load(home, "push").answered)
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
    listing: displayed,
    folder: folderDisplayed,
    visibleEntries,
    entryCount: sortedEntries.length,
    groups,
    collapsedGroups,
    toggleGroup,
    setAllGroupsCollapsed,
    hiddenCount,
    loading,
    error,
    // What the folder behind the results waits for is not the results'.
    stale: showingResults ? null : staleness,
    failedPath: showingResults ? null : (failed?.path ?? null),
    selection,
    cursorPath,
    sort,
    filter,
    showHidden,
    canGoBack: history.index > 0,
    canGoForward: history.index < history.entries.length - 1,
    canGoUp: Boolean(upward?.parent),
    showingResults,
    showResults,
    closeResults,
    navigate,
    refresh,
    clear,
    isLoading,
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
