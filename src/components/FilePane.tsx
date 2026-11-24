import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ArrowUpDown,
  Bookmark,
  ClipboardCopy,
  ClipboardPaste,
  Copy,
  CopyPlus,
  Download,
  Eye,
  EyeOff,
  FileDiff,
  FilePen,
  FilePlus,
  FolderInput,
  FolderOpen,
  FolderOutput,
  FolderPlus,
  FolderSearch,
  FolderSync,
  GitCompareArrows,
  HardDrive,
  Home,
  Info,
  ListTree,
  LocateFixed,
  Lock,
  LockKeyhole,
  PackageOpen,
  PanelRight,
  Pencil,
  RefreshCw,
  RotateCcw,
  Scissors,
  Search,
  ShieldCheck,
  ShieldPlus,
  SquareTerminal,
  Trash2,
  TriangleAlert,
  Upload,
  X,
} from "lucide-react";
import { useBackForwardButtons } from "../hooks/useBackForwardButtons";
import { useFolderCompare, useHiddenIdentical } from "../hooks/useFolderCompare";
import { useListingPrefetch } from "../hooks/useListingPrefetch";
import { usePane, type Focus } from "../hooks/usePane";
import { useSearch, type SearchView } from "../hooks/useSearch";
import { archiveFormatOf, isSingleFile, unpackedName } from "../lib/archives";
import { COLUMN_LABELS, availableColumns, type DetailColumn } from "../lib/columns";
import { entryAfterRemoval } from "../lib/cursor";
import { sameFilesystem, toLocation, worksInPlace, type FileOrigin } from "../lib/fileOrigin";
import type { FileSource } from "../lib/fileSource";
import { PANE_KEYBOARD_TARGET } from "../lib/focus";
import { FOCUS_PREVIEW_SHORTCUT, isFocusPreviewKey, nextPane } from "../lib/paneFocus";
import { toAppError } from "../lib/ipc";
import { formatSize, pluralize } from "../lib/format";
import { baseName, exampleFileName, samePath } from "../lib/path";
import { protocolInfo } from "../lib/protocols";
import {
  dateNarrows,
  defaultSearchForm,
  folderOf,
  recheckAfterChange,
  recheckTargets,
  relativeFolder,
  sharedFolder,
  sizeNarrows,
  type SearchForm,
} from "../lib/search";
import { groupOfTab } from "../lib/layout";
import { isDirLike, type SortKey, type SortSpec } from "../lib/sort";
import type { DirListing, FileEntry, FileLocation, PlaceMode } from "../lib/types";
import { sessionShowing, vaultSessions, vaultSign } from "../lib/vaults";
import { archivesAmong, extractArchives } from "../state/archiveActions";
import { useClipboardStore } from "../state/clipboardStore";
import { beginDrag, useDragStore, type DropHandler } from "../state/dragStore";
import { dragFilesOut } from "../state/dragOut";
import { openInEditor } from "../state/editorActions";
import { openWithDefault, useExternalEditStore, type OpenTarget } from "../state/externalEditStore";
import {
  deleteOnServer,
  dropAction,
  paneOrigin,
  paste,
  placeEntries,
  putOnClipboard,
  refreshFoldersOf,
} from "../state/fileOperations";
import { useLayoutStore } from "../state/layoutStore";
import { useLogStore } from "../state/logStore";
import {
  getPane,
  lastActivePane,
  listPanes,
  registerPane,
  unregisterPane,
  updatePane,
  type PaneHandle,
} from "../state/paneRegistry";
import { paneListings } from "../state/listings";
import { useSearchStore } from "../state/searchStore";
import { useSessionStore } from "../state/sessionStore";
import {
  previewSettingsSection,
  saveSettingsSection,
  useSettingsStore,
} from "../state/settingsStore";
import { togglePreview, usePreviewOpen } from "../state/previewState";
import { openTerminal } from "../state/terminalActions";
import { useToastStore } from "../state/toastStore";
import {
  sayWhereResultsDropWent,
  transferFromPane,
  transferToOtherSide,
} from "../state/transferActions";
import { useUiStore } from "../state/uiStore";
import { requestLockVault, vaultHandover } from "../state/vaultActions";
import { workspaceChanged } from "../state/workspace";
import { BulkRenameDialog } from "./BulkRenameDialog";
import { CompareDialog, type CompareSide } from "./CompareDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { DeleteDialog } from "./DeleteDialog";
import { FileList, type MenuPosition } from "./FileList";
import { FolderCompareBar } from "./FolderCompareBar";
import { COMPARE_FOLDERS_SHORTCUT, folderCompareMenuItems } from "./folderCompareMenu";
import { NewVaultDialog } from "./NewVaultDialog";
import { openMenuItems } from "./openWithMenu";
import { PanePreview } from "./PanePreview";
import { PathBar } from "./PathBar";
import { PreviewSplit } from "./PreviewSplit";
import { PromptDialog } from "./PromptDialog";
import { PropertiesDialog } from "./PropertiesDialog";
import { SearchBar } from "./SearchBar";
import { ShareDialog } from "./ShareDialog";
import { VaultUnlockCard } from "./VaultUnlockCard";
import { serverCommandsMenu } from "./commandMenu";
import { shareMenuItem } from "./shareMenu";

type PaneDialog =
  | { type: "newFolder" }
  | { type: "newFile"; example: string }
  | { type: "newVault" }
  | { type: "delete"; entries: FileEntry[] }
  | { type: "placeTo"; mode: PlaceMode; entries: FileEntry[] }
  | { type: "properties"; entries: FileEntry[] }
  | { type: "share"; entries: FileEntry[] }
  | { type: "bulkRename"; origin: FileOrigin; folder: string; entries: FileEntry[] }
  | { type: "compare"; left: CompareSide; right: CompareSide };

interface FilePaneProps {
  tabId: string;
  title: string;
  icon: ReactNode;
  source: FileSource;
  /** For server panes. */
  sessionId?: string;
  /** The pane is its group's selected tab. */
  visible: boolean;
  /** The pane is visible in the group the user works in. */
  active: boolean;
  /** Covers the file list, for example while the connection is lost. */
  overlay?: ReactNode;
}

interface OpenMenu {
  x: number;
  y: number;
  items: MenuItem[];
  /** The row the menu was opened on. */
  path?: string;
}

/**
 * Moves keyboard focus to the next file list on screen, left to right, then top to bottom.
 * Previews are not on the way: from one, focus goes on to the next pane's list.
 */
function focusNextPane(from: HTMLElement | null) {
  const lists = [
    ...document.querySelectorAll<HTMLElement>(`.tab-panel:not([hidden]) ${PANE_KEYBOARD_TARGET}`),
  ];
  const leavingPreview = Boolean(from?.closest(".preview"));
  const own = lists.find(
    (list) =>
      list === from ||
      list.contains(from) ||
      (leavingPreview && Boolean(list.closest(".pane")?.contains(from))),
  );
  const stops = lists.map((list) => {
    const bounds = list.getBoundingClientRect();
    return { pane: list, left: bounds.left, top: bounds.top };
  });
  nextPane(stops, own ?? null, leavingPreview)?.focus();
}

/** A regular file, or a link to one: what compare can read. */
function isFileLike(entry: FileEntry): boolean {
  return entry.kind === "file" || (entry.kind === "symlink" && entry.linkTarget === "file");
}

function compareSide(pane: PaneHandle, entry: FileEntry): CompareSide {
  return {
    location: toLocation(paneOrigin(pane)),
    path: entry.path,
    name: entry.name,
    where: pane.label,
  };
}

/** A file selected on its own in another pane, the one looked at last, to compare with. */
function counterpartFile(tabId: string): { pane: PaneHandle; entry: FileEntry } | null {
  let best: { pane: PaneHandle; entry: FileEntry } | null = null;
  for (const pane of listPanes()) {
    if (pane.tabId === tabId || !worksInPlace(paneOrigin(pane))) continue;
    const selected = pane.selected();
    if (selected.length !== 1 || !isFileLike(selected[0])) continue;
    const better =
      !best ||
      (pane.visible && !best.pane.visible) ||
      (pane.visible === best.pane.visible && pane.activatedAt > best.pane.activatedAt);
    if (better) best = { pane, entry: selected[0] };
  }
  return best;
}

