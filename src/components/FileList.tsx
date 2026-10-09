import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, CornerLeftUp } from "lucide-react";
import { useFitCount } from "../hooks/useFitCount";
import type { PaneController } from "../hooks/usePane";
import { formatDate, formatPermissions, formatSize } from "../lib/format";
import { COLUMN_LABELS, DETAIL_COLUMNS, availableColumns, type DetailColumn } from "../lib/columns";
import type { DateFormat } from "../lib/settings";
import { isDirLike, typeLabel, type SortKey } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { useDragStore } from "../state/dragStore";
import { FileIcon } from "./FileIcon";

const SCROLLBAR_GUTTER = 10;
/** Empty space right of the last column that belongs to no row: dropping there targets the
 * folder on show rather than the row under the pointer. */
const DROP_STRIP_WIDTH = 44;
const TYPE_AHEAD_RESET_MILLIS = 800;

export interface FileListActions {
  onActivate: () => void;
  onContextMenu: (event: MouseEvent, entry: FileEntry | null) => void;
  onHeaderContextMenu: (event: MouseEvent) => void;
  onDelete: () => void;
  onRename: () => void;
  onNewFolder: () => void;
  onEditPath: () => void;
  onFocusFilter: () => void;
  onSwitchPane: (from: HTMLElement) => void;
  onRowPointerDown: (event: ReactPointerEvent<HTMLElement>, entry: FileEntry) => void;
  /** Double-click or Enter on a file. */
  onFileActivate: (entry: FileEntry) => void;
  onCut: () => void;
  onCopy: () => void;
  onPaste: () => void;
  onProperties: () => void;
}

interface Column {
  key: SortKey;
  label: string;
  /** Pixels; the name column uses this as its minimum and takes the remaining space. */
  width: number;
  align?: "end";
  render: (entry: FileEntry) => string;
}

const NAME_COLUMN: Column = {
  key: "name",
  label: "Name",
  width: 180,
  render: (entry) => entry.name,
};
const MODIFIED_WIDTHS: Record<DateFormat, number> = { minutes: 136, seconds: 160, locale: 180 };
/** The font size the column widths are made for; they grow and shrink with it. */
const BASE_FONT_SIZE = 13;

function detailColumn(key: DetailColumn, dateFormat: DateFormat): Column {
  switch (key) {
    case "size":
      return {
        key,
        label: COLUMN_LABELS.size,
        width: 88,
        align: "end",
        render: (entry) => (isDirLike(entry) ? "" : formatSize(entry.size)),
      };
    case "type":
      return { key, label: COLUMN_LABELS.type, width: 84, render: typeLabel };
    case "modified":
      return {
        key,
        label: COLUMN_LABELS.modified,
        width: MODIFIED_WIDTHS[dateFormat],
        render: (entry) => formatDate(entry.modified, dateFormat),
      };
    case "permissions":
      return { key, label: COLUMN_LABELS.permissions, width: 108, render: formatPermissions };
    case "owner":
      return {
        key,
        label: COLUMN_LABELS.owner,
        width: 120,
        render: (entry) => [entry.owner, entry.group].filter(Boolean).join(":"),
      };
  }
}

/** Columns kept longest when the list gets narrow come first. */
const FIT_PRIORITY: DetailColumn[] = ["size", "modified", "type", "permissions", "owner"];

interface FileListProps extends FileListActions {
  pane: PaneController;
  active: boolean;
  rowHeight: number;
  hiddenColumns: readonly DetailColumn[];
  dateFormat: DateFormat;
  striped: boolean;
  fontSize: number;
  /** While files are dragged over this list: the folder under the pointer, or "" for the
   * list itself. */
  dropFolder?: string;
  /** Entries cut to the clipboard, shown faded until they are pasted. */
  cutPaths?: ReadonlySet<string>;
}

