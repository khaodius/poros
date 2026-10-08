import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
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
  Download,
  Eye,
  EyeOff,
  FolderOpen,
  FolderPlus,
  FolderSync,
  HardDrive,
  Home,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { usePane } from "../hooks/usePane";
import { COLUMN_LABELS, availableColumns, type DetailColumn } from "../lib/columns";
import type { FileSource } from "../lib/fileSource";
import { formatSize, pluralize } from "../lib/format";
import { stemLength } from "../lib/path";
import { isDirLike, type SortKey, type SortSpec } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { beginDrag, useDragStore } from "../state/dragStore";
import { useLogStore } from "../state/logStore";
import { lastActivePane, registerPane, updatePane } from "../state/paneRegistry";
import { useSessionStore } from "../state/sessionStore";
import { saveSettingsSection, useSettingsStore } from "../state/settingsStore";
import { useToastStore } from "../state/toastStore";
import { transferFromPane, transferToOtherSide } from "../state/transferActions";
import { useUiStore } from "../state/uiStore";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { FileList } from "./FileList";
import { PathBar } from "./PathBar";
import { PromptDialog } from "./PromptDialog";
import { serverCommandsMenu } from "./commandMenu";

type PaneDialog =
  | { type: "newFolder" }
  | { type: "rename"; entry: FileEntry }
  | { type: "delete"; entries: FileEntry[] };

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
}