/** Keeps rows from getting shorter than their text when the font is large. */
const MIN_ROW_PADDING = 6;

// Sorting by folder belongs to search results, so folders keep the order they had.
const rememberSort = (sort: SortSpec) =>
  sort.key !== "folder" && void saveSettingsSection("interface", { sort });

const NO_SEARCHES: SearchForm[] = [];

/** What the list says while a search shows no results. */
function searchEmptyMessage(view: SearchView): string {
  switch (view.status) {
    case "searching":
      return "Searching...";
    case "stopped":
      return "Stopped before anything was found";
    case "failed":
      return "Nothing was found";
    default: {
      const { form } = view;
      const narrowed =
        form.kind !== "any" ||
        sizeNarrows(form.size) ||
        dateNarrows(form.date) ||
        form.depth !== null;
      const found = narrowed ? "No results with these filters." : "No results.";
      return form.hidden ? found : `${found} Hidden folders were not searched.`;
    }
  }
}

export function FilePane({
  tabId,
  title,
  icon,
  source,
  sessionId,
  visible,
  active,
  overlay,
}: FilePaneProps) {
  const showHiddenByDefault = useSettingsStore((state) => state.settings.interface.showHiddenFiles);
  const doubleClickFile = useSettingsStore((state) => state.settings.interface.doubleClickFile);
  const foldersFirst = useSettingsStore((state) => state.settings.interface.foldersFirst);
  const hiddenColumns = useSettingsStore((state) => state.settings.interface.hiddenColumns);
  const columnWidths = useSettingsStore((state) => state.settings.interface.columnWidths);
  const cachedListings = useSettingsStore((state) => state.settings.interface.cachedListings);
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const rowHeight = useSettingsStore((state) => state.settings.appearance.rowHeight);
  const stripedRows = useSettingsStore((state) => state.settings.appearance.stripedRows);
  const fontSize = useSettingsStore((state) => state.settings.appearance.fontSize);
  const groupSetting = source.kind === "local" ? "groupLocalByDate" : "groupRemoteByDate";
  const groupByDate = useSettingsStore((state) => state.settings.appearance[groupSetting]);
  // A server and its unlocked vaults share their paths: wherever the pane goes, the path tells
  // which of their sessions lists it, and the tab takes that session.
  const handover = useCallback(
    (path: string) => (sessionId ? vaultHandover(tabId, sessionId, path) : null),
    [tabId, sessionId],
  );
  const hiddenIdentical = useHiddenIdentical(tabId);
  /** The entry whose row shows the name field. */
  const [renaming, setRenaming] = useState<string | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  // The row being renamed and the one a menu is open on stay as they are when the folder is
  // listed again under them.
  const menuPath = menu?.path;
  const heldPaths = useMemo(
    () => new Set([renaming, menuPath].filter((path): path is string => typeof path === "string")),
    [renaming, menuPath],
  );
  const searchLocation = useMemo<FileLocation>(
    () =>
      source.kind === "remote" && sessionId ? { kind: "remote", sessionId } : { kind: "local" },
    [source.kind, sessionId],
  );
  const search = useSearch(searchLocation);
  const searchView = search.view;
  const resultsRoot = searchView?.root;
  const resultEntries = searchView?.entries;
  // A new listing only when the results change, not with each step of the search's progress.
  const searchResults = useMemo<DirListing | null>(
    () =>
      resultsRoot !== undefined && resultEntries
        ? { path: resultsRoot, parent: null, entries: resultEntries }
        : null,
    [resultsRoot, resultEntries],
  );
  const pane = usePane(source, {
    showHiddenByDefault,
    initialSort: useSettingsStore.getState().settings.interface.sort,
    foldersFirst,
    groupByDate,
    onSortChange: rememberSort,
    handover,
    hideNames: hiddenIdentical,
    results: searchResults,
    // This computer's folders list at once, so only a server's are remembered.
    listings: sessionId && cachedListings ? paneListings : undefined,
    heldPaths,
  });
  useListingPrefetch(pane, sessionId, visible);
  /** The pane shows search results in place of the folder. */
  const inResults = pane.showingResults && searchView !== null;
  /** The search bar shows: opened over the folder, or above results. `root` is the folder a
   * search started from there would start in, when not the one on show. */
  const [searchBar, setSearchBar] = useState<{ root: string | null } | null>(null);
  const [searchForm, setSearchForm] = useState<SearchForm>(() =>
    defaultSearchForm(showHiddenByDefault),
  );
  const [searchProblem, setSearchProblem] = useState<string | null>(null);
  const [searchFocus, setSearchFocus] = useState(0);
  const recentSearches = useSearchStore((state) => state.recent[source.key]) ?? NO_SEARCHES;
  const [dialog, setDialog] = useState<PaneDialog | null>(null);
  // A field whose row is no longer on show, as after going to another folder with the mouse's
  // back button, ends there, so it does not come back when the folder is shown again.
  if (renaming !== null && !pane.listing?.entries.some((entry) => entry.path === renaming)) {
    setRenaming(null);
  }
  const [pathEditRequest, setPathEditRequest] = useState(0);
  const previewOpen = usePreviewOpen(tabId);
  const sectionRef = useRef<HTMLElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);
  const writeLog = useLogStore((state) => state.write);
  const showToast = useToastStore((state) => state.show);
  const openDialog = useUiStore((state) => state.open);
  const unsaved = useSessionStore((state) => {
    const entry = sessionId ? state.sessions[sessionId] : undefined;
    return Boolean(entry?.profile && !entry.info.savedConnectionId);
  });
  const protocol = useSessionStore((state) =>
    sessionId ? state.sessions[sessionId]?.info.protocol : undefined,
  );
  /** The share links the session's service makes, if any. */
  const sharing = useSessionStore((state) =>
    sessionId ? state.sessions[sessionId]?.info.sharing : undefined,
  );
  /** The unlocked vault the pane is in, when its session is a vault's. */
  const vault = useSessionStore((state) =>
    sessionId ? state.sessions[sessionId]?.info.vault : undefined,
  );
  const where = source.kind === "remote" ? "remote" : "local";
  // Moving, copying and comparing in place, and Properties, run over SFTP, which reaches a
  // vault's encrypted files only.
  const overSftp = protocol === "sftp";
  const worksHere = source.kind === "local" || overSftp;
  const needsSftp = vault ? "not in a vault" : "SFTP only";
  const paneRef = useRef(pane);
  useLayoutEffect(() => {
    paneRef.current = pane;
  });
  // Results are looked at again where the folder would be listed again.
  const refreshRef = useRef<(focus?: Focus) => void>(() => undefined);
  useBackForwardButtons(sectionRef, pane.goBack, pane.goForward);

  /** Back to the file list, so the keyboard works on files again after typing a path or filter. */
  const focusList = () =>
    sectionRef.current
      ?.querySelector<HTMLElement>(PANE_KEYBOARD_TARGET)
      ?.focus({ preventScroll: true });

  // The pane stays in the registry while it registers again under another session or name, so
  // what the registry was told of it since (on screen, used last, locked) holds; it leaves the
  // registry only with its tab.
  useEffect(
    () =>
      void registerPane({
        tabId,
        kind: source.kind,
        sessionId,
        label: title,
        path: () => paneRef.current.listing?.path ?? null,
        refresh: (focus) => refreshRef.current(focus),
        clear: () => paneRef.current.clear(),
        loading: () => paneRef.current.isLoading(),
        selected: () => paneRef.current.selectedEntries(),
        entries: () => paneRef.current.listing?.entries ?? [],
        showsResults: () => paneRef.current.showingResults,
        visible: false,
        activatedAt: 0,
      }),
    [tabId, source.kind, sessionId, title],
  );
  useEffect(() => () => unregisterPane(tabId), [tabId]);

  useEffect(() => {
    updatePane(tabId, active ? { visible, activatedAt: Date.now() } : { visible });
  }, [tabId, visible, active]);

  // The folder a pane shows is kept for the next start along with its tab.
  const listedPath = pane.listing?.path;
  useEffect(() => {
    if (listedPath) workspaceChanged();
  }, [listedPath]);

  // What a server's folder is by its listing: a locked vault, one too old to open, or neither.
  // An unlocked vault lists its decrypted files, among which nothing marks a vault.
  const listedEntries = pane.listing?.entries;
  const sign = useMemo(
    () =>
      source.kind === "remote" && !vault && listedEntries && !inResults
        ? vaultSign(listedEntries)
        : "none",
    [source.kind, vault, listedEntries, inResults],
  );
  /** The locked vault whose encrypted files the user asked to see, for as long as it shows. */
  const [revealedVault, setRevealedVault] = useState<string | null>(null);
  if (revealedVault !== null && revealedVault !== listedPath) setRevealedVault(null);
  /** The card asking for a locked vault's password stands in for the file list. */
  const lockedCard = sign === "locked" && revealedVault === null;
  useEffect(() => {
    updatePane(tabId, { locked: lockedCard });
  }, [tabId, lockedCard]);
  const folderCompare = useFolderCompare({
    tabId,
    source,
    sessionId,
    pane,
    covered: lockedCard,
    focusList,
  });
  /** Rows left out because they are identical in the pane this one is compared with. */
  const identicalHidden = hiddenIdentical ? (folderCompare.bar?.counts?.identical ?? 0) : 0;

  // The card takes the keyboard with it as it leaves, so the file list gets it back.
  const hadLockedCard = useRef(lockedCard);
  useEffect(() => {
    const left = hadLockedCard.current && !lockedCard;
    hadLockedCard.current = lockedCard;
    if (left && active && document.activeElement === document.body) {
      sectionRef.current
        ?.querySelector<HTMLElement>(PANE_KEYBOARD_TARGET)
        ?.focus({ preventScroll: true });
    }
  }, [lockedCard, active]);

  const dropFolder = useDragStore((state) => {
    const { payload, target, copy } = state;
    if (!payload || payload.kind === "tab" || target?.kind !== "pane" || target.tabId !== tabId) {
      return undefined;
    }
    if (payload.kind === "files") {
      const action = dropAction(payload.sourceTabId, payload.entries, target, copy);
      if (!action || action.kind === "blocked") return undefined;
    }
    return target.folder ?? "";
  });

  const side = useMemo(() => ({ kind: source.kind, sessionId }), [source.kind, sessionId]);
  const clip = useClipboardStore((state) => state.clip);
  const cutPaths = useMemo(
    () =>
      clip?.mode === "move" &&
      sameFilesystem(clip.origin, paneOrigin({ kind: source.kind, sessionId }))
        ? new Set(clip.entries.map((entry) => entry.path))
        : undefined,
    [clip, source.kind, sessionId],
  );

  const reportedError = useRef(pane.error);
  useEffect(() => {
    if (!pane.error || !pane.listing || reportedError.current === pane.error) return;
    reportedError.current = pane.error;
    // A listing left up because the server could not be reached says so in the pane.
    if (pane.stale !== "outdated") showToast("error", pane.error.message);
    writeLog("error", `${title}: ${pane.error.message}`);
  }, [pane.error, pane.listing, pane.stale, showToast, writeLog, title]);

  const selected = pane.selectedEntries();
  const selectedSize = selected.reduce(
    (total, entry) => total + (isDirLike(entry) ? 0 : entry.size),
    0,
  );

  const style = source.pathStyle;
  const searchBarShown = searchBar !== null || inResults;
  /** Where a search from the bar starts: where the results were found, or the folder on show. */
  const searchRoot = inResults ? searchView.root : (searchBar?.root ?? pane.listing?.path ?? null);

  const runSearch = (form: SearchForm = searchForm, root: string | null = searchRoot) => {
    if (!root) return;
    setSearchForm(form);
    const problem = search.start(root, form);
    setSearchProblem(problem);
    if (problem) return;
    useSearchStore.getState().remember(source.key, form);
    pane.showResults(root);
  };

  /** Opens the search bar for the folder on show, or for `folder` inside it. */
  const openSearch = (folder?: string) => {
    if (folder !== undefined && inResults) {
      runSearch(searchForm, folder);
      return;
    }
    setSearchBar({ root: folder ?? null });
    setSearchProblem(null);
    setSearchFocus((count) => count + 1);
  };

  const closeSearch = () => {
    search.discard();
    setSearchBar(null);
    setSearchProblem(null);
    setSearchFocus(0);
    if (inResults) pane.closeResults();
    focusList();
  };

  // Leaving the results for a folder, by Back or by going there, puts the bar away. The
  // results stay, for Back to return to.
  const wasInResults = useRef(false);
  useEffect(() => {
    if (wasInResults.current && !inResults) {
      setSearchBar(null);
      setSearchProblem(null);
      setSearchFocus(0);
      const current = paneRef.current;
      if (current.sort.key === "folder") {
        current.setSort(useSettingsStore.getState().settings.interface.sort);
      }
    }
    wasInResults.current = inResults;
  }, [inResults]);

  // Results belong to the session that found them.
  const discardSearch = search.discard;
  const searchedSource = useRef(source);
  useEffect(() => {
    if (searchedSource.current === source) return;
    searchedSource.current = source;
    discardSearch();
    setSearchBar(null);
  }, [source, discardSearch]);

  /** Looks at the results again after their folder changed, as the folder would reload. */
  const refreshResults = () => {
    if (!searchView) return;
    const clip = useClipboardStore.getState().clip;
    const clipped =
      clip && sameFilesystem(clip.origin, paneOrigin(side))
        ? clip.entries.map((entry) => entry.path)
        : [];
    void search.recheck(
      recheckTargets(searchView.entries, searchView.root, style, [...pane.selection, ...clipped]),
    );
  };
  useLayoutEffect(() => {
    refreshRef.current = inResults
      ? () => refreshResults()
      : (focus) => void paneRef.current.refresh(focus);
  });

  /**
   * After results were renamed, moved or deleted: they are looked at again, with `added` paths
   * they go by now and the results inside changed folders. Moves and deletes on the server
   * reload the panes showing their folders themselves; for the rest, `reload` has it done.
   */
  const resultsChanged = async (
    entries: FileEntry[],
    added: string[] = [],
    focus?: string,
    reload = false,
  ) => {
    if (reload) refreshFoldersOf(paneOrigin(side), entries);
    await search.recheck(recheckAfterChange(searchView?.entries ?? [], entries, added, style));
    if (focus) pane.selectOnly(focus);
  };

  /** The folder of results acted on together: theirs when they share one. */
  const folderOfResults = (entries: FileEntry[]): string =>
    sharedFolder(entries, style) ?? searchView?.root ?? pane.listing?.path ?? "";

  /** Leaves the results for the folder `entry` is in, with it selected there. */
  const goToFolder = (entry: FileEntry) => void pane.navigate(folderOf(entry, style), entry.path);

  const folderLabel = useCallback(
    (entry: FileEntry) => (resultsRoot ? relativeFolder(resultsRoot, entry, style) : ""),
    [resultsRoot, style],
  );
  /** The list's view of the pane: F5 on results searches again. */
  const listPane = inResults
    ? {
        ...pane,
        refresh: async () => {
          runSearch();
          return true;
        },
      }
    : pane;

  /** Closes `shown` only, so a dialog that finishes its work after it was dismissed leaves alone
   * the one opened since. */
  const closeDialog = (shown: PaneDialog) => () =>
    setDialog((current) => (current === shown ? null : current));
  const openNewFolder = () => pane.listing && !inResults && setDialog({ type: "newFolder" });
  const openNewFile = () =>
    pane.listing && !inResults && setDialog({ type: "newFile", example: exampleFileName() });
  /** A vault goes on a server, in a folder that is not a vault's own. */
  const canMakeVault = source.kind === "remote" && Boolean(sessionId) && !vault && !inResults;
  const openNewVault = () => pane.listing && sign === "none" && setDialog({ type: "newVault" });

  /** Shows the vault at `root` once it is unlocked: the path is the vault's from then on, so
   * going there gives the tab the vault's session. */
  const enterVault = async (root: string) => {
    if (!(await pane.navigate(root))) await pane.refresh(root);
  };

  /** Set while this pane unlocks a vault and goes into it by itself. */
  const unlocking = useRef(false);
  const unlockVault = async (password: string) => {
    if (!sessionId || !pane.listing) return;
    const folder = pane.listing.path;
    unlocking.current = true;
    try {
      const info = await useSessionStore.getState().unlockVault(sessionId, folder, password);
      writeLog("info", `Unlocked vault ${info.vault?.root ?? folder}`, sessionId);
      await enterVault(info.vault?.root ?? folder);
    } finally {
      unlocking.current = false;
    }
  };

  // A vault unlocked from another pane holds the folder on show here from then on, so this
  // pane goes into the vault too, instead of asking for a password that is no longer needed.
  const unlockedElsewhere = useSessionStore((state) => {
    if (!sessionId || !listedPath) return undefined;
    const showing = sessionShowing(state.sessions, sessionId, listedPath);
    return showing === sessionId ? undefined : state.sessions[showing]?.info.vault?.root;
  });
  useEffect(() => {
    if (unlockedElsewhere === undefined || unlocking.current) return;
    void paneRef.current.navigate(unlockedElsewhere);
  }, [unlockedElsewhere]);

  const createVault = async (name: string, password: string) => {
    if (!sessionId || !pane.listing) return;
    const folder = pane.listing.path;
    const info = await useSessionStore.getState().createVault(sessionId, folder, name, password);
    writeLog("info", `Created vault ${info.vault?.root ?? name} in ${folder}`, sessionId);
    if (info.vault) await enterVault(info.vault.root);
  };

  /** The session of the vault to lock from here: the one the pane is in, or the unlocked one
   * whose folder `entry` is in the server's own listing. */
  const lockableVault = (entry: FileEntry | null): string | undefined => {
    if (!sessionId) return undefined;
    if (vault) return sessionId;
    if (!entry) return undefined;
    return vaultSessions(useSessionStore.getState().sessions, sessionId).find((info) =>
      samePath(info.vault.root, entry.path, "posix"),
    )?.id;
  };

  /** The pane's shortcuts for going elsewhere, from the card that stands in for the file list. */
  const handleCardKey = (event: KeyboardEvent<HTMLElement>) => {
    if (!event.altKey) return;
    const go = {
      ArrowLeft: pane.goBack,
      ArrowRight: pane.goForward,
      ArrowUp: pane.goUp,
      Home: pane.goHome,
    }[event.key];
    if (!go) return;
    event.preventDefault();
    go();
  };
  const renameEntry = async (entry: FileEntry, newName: string): Promise<boolean> => {
    try {
      const renamed = await source.rename(entry.path, newName);
      writeLog("info", `Renamed ${entry.path} to ${renamed}`);
      setRenaming(null);
      if (inResults) await resultsChanged([entry], [renamed], renamed, true);
      else await pane.refresh(renamed);
      return true;
    } catch (caught) {
      showToast("error", toAppError(caught).message);
      return false;
    }
  };
  const openBulkRename = (entries: FileEntry[]) => {
    const listing = pane.listing;
    if (!listing || entries.length < 2) return;
    setDialog({
      type: "bulkRename",
      origin: paneOrigin({ kind: source.kind, sessionId }),
      folder: listing.path,
      entries,
    });
  };
  const openDelete = () => selected.length > 0 && setDialog({ type: "delete", entries: selected });

  const copyPaths = (entries: FileEntry[]) => {
    const text =
      entries.length > 0 ? entries.map((entry) => entry.path).join("\n") : pane.listing?.path;
    if (text) void navigator.clipboard.writeText(text);
  };

  const markActive = () => updatePane(tabId, { visible: true, activatedAt: Date.now() });

  /** Opens or closes the preview. The keyboard goes back to the list if the preview had it. */
  const switchPreview = () => {
    const leaving = sectionRef.current?.querySelector(".preview")?.contains(document.activeElement);
    togglePreview(tabId);
    if (leaving) focusList();
  };

  /** The preview takes the keyboard as soon as it is open, when it was opened to take it. */
  const previewWantsFocus = useRef(false);
  const focusPreview = () =>
    sectionRef.current?.querySelector<HTMLElement>(".preview")?.focus({ preventScroll: true });
  /** Gives the keyboard to the preview, opening it first if it is closed. */
  const goToPreview = () => {
    if (previewOpen) {
      focusPreview();
      return;
    }
    previewWantsFocus.current = true;
    togglePreview(tabId);
  };
  useEffect(() => {
    if (!previewOpen || !previewWantsFocus.current) return;
    previewWantsFocus.current = false;
    focusPreview();
  }, [previewOpen]);

  const startDrag = (event: ReactPointerEvent<HTMLElement>, entry: FileEntry) => {
    const wasSelected = pane.selection.has(entry.path);
    const selectedNow = pane.selectedEntries();
    const drop: DropHandler = (target, payload, copy) => {
      if (target.kind !== "pane" || payload.kind !== "files") return;
      const action = dropAction(tabId, payload.entries, target, copy);
      // The pile flies to where the files were let go of.
      const landing = { ...useDragStore.getState().pointer };
      if (action?.kind === "transfer") {
        transferFromPane(tabId, payload.entries, target.tabId, target.folder, landing);
      } else if (action?.kind === "blocked") {
        showToast("info", `${action.reason}.`);
      } else if (action?.kind === "place") {
        // Results already in the folder they are dropped on stay where they are.
        const entries = inResults
          ? payload.entries.filter(
              (entry) => !samePath(folderOf(entry, style), action.folder, style),
            )
          : payload.entries;
        void placeEntries({
          mode: action.mode,
          origin: paneOrigin(action.source),
          sourceFolder: inResults ? folderOfResults(entries) : (action.source.path() ?? ""),
          entries,
          target: action.target,
          targetFolder: action.folder,
          sourceTabId: tabId,
          landing,
        }).then((done) => {
          if (done && inResults && action.mode === "move") void resultsChanged(entries);
          const shown = getPane(target.tabId);
          if (done && target.folder === null && shown) {
            sayWhereResultsDropWent(
              shown,
              action.mode === "move" ? "Moved" : "Copied",
              action.folder,
            );
          }
        });
      }
    };
    beginDrag(
      event,
      () => ({ kind: "files", sourceTabId: tabId, entries: wasSelected ? selectedNow : [entry] }),
      drop,
      (payload) => void dragFilesOut(payload, drop),
    );
  };

  /** The group this pane is in, where editors and terminals it opens go. */
  const ownGroup = () => groupOfTab(useLayoutStore.getState().root, tabId)?.id;

  const edit = (entry: FileEntry) => {
    if (isDirLike(entry)) return;
    const location =
      source.kind === "remote" && sessionId
        ? { side: "remote" as const, sessionId, path: entry.path }
        : { side: "local" as const, path: entry.path };
    void openInEditor(location, ownGroup());
  };

  const openTarget = (entry: FileEntry): OpenTarget =>
    source.kind === "remote" && sessionId
      ? { side: "remote", sessionId, path: entry.path, name: entry.name }
      : { side: "local", path: entry.path, name: entry.name };

  /** Open: the file's own program, with Ctrl+Enter. */
  const openExternally = (entry: FileEntry) => {
    if (!isDirLike(entry)) void openWithDefault(openTarget(entry));
  };

  const cutOrCopy = (mode: PlaceMode, entries: FileEntry[]) => {
    const self = getPane(tabId);
    if (self) putOnClipboard(mode, self, entries);
  };

  /** Into the folder clicked, else the one on show. */
  const pasteInto = (entry: FileEntry | null) => {
    const self = getPane(tabId);
    const folder = entry && isDirLike(entry) ? entry.path : inResults ? null : pane.listing?.path;
    if (!folder && inResults) {
      showToast("info", "To paste, open a folder or right-click one in the results.");
      return;
    }
    if (self && folder) void paste(self, folder);
  };

  const openProperties = (entries: FileEntry[]) => {
    if (overSftp && entries.length > 0) setDialog({ type: "properties", entries });
  };

  const activateFile = (entry: FileEntry) => {
    if (doubleClickFile === "transfer") transferToOtherSide(tabId, [entry]);
    else if (doubleClickFile === "edit") edit(entry);
    else if (doubleClickFile === "open") openExternally(entry);
    else if (doubleClickFile === "ask") {
      useExternalEditStore.setState({
        prompt: { kind: "choose", target: openTarget(entry), nearGroupId: ownGroup() },
      });
    }
  };

  const transferMenuItem = (targets: FileEntry[]): MenuItem => {
    const upload = source.kind === "local";
    const destination = lastActivePane(upload ? "remote" : "local");
    const verb = upload ? "Upload" : "Download";
    return {
      label: destination
        ? `${verb} to ${upload ? destination.label : (destination.path() ?? "local folder")}`
        : `${verb} (${upload ? "no server open" : "no local tab open"})`,
      icon: upload ? <Upload size={14} /> : <Download size={14} />,
      disabled: !destination || targets.length === 0,
      onSelect: () => transferToOtherSide(tabId, targets),
    };
  };

  /** Synchronizes the folder clicked, or the one shown, with the other side. A file among
   * search results stands for the folder it is in. */
  const syncMenuItem = (entry: FileEntry | null): MenuItem => {
    const folder =
      entry && isDirLike(entry)
        ? entry.path
        : entry && inResults
          ? folderOf(entry, style)
          : (pane.listing?.path ?? "");
    // Synchronization runs over SSH.
    const synchronizable = source.kind === "local" || protocol === "sftp";
    return {
      label: synchronizable ? "Synchronize folder..." : `Synchronize folder (${needsSftp})`,
      icon: <FolderSync size={14} />,
      disabled: !folder || !synchronizable,
      onSelect: () =>
        openDialog(
          source.kind === "local"
            ? { kind: "sync", localPath: folder }
            : { kind: "sync", sessionId, remotePath: folder },
        ),
    };
  };

  const showContextMenu = (position: MenuPosition, entry: FileEntry | null) => {
    setMenu({ ...position, items: contextMenuItems(entry), path: entry?.path });
  };

  const showDriveMenu = async (event: MouseEvent<HTMLButtonElement>) => {
    if (!source.roots) return;
    const anchor = event.currentTarget.getBoundingClientRect();
    const drives = await source.roots();
    setMenu({
      x: anchor.left,
      y: anchor.bottom + 4,
      items: drives.map((drive) => ({
        label: drive,
        icon: <HardDrive size={14} />,
        onSelect: () => void pane.navigate(drive),
      })),
    });
  };

  const columns = availableColumns(pane.listing?.entries ?? []);

  const sortMenu = (): MenuItem => ({
    label: "Sort by",
    icon: <ArrowUpDown size={14} />,
    items: [
      ...(["name", ...(inResults ? ["folder"] : []), ...columns] as SortKey[]).map((key) => ({
        label: key === "name" ? "Name" : key === "folder" ? "Folder" : COLUMN_LABELS[key],
        checked: pane.sort.key === key,
        onSelect: () => pane.setSort({ key, direction: pane.sort.direction }),
      })),
      "separator",
      ...([1, -1] as const).map((direction) => ({
        label: direction === 1 ? "Ascending" : "Descending",
        checked: pane.sort.direction === direction,
        onSelect: () => pane.setSort({ key: pane.sort.key, direction }),
      })),
      "separator",
      {
        label: "Folders first",
        checked: foldersFirst,
        onSelect: () => void saveSettingsSection("interface", { foldersFirst: !foldersFirst }),
      },
    ],
  });

  // Explorer's Group by menu, for the one grouping Poros has.
  const groupMenu = (): MenuItem => ({
    label: "Group by",
    icon: <ListTree size={14} />,
    items: [
      ...([true, false] as const).map((grouped) => ({
        label: grouped ? "Date modified" : "None",
        checked: groupByDate === grouped,
        onSelect: () => void saveSettingsSection("appearance", { [groupSetting]: grouped }),
      })),
      "separator",
      {
        label: "Collapse all groups",
        disabled: !pane.groups || pane.collapsedGroups.size === pane.groups.length,
        onSelect: () => pane.setAllGroupsCollapsed(true),
      },
      {
        label: "Expand all groups",
        disabled: !pane.groups || pane.collapsedGroups.size === 0,
        onSelect: () => pane.setAllGroupsCollapsed(false),
      },
    ],
  });

  const showHeaderMenu = (event: MouseEvent) => {
    const toggleColumn = (key: DetailColumn) =>
      void saveSettingsSection("interface", {
        hiddenColumns: hiddenColumns.includes(key)
          ? hiddenColumns.filter((hidden) => hidden !== key)
          : [...hiddenColumns, key],
      });
    setMenu({
      x: event.clientX,
      y: event.clientY,
      items: [
        ...columns.map((key) => ({
          label: COLUMN_LABELS[key],
          checked: !hiddenColumns.includes(key),
          onSelect: () => toggleColumn(key),
        })),
        "separator",
        {
          label: "Reset column widths",
          icon: <RotateCcw size={14} />,
          disabled: Object.keys(columnWidths).length === 0,
          onSelect: () => void saveSettingsSection("interface", { columnWidths: {} }),
        },
        sortMenu(),
        groupMenu(),
      ],
    });
  };

  const compareMenuItem = (targets: FileEntry[]): MenuItem | null => {
    const self = getPane(tabId);
    if (!self || targets.length === 0 || targets.length > 2 || !targets.every(isFileLike)) {
      return null;
    }
    if (!worksHere) {
      return {
        label: `Compare files (${needsSftp})`,
        icon: <FileDiff size={14} />,
        disabled: true,
        onSelect: () => undefined,
      };
    }
    if (targets.length === 2) {
      return {
        label: "Compare the two files",
        icon: <FileDiff size={14} />,
        onSelect: () =>
          setDialog({
            type: "compare",
            left: compareSide(self, targets[0]),
            right: compareSide(self, targets[1]),
          }),
      };
    }
    const other = counterpartFile(tabId);
    return {
      label: other
        ? `Compare with ${other.entry.name}`
        : "Compare files (select a file in another tab)",
      icon: <FileDiff size={14} />,
      disabled: !other,
      onSelect: () =>
        other &&
        setDialog({
          type: "compare",
          left: compareSide(self, targets[0]),
          right: compareSide(other.pane, other.entry),
        }),
    };
  };

  const clipboardMenuItems = (entry: FileEntry | null, targets: FileEntry[]): MenuItem[] => [
    {
      label: "Cut",
      icon: <Scissors size={14} />,
      shortcut: "Ctrl+X",
      disabled: targets.length === 0,
      onSelect: () => cutOrCopy("move", targets),
    },
    {
      label: "Copy",
      icon: <Copy size={14} />,
      shortcut: "Ctrl+C",
      disabled: targets.length === 0,
      onSelect: () => cutOrCopy("copy", targets),
    },
    {
      label: entry && isDirLike(entry) ? `Paste into ${entry.name}` : "Paste",
      icon: <ClipboardPaste size={14} />,
      shortcut: "Ctrl+V",
      disabled: !clip,
      onSelect: () => pasteInto(entry),
    },
    {
      label: worksHere ? "Move to..." : `Move to (${needsSftp})`,
      icon: <FolderInput size={14} />,
      disabled: targets.length === 0 || !worksHere,
      onSelect: () => setDialog({ type: "placeTo", mode: "move", entries: targets }),
    },
    {
      label: worksHere ? "Copy to..." : `Copy to (${needsSftp})`,
      icon: <CopyPlus size={14} />,
      disabled: targets.length === 0 || !worksHere,
      onSelect: () => setDialog({ type: "placeTo", mode: "copy", entries: targets }),
    },
  ];

  /** Extract here, and into a folder named after the archive, for the archives among `targets`. */
  const extractMenuItems = (targets: FileEntry[]): MenuItem[] => {
    const archives = archivesAmong(targets);
    if (archives.length === 0) return [];
    if (!worksHere) {
      return [
        {
          label: `Extract (${needsSftp})`,
          icon: <PackageOpen size={14} />,
          disabled: true,
          onSelect: () => undefined,
        },
      ];
    }
    const self = { kind: source.kind, sessionId };
    const separator = source.pathStyle === "windows" ? "\\" : "/";
    const [first] = archives;
    const allSingle = archives.every((archive) => {
      const format = archiveFormatOf(archive.name);
      return format !== null && isSingleFile(format);
    });
    return [
      {
        label: archives.length > 1 ? `Extract ${archives.length} archives here` : "Extract here",
        icon: <PackageOpen size={14} />,
        onSelect: () => void extractArchives(self, archives, "here"),
      },
      ...(allSingle
        ? []
        : [
            {
              label:
                archives.length > 1
                  ? "Extract each to its own folder"
                  : `Extract to "${unpackedName(first.name)}${separator}"`,
              icon: <FolderOutput size={14} />,
              onSelect: () => void extractArchives(self, archives, "folder"),
            },
          ]),
    ];
  };

  /** Commands run in one folder, so results from several have none to run in. */
  const commandsMenuItem = (server: string, shown: string, targets: FileEntry[]): MenuItem => {
    const folder = !inResults ? shown : targets.length > 0 ? sharedFolder(targets, style) : shown;
    if (folder === null) {
      return {
        label: "Commands (items in one folder only)",
        icon: <SquareTerminal size={14} />,
        disabled: true,
        onSelect: () => undefined,
      };
    }
    return serverCommandsMenu(server, { folder, items: targets });
  };

  const contextMenuItems = (entry: FileEntry | null): MenuItem[] => {
    const targets = entry ? (pane.selection.has(entry.path) ? selected : [entry]) : [];
    const compare = compareMenuItem(targets);
    const extract = extractMenuItems(targets);
    const lockable = lockableVault(targets.length > 1 ? null : entry);
    const shareItem =
      source.kind === "remote"
        ? shareMenuItem(targets, sharing, Boolean(vault), () =>
            setDialog({ type: "share", entries: targets }),
          )
        : null;
    return [
      ...(targets.length > 0 ? [transferMenuItem(targets), "separator" as const] : []),
      ...(entry && isDirLike(entry)
        ? [
            {
              label: "Open",
              icon: <FolderOpen size={14} />,
              shortcut: "Enter",
              onSelect: () => pane.open(entry),
            },
          ]
        : []),
      ...(entry && !isDirLike(entry)
        ? [
            ...openMenuItems(openTarget(entry), ownGroup()),
            {
              label: "Edit",
              icon: <FilePen size={14} />,
              shortcut: "F4",
              onSelect: () => edit(entry),
            },
          ]
        : []),
      ...(inResults && entry && targets.length === 1
        ? [
            {
              label: "Go to folder",
              icon: <LocateFixed size={14} />,
              onSelect: () => goToFolder(entry),
            },
          ]
        : []),
      ...((entry && isDirLike(entry) && targets.length === 1) || (!entry && !inResults)
        ? [
            {
              label: "Search in this folder",
              icon: <FolderSearch size={14} />,
              shortcut: entry ? undefined : "Ctrl+Shift+F",
              onSelect: () => openSearch(entry?.path),
            },
          ]
        : []),
      ...(extract.length > 0 ? ["separator" as const, ...extract, "separator" as const] : []),
      {
        label: inResults ? "Search again" : "Refresh",
        icon: <RefreshCw size={14} />,
        shortcut: "F5",
        onSelect: () => (inResults ? runSearch() : void pane.refresh()),
      },
      ...(inResults
        ? []
        : [
            {
              label: "New folder",
              icon: <FolderPlus size={14} />,
              shortcut: "F7",
              onSelect: openNewFolder,
            },
            {
              label: "New file",
              icon: <FilePlus size={14} />,
              shortcut: "Shift+F4",
              onSelect: openNewFile,
            },
          ]),
      ...(canMakeVault
        ? [
            {
              label: "New vault...",
              icon: <ShieldPlus size={14} />,
              disabled: sign !== "none",
              onSelect: openNewVault,
            },
          ]
        : []),
      ...(lockable
        ? [
            {
              label: "Lock vault",
              icon: <Lock size={14} />,
              onSelect: () => void requestLockVault(lockable),
            },
          ]
        : []),
      ...(entry ? [] : [sortMenu(), groupMenu()]),
      syncMenuItem(entry),
      ...folderCompareMenuItems(folderCompare, entry === null),
      // Commands run over SSH.
      ...(sessionId && protocol === "sftp" && pane.listing
        ? [commandsMenuItem(sessionId, pane.listing.path, targets)]
        : []),
      "separator",
      ...clipboardMenuItems(entry, targets),
      "separator",
      {
        label: "Rename",
        icon: <Pencil size={14} />,
        shortcut: "F2",
        disabled: targets.length === 0,
        onSelect: () =>
          targets.length === 1 ? setRenaming(targets[0].path) : openBulkRename(targets),
      },
      {
        label: targets.length > 1 ? "Copy paths" : "Copy path",
        icon: <ClipboardCopy size={14} />,
        onSelect: () => copyPaths(targets),
      },
      ...(shareItem ? [shareItem] : []),
      ...(compare ? [compare] : []),
      ...(source.kind === "remote" && targets.length > 0
        ? [
            {
              label: overSftp ? "Properties" : `Properties (${needsSftp})`,
              icon: <Info size={14} />,
              shortcut: overSftp ? "Alt+Enter" : undefined,
              disabled: !overSftp,
              onSelect: () => openProperties(targets),
            },
          ]
        : []),
      "separator",
      {
        label: targets.length > 1 ? `Delete ${targets.length} items` : "Delete",
        icon: <Trash2 size={14} />,
        shortcut: "Del",
        danger: true,
        disabled: targets.length === 0,
        onSelect: () => setDialog({ type: "delete", entries: targets }),
      },
    ];
  };

  return (
    <section
      ref={sectionRef}
      className={[
        "pane",
        active && "is-active",
        dropFolder === "" && "is-drop-target",
        pane.stale && `is-${pane.stale}`,
      ]
        .filter(Boolean)
        .join(" ")}
      aria-busy={pane.loading}
      // Nothing is dropped unseen among a locked vault's encrypted files.
      data-drop-pane={lockedCard ? undefined : tabId}
      onMouseDown={markActive}
      onKeyDown={(event) => {
        // From anywhere in the pane; the file list handles it itself while it has the keyboard.
        const primary = event.ctrlKey || event.metaKey;
        if (primary && event.shiftKey && !event.altKey && event.key.toLowerCase() === "d") {
          if (dialog || menu) return;
          event.preventDefault();
          folderCompare.toggle();
          return;
        }
        // Not from a dialog the pane opened, whose keys reach here without being inside it.
        const inPane = event.currentTarget.contains(event.target as Node);
        if (!inPane) return;
        const plain = !(event.ctrlKey || event.metaKey || event.altKey || event.shiftKey);
        if (event.key === "F3" && plain) switchPreview();
        else if (isFocusPreviewKey(event)) goToPreview();
        else return;
        event.preventDefault();
      }}
    >
      <header className="pane-header">
        <div className="pane-title">
          {icon}
          <span title={title}>{title}</span>
        </div>
        <div className="pane-toolbar">
          {vault && sessionId && (
            <>
              <button
                type="button"
                className="toolbar-action"
                title="Lock vault"
                onClick={() => void requestLockVault(sessionId)}
              >
                <Lock size={14} />
                Lock
              </button>
              <span className="toolbar-divider" />
            </>
          )}
          {unsaved && sessionId && (
            <ToolbarButton
              label="Save this connection"
              onClick={() => openDialog({ kind: "saveConnection", sessionId })}
            >
              <Bookmark size={15} />
            </ToolbarButton>
          )}
          {sessionId && overSftp && (
            <ToolbarButton
              label="Open terminal"
              onClick={() => openTerminal(sessionId, ownGroup())}
            >
              <SquareTerminal size={15} />
            </ToolbarButton>
          )}
          <ToolbarButton label="Back (Alt+Left)" onClick={pane.goBack} disabled={!pane.canGoBack}>
            <ArrowLeft size={15} />
          </ToolbarButton>
          <ToolbarButton
            label="Forward (Alt+Right)"
            onClick={pane.goForward}
            disabled={!pane.canGoForward}
          >
            <ArrowRight size={15} />
          </ToolbarButton>
          <ToolbarButton label="Up (Backspace)" onClick={pane.goUp} disabled={!pane.canGoUp}>
            <ArrowUp size={15} />
          </ToolbarButton>
          <ToolbarButton label="Home (Alt+Home)" onClick={pane.goHome}>
            <Home size={15} />
          </ToolbarButton>
          {source.roots && (
            <ToolbarButton label="Drives" onClick={(event) => void showDriveMenu(event)}>
              <HardDrive size={15} />
            </ToolbarButton>
          )}
          <ToolbarButton
            label={inResults ? "Search again (F5)" : "Refresh (F5)"}
            onClick={() => (inResults ? runSearch() : void pane.refresh())}
            disabled={!pane.listing && !pane.failedPath}
          >
            <RefreshCw
              size={15}
              className={pane.loading || searchView?.status === "searching" ? "spin" : ""}
            />
          </ToolbarButton>
          <span className="toolbar-divider" />
          <ToolbarButton
            label="New folder (F7)"
            onClick={openNewFolder}
            disabled={!pane.listing || lockedCard || inResults}
          >
            <FolderPlus size={15} />
          </ToolbarButton>
          <ToolbarButton
            label="New file (Shift+F4)"
            onClick={openNewFile}
            disabled={!pane.listing || lockedCard || inResults}
          >
            <FilePlus size={15} />
          </ToolbarButton>
          {canMakeVault && (
            <ToolbarButton
              label="New vault"
              onClick={openNewVault}
              disabled={!pane.listing || sign !== "none"}
            >
              <ShieldPlus size={15} />
            </ToolbarButton>
          )}
          <ToolbarButton
            label={pane.showHidden ? "Hide hidden files (Ctrl+H)" : "Show hidden files (Ctrl+H)"}
            onClick={pane.toggleHidden}
            pressed={pane.showHidden}
          >
            {pane.showHidden ? <Eye size={15} /> : <EyeOff size={15} />}
          </ToolbarButton>
          <ToolbarButton
            label={searchBarShown ? "Close search (Ctrl+Shift+F)" : "Search (Ctrl+Shift+F)"}
            onClick={() => (searchBarShown ? closeSearch() : openSearch())}
            pressed={searchBarShown}
            disabled={!pane.listing || lockedCard}
          >
            <FolderSearch size={15} />
          </ToolbarButton>
          <ToolbarButton
            label={`Compare folders (${COMPARE_FOLDERS_SHORTCUT})`}
            onClick={folderCompare.toggle}
            pressed={folderCompare.comparing}
            disabled={!pane.listing && !folderCompare.comparing}
          >
            <GitCompareArrows size={15} />
          </ToolbarButton>
          <ToolbarButton
            label={`Preview (F3), focus it (${FOCUS_PREVIEW_SHORTCUT})`}
            onClick={switchPreview}
            pressed={previewOpen}
          >
            <PanelRight size={15} />
          </ToolbarButton>
        </div>
      </header>

      <div className="pane-locationbar">
        {vault && (
          <span
            className="vault-chip"
            title={`Files here are encrypted in the vault ${vault.name} before they reach the server`}
          >
            <ShieldCheck size={13} />
            Encrypted
          </span>
        )}
        <PathBar
          path={pane.listing?.path ?? pane.failedPath}
          pathStyle={source.pathStyle}
          vaultRoot={vault?.root}
          editRequest={pathEditRequest}
          onNavigate={pane.navigate}
          onDone={focusList}
        />
        <label className="filter">
          <Search size={13} />
          <input
            ref={filterRef}
            value={pane.filter}
            placeholder="Filter"
            title="Filter this folder (Ctrl+F)"
            spellCheck={false}
            onChange={(event) => pane.setFilter(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") pane.setFilter("");
              else if (event.key !== "Enter" && event.key !== "ArrowDown") return;
              event.preventDefault();
              focusList();
            }}
          />
          {pane.filter && (
            <button
              type="button"
              className="icon-button"
              aria-label="Clear filter"
              onClick={() => pane.setFilter("")}
            >
              <X size={12} />
            </button>
          )}
        </label>
      </div>

      {searchBarShown && searchRoot !== null && (
        <SearchBar
          form={searchForm}
          root={searchRoot}
          view={inResults ? searchView : null}
          recent={recentSearches}
          problem={searchProblem}
          focusRequest={searchFocus}
          onChange={setSearchForm}
          onSearch={(form) => runSearch(form)}
          onStop={search.stop}
          onClose={closeSearch}
          onLeave={focusList}
        />
      )}

      {pane.listing && lockedCard ? (
        <VaultUnlockCard
          key={pane.listing.path}
          name={baseName(pane.listing.path)}
          active={active}
          onUnlock={unlockVault}
          onShowEncrypted={() => setRevealedVault(pane.listing?.path ?? null)}
          onKeyDown={handleCardKey}
        />
      ) : pane.listing ? (
        <>
          {sign === "locked" && (
            <p className="pane-notice">
              <LockKeyhole size={13} />
              <span>This folder is a locked Cryptomator vault.</span>
              <button type="button" className="link-button" onClick={() => setRevealedVault(null)}>
                Unlock
              </button>
            </p>
          )}
          {sign === "outdated" && (
            <p className="pane-notice">
              <Info size={13} />
              <span>
                This vault uses an older Cryptomator format. Open it once in Cryptomator 1.6 or
                later to upgrade it.
              </span>
            </p>
          )}
          {pane.stale === "outdated" && (
            <p className="pane-notice pane-notice-warning" role="status">
              <TriangleAlert size={13} />
              <span>
                Could not refresh this folder: {pane.error?.message.trim().replace(/\.+$/, "")}.
                This listing may be out of date.
              </span>
              <button type="button" className="link-button" onClick={() => void pane.refresh()}>
                Try again
              </button>
            </p>
          )}
          {folderCompare.bar && (
            <FolderCompareBar
              bar={folderCompare.bar}
              onMenu={(anchor, items) => setMenu({ x: anchor.left, y: anchor.bottom + 4, items })}
            />
          )}
          <PreviewSplit
            preview={
              previewOpen ? (
                <PanePreview
                  pane={listPane}
                  source={source}
                  sessionId={sessionId}
                  dateFormat={dateFormat}
                  onOpenInEditor={edit}
                  onSwitchPane={focusNextPane}
                  onLeave={focusList}
                />
              ) : null
            }
          >
            <FileList
              pane={listPane}
              side={side}
              active={active}
              rowHeight={Math.max(rowHeight, fontSize + MIN_ROW_PADDING)}
              hiddenColumns={hiddenColumns}
              columnWidths={columnWidths}
              onColumnWidthsChange={(widths, done) => {
                if (done) void saveSettingsSection("interface", { columnWidths: widths });
                else previewSettingsSection("interface", { columnWidths: widths });
              }}
              dateFormat={dateFormat}
              striped={stripedRows}
              fontSize={fontSize}
              dropFolder={dropFolder}
              cutPaths={cutPaths}
              compareMarks={folderCompare.marks}
              onActivate={markActive}
              onContextMenu={showContextMenu}
              onHeaderContextMenu={showHeaderMenu}
              onDelete={openDelete}
              renaming={renaming}
              heldPaths={heldPaths}
              onRename={(entry) => setRenaming(entry.path)}
              onRenameSeveral={() => openBulkRename(pane.selectedEntries())}
              onRenameSubmit={renameEntry}
              onRenameCancel={() => setRenaming(null)}
              onEdit={edit}
              onOpenExternally={openExternally}
              onNewFolder={openNewFolder}
              onNewFile={openNewFile}
              onEditPath={() => setPathEditRequest((count) => count + 1)}
              onFocusFilter={() => filterRef.current?.focus()}
              onSwitchPane={(from) => focusNextPane(from)}
              onRowPointerDown={startDrag}
              onFileActivate={activateFile}
              onCut={() => cutOrCopy("move", pane.selectedEntries())}
              onCopy={() => cutOrCopy("copy", pane.selectedEntries())}
              onPaste={() => pasteInto(null)}
              onProperties={() => openProperties(pane.selectedEntries())}
              onCompareFolders={folderCompare.toggle}
              onSearch={() => (lockedCard ? undefined : openSearch())}
              folderOf={inResults ? folderLabel : undefined}
              emptyMessage={
                inResults
                  ? searchEmptyMessage(searchView)
                  : identicalHidden > 0
                    ? "Everything here is identical in both panes"
                    : undefined
              }
            />
          </PreviewSplit>
        </>
      ) : (
        <div className="pane-empty">
          {pane.error ? (
            <>
              <p className="pane-empty-title">
                {pane.failedPath ? "Could not refresh this folder" : "Could not open this folder"}
              </p>
              <p className="pane-empty-detail">{pane.error.message}</p>
              <div className="pane-empty-actions">
                {pane.failedPath && (
                  <button
                    type="button"
                    className="button"
                    disabled={pane.loading}
                    onClick={() => void pane.refresh()}
                  >
                    Try again
                  </button>
                )}
                <button type="button" className="button" onClick={pane.goHome}>
                  Go to home folder
                </button>
              </div>
            </>
          ) : (
            <p className="pane-empty-detail">Loading...</p>
          )}
        </div>
      )}

      {overlay}

      <footer className="pane-footer">
        <span>
          {lockedCard ? "Locked vault" : pluralize(pane.entryCount, inResults ? "result" : "item")}
          {!lockedCard && pane.hiddenCount > 0 && `, ${pane.hiddenCount} hidden`}
          {!lockedCard && identicalHidden > 0 && `, ${identicalHidden} identical not shown`}
        </span>
        {selected.length > 0 && (
          <span>
            {pluralize(selected.length, "selected", "selected")}
            {selectedSize > 0 && ` (${formatSize(selectedSize)})`}
          </span>
        )}
      </footer>

      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}

      {dialog?.type === "newFolder" && pane.listing && (
        <PromptDialog
          title="New folder"
          label={`Create in ${pane.listing.path}`}
          confirmLabel="Create"
          onClose={closeDialog(dialog)}
          onSubmit={async (name) => {
            const created = await source.mkdir(pane.listing!.path, name);
            writeLog("info", `Created ${where} folder ${created}`);
            await pane.refresh(created);
          }}
        />
      )}

      {dialog?.type === "newFile" && pane.listing && (
        <PromptDialog
          title="New file"
          label={`Create in ${pane.listing.path}`}
          placeholder={dialog.example}
          confirmLabel="Create"
          onClose={closeDialog(dialog)}
          onSubmit={async (name) => {
            const created = await source.makeFile(pane.listing!.path, name);
            writeLog("info", `Created ${where} file ${created}`);
            await pane.refresh(created);
          }}
        />
      )}

      {dialog?.type === "newVault" && pane.listing && (
        <NewVaultDialog
          folder={pane.listing.path}
          onCreate={createVault}
          onClose={closeDialog(dialog)}
        />
      )}

      {dialog?.type === "placeTo" && pane.listing && (
        <PromptDialog
          title={dialog.mode === "move" ? "Move to" : "Copy to"}
          label={`Folder to ${dialog.mode} ${
            dialog.entries.length === 1
              ? dialog.entries[0].name
              : pluralize(dialog.entries.length, "item")
          } into`}
          initialValue={pane.listing.path}
          confirmLabel={dialog.mode === "move" ? "Move" : "Copy"}
          onClose={closeDialog(dialog)}
          onSubmit={async (folder) => {
            const self = getPane(tabId);
            if (!self || !pane.listing) return;
            const { mode, entries } = dialog;
            void placeEntries({
              mode,
              origin: paneOrigin(self),
              sourceFolder: inResults ? folderOfResults(entries) : pane.listing.path,
              entries,
              target: self,
              targetFolder: folder,
            }).then((done) => {
              if (done && inResults && mode === "move") void resultsChanged(entries);
            });
          }}
        />
      )}

      {dialog?.type === "properties" && sessionId && pane.listing && (
        <PropertiesDialog
          sessionId={sessionId}
          folder={inResults ? folderOfResults(dialog.entries) : pane.listing.path}
          entries={dialog.entries}
          onClose={closeDialog(dialog)}
          onApplied={() => {
            if (!inResults) return void pane.refresh();
            // The dialog forgets the listing of the one folder it is given; results from
            // several folders have the listing of each forgotten, and its panes reloaded.
            refreshFoldersOf(paneOrigin(side), dialog.entries);
            void search.recheck(dialog.entries.map((entry) => entry.path));
          }}
        />
      )}

      {dialog?.type === "share" && sessionId && sharing && protocol && pane.listing && (
        <ShareDialog
          sessionId={sessionId}
          folder={inResults ? folderOfResults(dialog.entries) : pane.listing.path}
          folderStyle={inResults ? style : undefined}
          entries={dialog.entries}
          sharing={sharing}
          service={protocolInfo(protocol).shortLabel}
          onClose={() => setDialog(null)}
        />
      )}

      {dialog?.type === "bulkRename" && (
        <BulkRenameDialog
          origin={dialog.origin}
          folder={dialog.folder}
          chosen={dialog.entries}
          listing={pane.listing}
          stale={pane.stale}
          onRetry={() => void pane.refresh()}
          onClose={() => setDialog(null)}
        />
      )}

      {dialog?.type === "compare" && (
        <CompareDialog left={dialog.left} right={dialog.right} onClose={() => setDialog(null)} />
      )}

      {dialog?.type === "delete" && (
        <DeleteDialog
          chosen={dialog.entries}
          listing={pane.listing}
          checking={pane.stale === "refreshing"}
          summary={(entries) => <DeleteSummary entries={entries} where={where} />}
          onClose={closeDialog(dialog)}
          onConfirm={async (entries) => {
            const removed = entries.map((entry) => entry.path);
            const next = entryAfterRemoval(pane.visibleEntries, new Set(removed), pane.cursorPath);
            const folder = inResults ? folderOfResults(entries) : (pane.listing?.path ?? "");
            if (overSftp && pane.listing) {
              void deleteOnServer({
                origin: paneOrigin({ kind: source.kind, sessionId }),
                folder,
                entries,
                focusPath: next,
              }).then(() => {
                if (inResults) void resultsChanged(entries, [], next);
              });
              return;
            }
            await source.remove(removed);
            writeLog("info", `Deleted ${pluralize(entries.length, `${where} item`)} in ${folder}`);
            if (inResults) await resultsChanged(entries, [], next, true);
            else await pane.refresh(next);
          }}
        />
      )}
    </section>
  );
}

function DeleteSummary({ entries, where }: { entries: FileEntry[]; where: string }) {
  const folders = entries.filter((entry) => entry.kind === "dir").length;
  const preview = entries.slice(0, 5);
  return (
    <>
      <p>
        Permanently delete{" "}
        {entries.length === 1 ? (
          <strong>{entries[0].name}</strong>
        ) : (
          pluralize(entries.length, "item")
        )}{" "}
        from the {where} side? This cannot be undone.
      </p>
      {folders > 0 && (
        <p className="warning-text">Folders are deleted with everything inside them.</p>
      )}
      {entries.length > 1 && (
        <ul className="delete-preview">
          {preview.map((entry) => (
            <li key={entry.path}>{entry.name}</li>
          ))}
          {entries.length > preview.length && <li>and {entries.length - preview.length} more</li>}
        </ul>
      )}
    </>
  );
}

interface ToolbarButtonProps {
  label: string;
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  pressed?: boolean;
  children: ReactNode;
}

function ToolbarButton({ label, onClick, disabled, pressed, children }: ToolbarButtonProps) {
  return (
    <button
      type="button"
      className={`icon-button ${pressed ? "is-pressed" : ""}`}
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
