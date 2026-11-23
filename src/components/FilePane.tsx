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
  FolderSync,
  HardDrive,
  Home,
  Info,
  ListTree,
  Lock,
  LockKeyhole,
  PackageOpen,
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
import { useListingPrefetch } from "../hooks/useListingPrefetch";
import { usePane } from "../hooks/usePane";
import { archiveFormatOf, isSingleFile, unpackedName } from "../lib/archives";
import { COLUMN_LABELS, availableColumns, type DetailColumn } from "../lib/columns";
import { entryAfterRemoval } from "../lib/cursor";
import { sameFilesystem, toLocation, worksInPlace } from "../lib/fileOrigin";
import type { FileSource } from "../lib/fileSource";
import { PANE_KEYBOARD_TARGET } from "../lib/focus";
import { toAppError } from "../lib/ipc";
import { formatSize, pluralize } from "../lib/format";
import { baseName, exampleFileName, samePath } from "../lib/path";
import { groupOfTab } from "../lib/layout";
import { isDirLike, type SortKey, type SortSpec } from "../lib/sort";
import type { FileEntry, PlaceMode } from "../lib/types";
import { sessionShowing, vaultSessions, vaultSign } from "../lib/vaults";
import { archivesAmong, extractArchives } from "../state/archiveActions";
import { useClipboardStore } from "../state/clipboardStore";
import { beginDrag, useDragStore, type DropHandler } from "../state/dragStore";
import { dragFilesOut } from "../state/dragOut";
import { openInEditor } from "../state/editorActions";
import {
  deleteOnServer,
  dropAction,
  paneOrigin,
  paste,
  placeEntries,
  putOnClipboard,
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
import { useSessionStore } from "../state/sessionStore";
import {
  previewSettingsSection,
  saveSettingsSection,
  useSettingsStore,
} from "../state/settingsStore";
import { openTerminal } from "../state/terminalActions";
import { useToastStore } from "../state/toastStore";
import { transferFromPane, transferToOtherSide } from "../state/transferActions";
import { useUiStore } from "../state/uiStore";
import { requestLockVault, vaultHandover } from "../state/vaultActions";
import { workspaceChanged } from "../state/workspace";
import { CompareDialog, type CompareSide } from "./CompareDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { DeleteDialog } from "./DeleteDialog";
import { FileList, type MenuPosition } from "./FileList";
import { NewVaultDialog } from "./NewVaultDialog";
import { PathBar } from "./PathBar";
import { PromptDialog } from "./PromptDialog";
import { PropertiesDialog } from "./PropertiesDialog";
import { VaultUnlockCard } from "./VaultUnlockCard";
import { serverCommandsMenu } from "./commandMenu";

type PaneDialog =
  | { type: "newFolder" }
  | { type: "newFile"; example: string }
  | { type: "newVault" }
  | { type: "delete"; entries: FileEntry[] }
  | { type: "placeTo"; mode: PlaceMode; entries: FileEntry[] }
  | { type: "properties"; entries: FileEntry[] }
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

/** Moves keyboard focus to the next file list on screen, left to right, then top to bottom. */
function focusNextPane(from: HTMLElement | null) {
  const lists = [
    ...document.querySelectorAll<HTMLElement>(`.tab-panel:not([hidden]) ${PANE_KEYBOARD_TARGET}`),
  ]
    .map((list) => ({ list, bounds: list.getBoundingClientRect() }))
    .sort(
      (first, second) =>
        first.bounds.left - second.bounds.left || first.bounds.top - second.bounds.top,
    )
    .map(({ list }) => list);
  if (lists.length < 2) return;
  const current = lists.findIndex((list) => list === from || list.contains(from));
  lists[(current + 1) % lists.length].focus();
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

const rememberSort = (sort: SortSpec) => void saveSettingsSection("interface", { sort });

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
  const pane = usePane(source, {
    showHiddenByDefault,
    initialSort: useSettingsStore.getState().settings.interface.sort,
    foldersFirst,
    groupByDate,
    onSortChange: rememberSort,
    handover,
    // This computer's folders list at once, so only a server's are remembered.
    listings: sessionId && cachedListings ? paneListings : undefined,
    heldPaths,
  });
  useListingPrefetch(pane, sessionId, visible);
  const [dialog, setDialog] = useState<PaneDialog | null>(null);
  const [pathEditRequest, setPathEditRequest] = useState(0);
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
        refresh: (focus) => void paneRef.current.refresh(focus),
        clear: () => paneRef.current.clear(),
        loading: () => paneRef.current.isLoading(),
        selected: () => paneRef.current.selectedEntries(),
        entries: () => paneRef.current.listing?.entries ?? [],
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
    () => (source.kind === "remote" && !vault && listedEntries ? vaultSign(listedEntries) : "none"),
    [source.kind, vault, listedEntries],
  );
  /** The locked vault whose encrypted files the user asked to see, for as long as it shows. */
  const [revealedVault, setRevealedVault] = useState<string | null>(null);
  if (revealedVault !== null && revealedVault !== listedPath) setRevealedVault(null);
  /** The card asking for a locked vault's password stands in for the file list. */
  const lockedCard = sign === "locked" && revealedVault === null;
  useEffect(() => {
    updatePane(tabId, { locked: lockedCard });
  }, [tabId, lockedCard]);

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

  const openNewFolder = () => pane.listing && setDialog({ type: "newFolder" });
  const openNewFile = () =>
    pane.listing && setDialog({ type: "newFile", example: exampleFileName() });
  /** A vault goes on a server, in a folder that is not a vault's own. */
  const canMakeVault = source.kind === "remote" && Boolean(sessionId) && !vault;
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
      await pane.refresh(renamed);
      return true;
    } catch (caught) {
      showToast("error", toAppError(caught).message);
      return false;
    }
  };
  const openDelete = () => selected.length > 0 && setDialog({ type: "delete", entries: selected });

  const copyPaths = (entries: FileEntry[]) => {
    const text =
      entries.length > 0 ? entries.map((entry) => entry.path).join("\n") : pane.listing?.path;
    if (text) void navigator.clipboard.writeText(text);
  };

  const markActive = () => updatePane(tabId, { visible: true, activatedAt: Date.now() });

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
        void placeEntries({
          mode: action.mode,
          origin: paneOrigin(action.source),
          sourceFolder: action.source.path() ?? "",
          entries: payload.entries,
          target: action.target,
          targetFolder: action.folder,
          sourceTabId: tabId,
          landing,
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

  const cutOrCopy = (mode: PlaceMode, entries: FileEntry[]) => {
    const self = getPane(tabId);
    if (self) putOnClipboard(mode, self, entries);
  };

  /** Into the folder clicked, else the one on show. */
  const pasteInto = (entry: FileEntry | null) => {
    const self = getPane(tabId);
    const folder = entry && isDirLike(entry) ? entry.path : pane.listing?.path;
    if (self && folder) void paste(self, folder);
  };

  const openProperties = (entries: FileEntry[]) => {
    if (overSftp && entries.length > 0) setDialog({ type: "properties", entries });
  };

  const activateFile = (entry: FileEntry) => {
    if (doubleClickFile === "transfer") transferToOtherSide(tabId, [entry]);
    else if (doubleClickFile === "edit") edit(entry);
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

  /** Synchronizes the folder clicked, or the one shown, with the other side. */
  const syncMenuItem = (entry: FileEntry | null): MenuItem => {
    const folder = entry && isDirLike(entry) ? entry.path : (pane.listing?.path ?? "");
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
      ...(["name", ...columns] as SortKey[]).map((key) => ({
        label: key === "name" ? "Name" : COLUMN_LABELS[key],
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
        label: `Compare (${needsSftp})`,
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
      label: other ? `Compare with ${other.entry.name}` : "Compare (select a file in another tab)",
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

  const contextMenuItems = (entry: FileEntry | null): MenuItem[] => {
    const targets = entry ? (pane.selection.has(entry.path) ? selected : [entry]) : [];
    const compare = compareMenuItem(targets);
    const extract = extractMenuItems(targets);
    const lockable = lockableVault(targets.length > 1 ? null : entry);
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
            {
              label: "Edit",
              icon: <FilePen size={14} />,
              shortcut: "F4",
              onSelect: () => edit(entry),
            },
          ]
        : []),
      ...(extract.length > 0 ? ["separator" as const, ...extract, "separator" as const] : []),
      {
        label: "Refresh",
        icon: <RefreshCw size={14} />,
        shortcut: "F5",
        onSelect: () => void pane.refresh(),
      },
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
      // Commands run over SSH.
      ...(sessionId && protocol === "sftp" && pane.listing
        ? [serverCommandsMenu(sessionId, { folder: pane.listing.path, items: targets })]
        : []),
      "separator",
      ...clipboardMenuItems(entry, targets),
      "separator",
      {
        label: "Rename",
        icon: <Pencil size={14} />,
        shortcut: "F2",
        disabled: targets.length !== 1,
        onSelect: () => setRenaming(targets[0].path),
      },
      {
        label: targets.length > 1 ? "Copy paths" : "Copy path",
        icon: <ClipboardCopy size={14} />,
        onSelect: () => copyPaths(targets),
      },
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
            label="Refresh (F5)"
            onClick={() => void pane.refresh()}
            disabled={!pane.listing && !pane.failedPath}
          >
            <RefreshCw size={15} className={pane.loading ? "spin" : ""} />
          </ToolbarButton>
          <span className="toolbar-divider" />
          <ToolbarButton
            label="New folder (F7)"
            onClick={openNewFolder}
            disabled={!pane.listing || lockedCard}
          >
            <FolderPlus size={15} />
          </ToolbarButton>
          <ToolbarButton
            label="New file (Shift+F4)"
            onClick={openNewFile}
            disabled={!pane.listing || lockedCard}
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
          <FileList
            pane={pane}
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
            onActivate={markActive}
            onContextMenu={showContextMenu}
            onHeaderContextMenu={showHeaderMenu}
            onDelete={openDelete}
            renaming={renaming}
            heldPaths={heldPaths}
            onRename={(entry) => setRenaming(entry.path)}
            onRenameSubmit={renameEntry}
            onRenameCancel={() => setRenaming(null)}
            onEdit={edit}
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
          />
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
          {lockedCard ? "Locked vault" : pluralize(pane.entryCount, "item")}
          {!lockedCard && pane.hiddenCount > 0 && `, ${pane.hiddenCount} hidden`}
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
          onClose={() => setDialog(null)}
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
          onClose={() => setDialog(null)}
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
          onClose={() => setDialog(null)}
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
          onClose={() => setDialog(null)}
          onSubmit={async (folder) => {
            const self = getPane(tabId);
            if (!self || !pane.listing) return;
            void placeEntries({
              mode: dialog.mode,
              origin: paneOrigin(self),
              sourceFolder: pane.listing.path,
              entries: dialog.entries,
              target: self,
              targetFolder: folder,
            });
          }}
        />
      )}

      {dialog?.type === "properties" && sessionId && pane.listing && (
        <PropertiesDialog
          sessionId={sessionId}
          folder={pane.listing.path}
          entries={dialog.entries}
          onClose={() => setDialog(null)}
          onApplied={() => void pane.refresh()}
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
          onClose={() => setDialog(null)}
          onConfirm={async (entries) => {
            const removed = entries.map((entry) => entry.path);
            const next = entryAfterRemoval(pane.visibleEntries, new Set(removed), pane.cursorPath);
            if (overSftp && pane.listing) {
              void deleteOnServer({
                origin: paneOrigin({ kind: source.kind, sessionId }),
                folder: pane.listing.path,
                entries,
                focusPath: next,
              });
              return;
            }
            await source.remove(removed);
            writeLog(
              "info",
              `Deleted ${pluralize(entries.length, `${where} item`)} in ${pane.listing?.path ?? ""}`,
            );
            await pane.refresh(next);
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
