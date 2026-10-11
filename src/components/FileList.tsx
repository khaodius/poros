import {
  useCallback,
  useEffect,
  useLayoutEffect,
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
import { useSelectionBox, type RowRange } from "../hooks/useSelectionBox";
import { formatDate, formatPermissions, formatSize } from "../lib/format";
import { COLUMN_LABELS, DETAIL_COLUMNS, availableColumns, type DetailColumn } from "../lib/columns";
import type { DateFormat } from "../lib/settings";
import { isDirLike, typeLabel, type SortKey } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { useDragStore } from "../state/dragStore";
import { FileIcon } from "./FileIcon";

/** Matches `--scrollbar-size`: the list always shows its vertical scrollbar. */
const SCROLLBAR_GUTTER = 8;
/** Empty space right of the last column that belongs to no row: dropping there targets the
 * folder on show rather than the row under the pointer. */
const DROP_STRIP_WIDTH = 44;
const TYPE_AHEAD_RESET_MILLIS = 800;

/** Where a menu opens, in viewport pixels. */
export interface MenuPosition {
  x: number;
  y: number;
}

export interface FileListActions {
  onActivate: () => void;
  onContextMenu: (position: MenuPosition, entry: FileEntry | null) => void;
  onHeaderContextMenu: (event: MouseEvent) => void;
  onDelete: () => void;
  onRename: () => void;
  /** F4 on a row: opens the file in the editor. */
  onEdit: (entry: FileEntry) => void;
  onNewFolder: () => void;
  onNewFile: () => void;
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
  const contentRef = useRef<HTMLDivElement>(null);
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

  // A folder opens at the top, or with the cursor's row in the middle when it has one, as the
  // folder just left does after going up or back.
  const scrolledPath = useRef<string | undefined>(undefined);
  useEffect(() => {
    const opened = scrolledPath.current !== pane.listing?.path;
    scrolledPath.current = pane.listing?.path;
    const index = pane.cursorPath
      ? entries.findIndex((entry) => entry.path === pane.cursorPath)
      : -1;
    if (index >= 0) {
      virtualizer.scrollToIndex(index + rowOffset, { align: opened ? "center" : "auto" });
    } else if (opened) {
      scrollRef.current?.scrollTo({ top: 0 });
    }
  }, [pane.listing?.path, pane.cursorPath, entries, rowOffset, virtualizer]);

  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);

  const selectionBox = useSelectionBox(scrollRef, contentRef, {
    rowHeight,
    rowCount: entries.length + rowOffset,
    rowsInset: DROP_STRIP_WIDTH,
  });
  // Read as a box changes, so the rows it selects are those on show even if the folder reloads
  // while it is drawn.
  const shownRows = useRef({ entries, rowOffset });
  useLayoutEffect(() => {
    shownRows.current = { entries, rowOffset };
  });

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
  const cursorEntry = cursorIndex >= 0 ? entries[cursorIndex] : undefined;

  const typingAhead = () =>
    typeAhead.current.text !== "" &&
    Date.now() - typeAhead.current.lastKeyAt <= TYPE_AHEAD_RESET_MILLIS;

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
    const handled = (() => {
      switch (event.key) {
        case "ArrowDown":
          if (primary) pane.stepCursor(1);
          else pane.moveCursor(1, extend);
          return true;
        case "ArrowUp":
          if (event.altKey) pane.goUp();
          else if (primary) pane.stepCursor(-1);
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
          // Ctrl+PageDown and Ctrl+PageUp switch tabs.
          if (primary) return false;
          pane.moveCursor(pageSize, extend);
          return true;
        case "PageUp":
          if (primary) return false;
          pane.moveCursor(-pageSize, extend);
          return true;
        case "Home":
          if (event.altKey) pane.goHome();
          else pane.moveCursorTo(0, extend);
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
        case "F4":
          if (event.shiftKey) actions.onNewFile();
          else if (cursorEntry) actions.onEdit(cursorEntry);
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
        case " ":
          if (typingAhead()) jumpToTypedPrefix(" ");
          else if (cursorEntry) pane.toggleSelected(cursorEntry.path);
          return true;
        case "Insert":
          if (cursorEntry) {
            pane.toggleSelected(cursorEntry.path);
            pane.stepCursor(1);
          }
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
      if (primary && event.key.toLowerCase() === "h") {
        pane.toggleHidden();
        return true;
      }
      if (primary && event.shiftKey && event.key.toLowerCase() === "n") {
        actions.onNewFolder();
        return true;
      }
      if (!primary && !event.altKey && event.key.length === 1) {
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

  // A box drawn from empty space, or from a row away from its name, selects the rows it
  // touches; with Ctrl or Shift held it adds them to the selection.
  const startBox = (event: ReactPointerEvent<HTMLElement>) => {
    const before = pane.selection;
    const kept = event.ctrlKey || event.metaKey || event.shiftKey ? [...before] : [];
    const pathsIn = (rows: RowRange | null) => {
      if (!rows) return [];
      const { entries: shown, rowOffset: offset } = shownRows.current;
      const first = Math.max(rows.first - offset, 0);
      return shown.slice(first, rows.last - offset + 1).map((entry) => entry.path);
    };
    selectionBox.begin(event, {
      onRows: (rows) => pane.selectPaths(new Set([...kept, ...pathsIn(rows)])),
      onFinish: (rows, downward) => {
        const paths = pathsIn(rows);
        if (paths.length === 0) return;
        const [top, bottom] = [paths[0], paths[paths.length - 1]];
        pane.selectPaths(
          new Set([...kept, ...paths]),
          downward ? top : bottom,
          downward ? bottom : top,
        );
      },
      onCancel: () => pane.selectPaths(before),
    });
  };

  // Pressing an unselected row's name selects it so a drag carries it; pressing a selected row
  // keeps the selection until release, so several rows can be dragged. Pressing an unselected
  // row away from its name draws a box instead, and selects the row on release if it does not.
  const handleRowPointerDown = (event: ReactPointerEvent<HTMLElement>, entry: FileEntry) => {
    if (event.button !== 0) return;
    const selected = pane.selection.has(entry.path);
    const onName = (event.target as Element).closest(".file-name-text, .file-icon");
    if (!selected && !onName && event.pointerType !== "touch") {
      startBox(event);
      return;
    }
    const modified = event.ctrlKey || event.metaKey || event.shiftKey;
    if (!modified && !selected) pane.selectOnly(entry.path);
    if (!modified) actions.onRowPointerDown(event, entry);
  };

  const handleBodyPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    // Rows, the parent row aside, decide for themselves; the scrollbar is right of the content.
    const list = event.currentTarget;
    if ((event.target as Element).closest(".file-row:not(.file-row-parent)")) return;
    if (event.clientX >= list.getBoundingClientRect().left + list.clientWidth) return;
    startBox(event);
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
    actions.onContextMenu({ x: event.clientX, y: event.clientY }, entry);
  };

  /** The Menu key or Shift+F10: the menu for the cursor's row, or the selection, beside it. */
  const showKeyboardMenu = () => {
    const list = scrollRef.current;
    if (!list) return;
    const row =
      list.querySelector(".file-row.is-cursor") ?? list.querySelector(".file-row.is-selected");
    const bounds = (row ?? list).getBoundingClientRect();
    const position = { x: bounds.left + 24, y: row ? bounds.bottom : bounds.top + 8 };
    const target = cursorEntry ?? entries.find((entry) => pane.selection.has(entry.path));
    if (target && !pane.selection.has(target.path)) pane.selectOnly(target.path);
    actions.onContextMenu(position, target ?? null);
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
        onPointerDown={handleBodyPointerDown}
        onKeyDown={handleKeyDown}
        onClick={(event) => {
          if (!(event.target as Element).closest(".file-row")) pane.clearSelection();
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          // A right click reports its button; a menu opened from the keyboard reports none.
          if (event.button !== 2 && !event.ctrlKey) {
            showKeyboardMenu();
            return;
          }
          pane.clearSelection();
          actions.onContextMenu({ x: event.clientX, y: event.clientY }, null);
        }}
      >
        <div ref={contentRef} style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
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
                    actions.onContextMenu({ x: event.clientX, y: event.clientY }, null);
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
          {selectionBox.box && (
            <div
              className="selection-box"
              style={{
                left: selectionBox.box.left,
                top: selectionBox.box.top,
                width: selectionBox.box.width,
                height: selectionBox.box.height,
              }}
              aria-hidden
            />
          )}
        </div>
        {entries.length === 0 && <p className="file-list-empty">{emptyMessage}</p>}
      </div>
    </div>
  );
}