/** Moves keyboard focus to the next file list on screen, left to right, then top to bottom. */
function focusNextPane(from: HTMLElement | null) {
  const lists = [
    ...document.querySelectorAll<HTMLElement>(".tab-panel:not([hidden]) .file-list-body"),
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
  const dateFormat = useSettingsStore((state) => state.settings.interface.dateFormat);
  const rowHeight = useSettingsStore((state) => state.settings.appearance.rowHeight);
  const stripedRows = useSettingsStore((state) => state.settings.appearance.stripedRows);
  const fontSize = useSettingsStore((state) => state.settings.appearance.fontSize);
  const pane = usePane(source, {
    showHiddenByDefault,
    initialSort: useSettingsStore.getState().settings.interface.sort,
    foldersFirst,
    onSortChange: rememberSort,
  });
  const [dialog, setDialog] = useState<PaneDialog | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [pathEditRequest, setPathEditRequest] = useState(0);
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
  const where = source.kind === "remote" ? "remote" : "local";
  const paneRef = useRef(pane);
  useLayoutEffect(() => {
    paneRef.current = pane;
  });

  useEffect(
    () =>
      registerPane({
        tabId,
        kind: source.kind,
        sessionId,
        label: title,
        path: () => paneRef.current.listing?.path ?? null,
        refresh: () => void paneRef.current.refresh(),
        visible: false,
        activatedAt: 0,
      }),
    [tabId, source.kind, sessionId, title],
  );

  useEffect(() => {
    updatePane(tabId, active ? { visible, activatedAt: Date.now() } : { visible });
  }, [tabId, visible, active]);

  const dropFolder = useDragStore((state) => {
    const { payload, target } = state;
    if (!payload || payload.kind === "tab" || target?.kind !== "pane" || target.tabId !== tabId) {
      return undefined;
    }
    if (payload.kind === "files" && payload.sourceTabId === tabId) return undefined;
    return target.folder ?? "";
  });

  useEffect(() => {
    if (pane.error && pane.listing) {
      showToast("error", pane.error.message);
      writeLog("error", `${title}: ${pane.error.message}`);
    }
  }, [pane.error, pane.listing, showToast, writeLog, title]);

  const selected = pane.selectedEntries();
  const selectedSize = selected.reduce(
    (total, entry) => total + (isDirLike(entry) ? 0 : entry.size),
    0,
  );

  const openNewFolder = () => pane.listing && setDialog({ type: "newFolder" });
  const openRename = () =>
    selected.length === 1 && setDialog({ type: "rename", entry: selected[0] });
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
    beginDrag(
      event,
      () => ({ kind: "files", sourceTabId: tabId, entries: wasSelected ? selectedNow : [entry] }),
      (target, payload) => {
        if (target.kind !== "pane" || target.tabId === tabId || payload.kind !== "files") return;
        transferFromPane(tabId, payload.entries, target.tabId, target.folder);
      },
    );
  };

  const activateFile = (entry: FileEntry) => {
    if (doubleClickFile === "transfer") transferToOtherSide(tabId, [entry]);
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
      label: synchronizable ? "Synchronize folder..." : "Synchronize folder (SFTP only)",
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

  const showContextMenu = (event: MouseEvent, entry: FileEntry | null) => {
    setMenu({ x: event.clientX, y: event.clientY, items: contextMenuItems(entry) });
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
        sortMenu(),
      ],
    });
  };

  const contextMenuItems = (entry: FileEntry | null): MenuItem[] => {
    const targets = entry ? (pane.selection.has(entry.path) ? selected : [entry]) : [];
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
      ...(entry ? [] : [sortMenu()]),
      syncMenuItem(entry),
      // Commands run over SSH.
      ...(sessionId && protocol === "sftp" && pane.listing
        ? [serverCommandsMenu(sessionId, { folder: pane.listing.path, items: targets })]
        : []),
      "separator",
      {
        label: "Rename",
        icon: <Pencil size={14} />,
        shortcut: "F2",
        disabled: targets.length !== 1,
        onSelect: () => setDialog({ type: "rename", entry: targets[0] }),
      },
      {
        label: targets.length > 1 ? "Copy paths" : "Copy path",
        icon: <ClipboardCopy size={14} />,
        onSelect: () => copyPaths(targets),
      },
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
      className={["pane", active && "is-active", dropFolder === "" && "is-drop-target"]
        .filter(Boolean)
        .join(" ")}
      data-drop-pane={tabId}
      onMouseDown={markActive}
    >
      <header className="pane-header">
        <div className="pane-title">
          {icon}
          <span title={title}>{title}</span>
        </div>
        <div className="pane-toolbar">
          {unsaved && sessionId && (
            <ToolbarButton
              label="Save this connection"
              onClick={() => openDialog({ kind: "saveConnection", sessionId })}
            >
              <Bookmark size={15} />
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
          <ToolbarButton label="Home" onClick={pane.goHome}>
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
            disabled={!pane.listing}
          >
            <RefreshCw size={15} className={pane.loading ? "spin" : ""} />
          </ToolbarButton>
          <span className="toolbar-divider" />
          <ToolbarButton label="New folder (F7)" onClick={openNewFolder} disabled={!pane.listing}>
            <FolderPlus size={15} />
          </ToolbarButton>
          <ToolbarButton
            label={pane.showHidden ? "Hide hidden files" : "Show hidden files"}
            onClick={pane.toggleHidden}
            pressed={pane.showHidden}
          >
            {pane.showHidden ? <Eye size={15} /> : <EyeOff size={15} />}
          </ToolbarButton>
        </div>
      </header>

      <div className="pane-locationbar">
        <PathBar
          path={pane.listing?.path ?? null}
          pathStyle={source.pathStyle}
          editRequest={pathEditRequest}
          onNavigate={pane.navigate}
        />
        <label className="filter">
          <Search size={13} />
          <input
            ref={filterRef}
            value={pane.filter}
            placeholder="Filter"
            spellCheck={false}
            onChange={(event) => pane.setFilter(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                pane.setFilter("");
                event.currentTarget.blur();
              }
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

      {pane.listing ? (
        <FileList
          pane={pane}
          active={active}
          rowHeight={Math.max(rowHeight, fontSize + MIN_ROW_PADDING)}
          hiddenColumns={hiddenColumns}
          dateFormat={dateFormat}
          striped={stripedRows}
          fontSize={fontSize}
          dropFolder={dropFolder}
          onActivate={markActive}
          onContextMenu={showContextMenu}
          onHeaderContextMenu={showHeaderMenu}
          onDelete={openDelete}
          onRename={openRename}
          onNewFolder={openNewFolder}
          onEditPath={() => setPathEditRequest((count) => count + 1)}
          onFocusFilter={() => filterRef.current?.focus()}
          onSwitchPane={(from) => focusNextPane(from)}
          onRowPointerDown={startDrag}
          onFileActivate={activateFile}
        />
      ) : (
        <div className="pane-empty">
          {pane.error ? (
            <>
              <p className="pane-empty-title">Could not open this folder</p>
              <p className="pane-empty-detail">{pane.error.message}</p>
              <button type="button" className="button" onClick={pane.goHome}>
                Go to home folder
              </button>
            </>
          ) : (
            <p className="pane-empty-detail">Loading...</p>
          )}
        </div>
      )}

      {overlay}

      <footer className="pane-footer">
        <span>
          {pluralize(pane.visibleEntries.length, "item")}
          {pane.hiddenCount > 0 && `, ${pane.hiddenCount} hidden`}
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

      {dialog?.type === "rename" && (
        <PromptDialog
          title="Rename"
          label={`New name for ${dialog.entry.name}`}
          initialValue={dialog.entry.name}
          selectLength={stemLength(dialog.entry.name, isDirLike(dialog.entry))}
          confirmLabel="Rename"
          onClose={() => setDialog(null)}
          onSubmit={async (newName) => {
            if (newName === dialog.entry.name) return;
            const renamed = await source.rename(dialog.entry.path, newName);
            writeLog("info", `Renamed ${dialog.entry.path} to ${renamed}`);
            await pane.refresh(renamed);
          }}
        />
      )}

      {dialog?.type === "delete" && (
        <ConfirmDialog
          title={
            dialog.entries.length === 1 ? "Delete item" : `Delete ${dialog.entries.length} items`
          }
          confirmLabel="Delete"
          danger
          onClose={() => setDialog(null)}
          onConfirm={async () => {
            await source.remove(dialog.entries.map((entry) => entry.path));
            writeLog(
              "info",
              `Deleted ${pluralize(dialog.entries.length, `${where} item`)} in ${pane.listing?.path ?? ""}`,
            );
            await pane.refresh();
          }}
        >
          <DeleteSummary entries={dialog.entries} where={where} />
        </ConfirmDialog>
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
