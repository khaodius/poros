import { useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  ClipboardCopy,
  Eye,
  EyeOff,
  FolderOpen,
  FolderPlus,
  HardDrive,
  Home,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { usePane } from "../hooks/usePane";
import type { FileSource } from "../lib/fileSource";
import { formatSize, pluralize } from "../lib/format";
import { stemLength } from "../lib/path";
import { isDirLike } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { useLogStore } from "../state/logStore";
import { useToastStore } from "../state/toastStore";
import { ConfirmDialog } from "./ConfirmDialog";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { FileList } from "./FileList";
import { PathBar } from "./PathBar";
import { PromptDialog } from "./PromptDialog";

type PaneDialog =
  | { type: "newFolder" }
  | { type: "rename"; entry: FileEntry }
  | { type: "delete"; entries: FileEntry[] };

interface FilePaneProps {
  title: string;
  icon: ReactNode;
  source: FileSource;
  active: boolean;
  onActivate: () => void;
  onSwitchPane: () => void;
}

interface OpenMenu {
  x: number;
  y: number;
  items: MenuItem[];
}

export function FilePane({ title, icon, source, active, onActivate, onSwitchPane }: FilePaneProps) {
  const pane = usePane(source);
  const [dialog, setDialog] = useState<PaneDialog | null>(null);
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const [pathEditRequest, setPathEditRequest] = useState(0);
  const filterRef = useRef<HTMLInputElement>(null);
  const writeLog = useLogStore((state) => state.write);
  const showToast = useToastStore((state) => state.show);
  const where = source.kind === "remote" ? "remote" : "local";

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

  const contextMenuItems = (entry: FileEntry | null): MenuItem[] => {
    const targets = entry ? (pane.selection.has(entry.path) ? selected : [entry]) : [];
    return [
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
    <section className={`pane ${active ? "is-active" : ""}`} onMouseDown={onActivate}>
      <header className="pane-header">
        <div className="pane-title">
          {icon}
          <span title={title}>{title}</span>
        </div>
        <div className="pane-toolbar">
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
          onActivate={onActivate}
          onContextMenu={showContextMenu}
          onDelete={openDelete}
          onRename={openRename}
          onNewFolder={openNewFolder}
          onEditPath={() => setPathEditRequest((count) => count + 1)}
          onFocusFilter={() => filterRef.current?.focus()}
          onSwitchPane={onSwitchPane}
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
