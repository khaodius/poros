import { useEffect, useMemo, useRef, type KeyboardEvent, type MouseEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, CornerLeftUp } from "lucide-react";
import { useElementWidth } from "../hooks/useElementWidth";
import type { PaneController } from "../hooks/usePane";
import { formatDate, formatPermissions, formatSize } from "../lib/format";
import { isDirLike, type SortKey } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { FileIcon } from "./FileIcon";

const ROW_HEIGHT = 24;
const SCROLLBAR_GUTTER = 10;
const TYPE_AHEAD_RESET_MILLIS = 800;

export interface FileListActions {
  onActivate: () => void;
  onContextMenu: (event: MouseEvent, entry: FileEntry | null) => void;
  onDelete: () => void;
  onRename: () => void;
  onNewFolder: () => void;
  onEditPath: () => void;
  onFocusFilter: () => void;
  onSwitchPane: () => void;
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
const SIZE_COLUMN: Column = {
  key: "size",
  label: "Size",
  width: 88,
  align: "end",
  render: (entry) => (isDirLike(entry) ? "" : formatSize(entry.size)),
};
const MODIFIED_COLUMN: Column = {
  key: "modified",
  label: "Modified",
  width: 136,
  render: (entry) => formatDate(entry.modified),
};
const PERMISSIONS_COLUMN: Column = {
  key: "permissions",
  label: "Permissions",
  width: 108,
  render: formatPermissions,
};
const OWNER_COLUMN: Column = {
  key: "owner",
  label: "Owner",
  width: 120,
  render: (entry) => [entry.owner, entry.group].filter(Boolean).join(":"),
};

interface FileListProps extends FileListActions {
  pane: PaneController;
  active: boolean;
}

export function FileList({ pane, active, ...actions }: FileListProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const listWidth = useElementWidth(rootRef);
  const typeAhead = useRef({ text: "", lastKeyAt: 0 });
  const entries = pane.visibleEntries;
  const showParentRow = pane.canGoUp && pane.filter === "";
  const rowOffset = showParentRow ? 1 : 0;

  const columns = useMemo(() => {
    const all = pane.listing?.entries ?? [];
    const detailColumns = [
      SIZE_COLUMN,
      MODIFIED_COLUMN,
      ...(all.some((entry) => entry.permissions !== null) ? [PERMISSIONS_COLUMN] : []),
      ...(all.some((entry) => entry.owner !== null) ? [OWNER_COLUMN] : []),
    ];
    const visible = [NAME_COLUMN];
    let remaining = listWidth - SCROLLBAR_GUTTER - NAME_COLUMN.width;
    for (const column of detailColumns) {
      if (column.width > remaining) break;
      visible.push(column);
      remaining -= column.width;
    }
    return visible;
  }, [pane.listing, listWidth]);
  const gridTemplateColumns = columns
    .map((column) =>
      column === NAME_COLUMN ? `minmax(${column.width}px, 1fr)` : `${column.width}px`,
    )
    .join(" ");

  const virtualizer = useVirtualizer({
    count: entries.length + rowOffset,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
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
    scrollRef.current?.focus({ preventScroll: true });
  }, []);

  const pageSize = Math.max(
    1,
    Math.floor((scrollRef.current?.clientHeight ?? 400) / ROW_HEIGHT) - 1,
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
          if (cursorEntry) pane.open(cursorEntry);
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
          actions.onSwitchPane();
          return true;
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
    <div ref={rootRef} className={`file-list ${active ? "is-active" : ""}`}>
      <div className="file-list-header" style={{ gridTemplateColumns }} role="row">
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
          if (event.target === event.currentTarget) pane.clearSelection();
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
              height: ROW_HEIGHT,
              gridTemplateColumns,
            };
            if (showParentRow && item.index === 0) {
              return (
                <div
                  key="parent"
                  className="file-row file-row-parent"
                  style={style}
                  role="row"
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
            return (
              <div
                key={entry.path}
                className={[
                  "file-row",
                  selected && "is-selected",
                  isCursor && "is-cursor",
                  entry.hidden && "is-hidden",
                ]
                  .filter(Boolean)
                  .join(" ")}
                style={style}
                role="row"
                aria-selected={selected}
                title={entry.name}
                onClick={(event) => handleRowClick(event, entry)}
                onDoubleClick={() => pane.open(entry)}
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