export function FileList({
  pane,
  active,
  rowHeight,
  hiddenColumns,
  dateFormat,
  striped,
  fontSize,
  dropFolder,
  cutPaths,
  ...actions
}: FileListProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const typeAhead = useRef({ text: "", lastKeyAt: 0 });
  const entries = pane.visibleEntries;
  const showParentRow = pane.canGoUp && pane.filter === "";
  const rowOffset = showParentRow ? 1 : 0;
  const draggingFiles = useDragStore((state) => !!state.payload && state.payload.kind !== "tab");

  const detailColumns = useMemo(() => {
    const shown = availableColumns(pane.listing?.entries ?? []).filter(
      (key) => !hiddenColumns.includes(key),
    );
    return FIT_PRIORITY.filter((key) => shown.includes(key)).map((key) =>
      detailColumn(key, dateFormat),
    );
  }, [pane.listing, hiddenColumns, dateFormat]);
  const widthOf = useCallback(
    (column: Column) => Math.round((column.width * fontSize) / BASE_FONT_SIZE),
    [fontSize],
  );
  const widthNeeded = useMemo(() => {
    let total = widthOf(NAME_COLUMN) + SCROLLBAR_GUTTER + DROP_STRIP_WIDTH;
    return detailColumns.map((column) => (total += widthOf(column)));
  }, [detailColumns, widthOf]);
  const fittingColumns = useFitCount(rootRef, widthNeeded);
  const columns = useMemo(() => {
    const fitting = detailColumns.slice(0, fittingColumns);
    const inOrder = DETAIL_COLUMNS.flatMap((key) => fitting.filter((column) => column.key === key));
    return [NAME_COLUMN, ...inOrder];
  }, [detailColumns, fittingColumns]);
  const gridTemplateColumns = columns
    .map((column) =>
      column === NAME_COLUMN ? `minmax(${widthOf(column)}px, 1fr)` : `${widthOf(column)}px`,
    )
    .join(" ");

  const virtualizer = useVirtualizer({
    count: entries.length + rowOffset,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 16,
  });

  useEffect(() => {
    if (!pane.cursorPath) return;
    const index = entries.findIndex((entry) => entry.path === pane.cursorPath);
    if (index >= 0) virtualizer.scrollToIndex(index + rowOffset, { align: "auto" });
  }, [pane.cursorPath, entries, rowOffset, virtualizer]);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 });
  }, [pane.listing?.path]);

  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  useEffect(() => {
    scrollRef.current?.focus({ preventScroll: true });
  }, []);

  const pageSize = Math.max(
    1,
    Math.floor((scrollRef.current?.clientHeight ?? 400) / rowHeight) - 1,
  );
  const emptyMessage = pane.filter
    ? "Nothing matches the filter"
    : pane.hiddenCount > 0
      ? "Only hidden items here"
      : "This folder is empty";
  const cursorIndex = entries.findIndex((entry) => entry.path === pane.cursorPath);

  const jumpToTypedPrefix = (character: string) => {
    const now = Date.now();
    const state = typeAhead.current;
    state.text =
      now - state.lastKeyAt > TYPE_AHEAD_RESET_MILLIS ? character : state.text + character;
    state.lastKeyAt = now;
    const prefix = state.text.toLowerCase();
    const match = entries.findIndex((entry) => entry.name.toLowerCase().startsWith(prefix));
    if (match >= 0) pane.moveCursorTo(match, false);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const primary = event.ctrlKey || event.metaKey;
    const extend = event.shiftKey;
    const cursorEntry = cursorIndex >= 0 ? entries[cursorIndex] : undefined;
    const handled = (() => {
      switch (event.key) {
        case "ArrowDown":
          pane.moveCursor(1, extend);
          return true;
        case "ArrowUp":
          if (event.altKey) pane.goUp();
          else pane.moveCursor(-1, extend);
          return true;
        case "ArrowLeft":
          if (!event.altKey) return false;
          pane.goBack();
          return true;
        case "ArrowRight":
          if (!event.altKey) return false;
          pane.goForward();
          return true;
        case "PageDown":
          pane.moveCursor(pageSize, extend);
          return true;
        case "PageUp":
          pane.moveCursor(-pageSize, extend);
          return true;
        case "Home":
          pane.moveCursorTo(0, extend);
          return true;
        case "End":
          pane.moveCursorTo(entries.length - 1, extend);
          return true;
        case "Enter":
          if (event.altKey) {
            if (pane.selection.size > 0) actions.onProperties();
          } else if (cursorEntry) {
            openEntry(cursorEntry);
          }
          return true;
        case "Backspace":
          pane.goUp();
          return true;
        case "Delete":
          if (pane.selection.size > 0) actions.onDelete();
          return true;
        case "F2":
          if (pane.selection.size === 1) actions.onRename();
          return true;
        case "F5":
          void pane.refresh();
          return true;
        case "F7":
          actions.onNewFolder();
          return true;
        case "Escape":
          pane.clearSelection();
          return true;
        case "Tab":
          if (primary || event.altKey) return false;
          actions.onSwitchPane(event.currentTarget);
          return true;
      }
      if (primary && !event.shiftKey && !event.altKey) {
        const clipboardAction = { x: actions.onCut, c: actions.onCopy, v: actions.onPaste }[
          event.key.toLowerCase()
        ];
        if (clipboardAction) {
          if (event.key.toLowerCase() === "v" || pane.selection.size > 0) clipboardAction();
          return true;
        }
      }
      if (primary && event.key.toLowerCase() === "a") {
        pane.selectAll();
        return true;
      }
      if (primary && event.key.toLowerCase() === "r") {
        void pane.refresh();
        return true;
      }
      if (primary && event.key.toLowerCase() === "l") {
        actions.onEditPath();
        return true;
      }
      if (primary && event.key.toLowerCase() === "f") {
        actions.onFocusFilter();
        return true;
      }
      if (primary && event.shiftKey && event.key.toLowerCase() === "n") {
        actions.onNewFolder();
        return true;
      }
      if (!primary && !event.altKey && event.key.length === 1 && event.key !== " ") {
        jumpToTypedPrefix(event.key);
        return true;
      }
      return false;
    })();
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  };

  const openEntry = (entry: FileEntry) => {
    if (isDirLike(entry)) pane.open(entry);
    else actions.onFileActivate(entry);
  };

  // Pressing an unselected row selects it so a drag carries it; pressing a selected row keeps
  // the selection until release, so several rows can be dragged.
  const handleRowPointerDown = (event: ReactPointerEvent<HTMLElement>, entry: FileEntry) => {
    if (event.button !== 0) return;
    const modified = event.ctrlKey || event.metaKey || event.shiftKey;
    if (!modified && !pane.selection.has(entry.path)) pane.selectOnly(entry.path);
    if (!modified) actions.onRowPointerDown(event, entry);
  };

  const handleRowClick = (event: MouseEvent, entry: FileEntry) => {
    if (event.ctrlKey || event.metaKey) pane.toggleSelected(entry.path);
    else if (event.shiftKey) pane.selectRangeTo(entry.path);
    else pane.selectOnly(entry.path);
  };

  const handleRowContextMenu = (event: MouseEvent, entry: FileEntry) => {
    event.preventDefault();
    event.stopPropagation();
    if (!pane.selection.has(entry.path)) pane.selectOnly(entry.path);
    actions.onContextMenu(event, entry);
  };

  return (
    <div
      ref={rootRef}
      className={[
        "file-list",
        active && "is-active",
        draggingFiles && "is-dragging-files",
        striped && "is-striped",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{ "--drop-strip-width": `${DROP_STRIP_WIDTH}px` } as CSSProperties}
    >
      <div
        className="file-list-header"
        style={{ gridTemplateColumns: `${gridTemplateColumns} ${DROP_STRIP_WIDTH}px` }}
        role="row"
        onContextMenu={(event) => {
          event.preventDefault();
          actions.onHeaderContextMenu(event);
        }}
      >
        {columns.map((column) => {
          const sorted = pane.sort.key === column.key;
          const SortIcon = pane.sort.direction === 1 ? ArrowUp : ArrowDown;
          return (
            <button
              key={column.key}
              type="button"
              role="columnheader"
              className={`file-list-heading ${column.align === "end" ? "align-end" : ""}`}
              aria-sort={sorted ? (pane.sort.direction === 1 ? "ascending" : "descending") : "none"}
              onClick={() => pane.setSortKey(column.key)}
            >
              <span>{column.label}</span>
              {sorted && <SortIcon size={12} />}
            </button>
          );
        })}
      </div>
      <div
        ref={scrollRef}
        className="file-list-body"
        tabIndex={0}
        role="grid"
        aria-multiselectable
        onFocus={actions.onActivate}
        onMouseDown={actions.onActivate}
        onKeyDown={handleKeyDown}
        onClick={(event) => {
          if (!(event.target as Element).closest(".file-row")) pane.clearSelection();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          pane.clearSelection();
          actions.onContextMenu(event, null);
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const style = {
              transform: `translateY(${item.start}px)`,
              width: `calc(100% - ${DROP_STRIP_WIDTH}px)`,
              height: rowHeight,
              gridTemplateColumns,
            };
            if (showParentRow && item.index === 0) {
              const parent = pane.listing?.parent ?? undefined;
              return (
                <div
                  key="parent"
                  className={`file-row file-row-parent ${dropFolder && dropFolder === parent ? "is-drop-folder" : ""}`}
                  style={style}
                  role="row"
                  data-drop-folder={parent}
                  onDoubleClick={pane.goUp}
                  onClick={pane.clearSelection}
                  onContextMenu={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    pane.clearSelection();
                    actions.onContextMenu(event, null);
                  }}
                >
                  <span className="file-cell file-name">
                    <CornerLeftUp size={16} className="file-icon file-icon-folder" aria-hidden />
                    ..
                  </span>
                </div>
              );
            }
            const entry = entries[item.index - rowOffset];
            const selected = pane.selection.has(entry.path);
            const isCursor = pane.cursorPath === entry.path;
            const folder = isDirLike(entry);
            return (
              <div
                key={entry.path}
                className={[
                  "file-row",
                  item.index % 2 === 1 && "is-odd",
                  selected && "is-selected",
                  isCursor && "is-cursor",
                  entry.hidden && "is-hidden",
                  cutPaths?.has(entry.path) && "is-cut",
                  folder && dropFolder === entry.path && "is-drop-folder",
                ]
                  .filter(Boolean)
                  .join(" ")}
                style={style}
                role="row"
                aria-selected={selected}
                title={entry.name}
                data-drop-folder={folder ? entry.path : undefined}
                onPointerDown={(event) => handleRowPointerDown(event, entry)}
                onClick={(event) => handleRowClick(event, entry)}
                onDoubleClick={() => openEntry(entry)}
                onContextMenu={(event) => handleRowContextMenu(event, entry)}
              >
                {columns.map((column) =>
                  column.key === "name" ? (
                    <span key="name" className="file-cell file-name" role="gridcell">
                      <FileIcon entry={entry} />
                      <span className="file-name-text">{entry.name}</span>
                    </span>
                  ) : (
                    <span
                      key={column.key}
                      role="gridcell"
                      className={`file-cell file-${column.key} ${column.align === "end" ? "align-end" : ""}`}
                    >
                      {column.render(entry)}
                    </span>
                  ),
                )}
              </div>
            );
          })}
        </div>
        {entries.length === 0 && <p className="file-list-empty">{emptyMessage}</p>}
      </div>
    </div>
  );
}
