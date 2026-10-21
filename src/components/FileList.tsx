import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type SyntheticEvent,
} from "react";
import { defaultRangeExtractor, useVirtualizer, type Range } from "@tanstack/react-virtual";
import { ArrowDown, ArrowUp, CornerLeftUp } from "lucide-react";
import { useFitCount } from "../hooks/useFitCount";
import type { PaneController } from "../hooks/usePane";
import { useSelectionBox, type RowRange } from "../hooks/useSelectionBox";
import { formatDate, formatPermissions, formatSize } from "../lib/format";
import {
  COLUMN_LABELS,
  COLUMN_WIDTH_LIMITS,
  DETAIL_COLUMNS,
  availableColumns,
  type ColumnWidths,
  type DetailColumn,
} from "../lib/columns";
import { stemLength } from "../lib/path";
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
/**
 * How long a second, slower click on the only selected row's name waits before renaming it, as
 * in Windows Explorer, so that a click that turns out to start a double click can still open it.
 */
const SLOW_CLICK_RENAME_MILLIS = 500;
/** A press that moves further than this before release was a drag, not a click. */
const CLICK_SLOP = 4;

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
  /** Shows the name field in the entry's row. */
  onRename: (entry: FileEntry) => void;
  /** Renames the entry; false when it could not be, which the pane has said why. */
  onRenameSubmit: (entry: FileEntry, newName: string) => Promise<boolean>;
  onRenameCancel: () => void;
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
/** The sort arrow beside a heading, and the gap before it. */
const SORT_ICON_ROOM = 16;

function detailColumn(key: DetailColumn, dateFormat: DateFormat, widths: ColumnWidths): Column {
  const column = defaultColumn(key, dateFormat);
  return { ...column, width: widths[key] ?? column.width };
}

function defaultColumn(key: DetailColumn, dateFormat: DateFormat): Column {
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

interface RowEvents {
  onPointerDown: (event: ReactPointerEvent<HTMLElement>, entry: FileEntry) => void;
  onClick: (event: MouseEvent, entry: FileEntry) => void;
  onOpen: (entry: FileEntry) => void;
  onContextMenu: (event: MouseEvent, entry: FileEntry) => void;
  onRenameSubmit: (entry: FileEntry, newName: string, fromKeyboard: boolean) => Promise<boolean>;
  onRenameCancel: (fromKeyboard: boolean) => void;
}

interface FileRowProps {
  entry: FileEntry;
  columns: Column[];
  gridTemplateColumns: string;
  top: number;
  height: number;
  odd: boolean;
  selected: boolean;
  cursor: boolean;
  cut: boolean;
  /** Files dragged over the list would drop into this folder. */
  dropTarget: boolean;
  /** The row shows a field to type a new name in. */
  renaming: boolean;
  events: RowEvents;
}

/**
 * A file's row. Rows render again only when what they show changes, so selecting rows, as a
 * box drawn across the list does on every row it reaches, touches only those rows.
 */
const FileRow = memo(function FileRow({
  entry,
  columns,
  gridTemplateColumns,
  top,
  height,
  odd,
  selected,
  cursor,
  cut,
  dropTarget,
  renaming,
  events,
}: FileRowProps) {
  const folder = isDirLike(entry);
  return (
    <div
      className={[
        "file-row",
        odd && "is-odd",
        selected && "is-selected",
        cursor && "is-cursor",
        entry.hidden && "is-hidden",
        cut && "is-cut",
        dropTarget && "is-drop-folder",
      ]
        .filter(Boolean)
        .join(" ")}
      style={{
        transform: `translateY(${top}px)`,
        width: `calc(100% - ${DROP_STRIP_WIDTH}px)`,
        height,
        gridTemplateColumns,
      }}
      role="row"
      aria-selected={selected}
      title={entry.name}
      data-drop-folder={folder ? entry.path : undefined}
      onPointerDown={(event) => events.onPointerDown(event, entry)}
      onClick={(event) => events.onClick(event, entry)}
      onDoubleClick={() => events.onOpen(entry)}
      onContextMenu={(event) => events.onContextMenu(event, entry)}
    >
      {columns.map((column) =>
        column.key === "name" ? (
          <span key="name" className="file-cell file-name" role="gridcell">
            <FileIcon entry={entry} />
            {renaming ? (
              <RenameField entry={entry} events={events} />
            ) : (
              <span className="file-name-text">{entry.name}</span>
            )}
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
});

/** Keeps events in the field from reaching the row and the list, which would act on them. */
const stopAtField = (event: SyntheticEvent) => event.stopPropagation();

/**
 * The new name, typed in place of the old one in the row. Enter or leaving the field renames,
 * Esc keeps the old name. The name before the extension is selected to start with.
 */
function RenameField({ entry, events }: { entry: FileEntry; events: RowEvents }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [name, setName] = useState(entry.name);
  const [busy, setBusy] = useState(false);
  const finished = useRef(false);

  useLayoutEffect(() => {
    const input = inputRef.current;
    if (!input) return;
    input.focus({ preventScroll: true });
    input.setSelectionRange(0, stemLength(entry.name, isDirLike(entry)));
  }, [entry]);

  const submit = async (fromKeyboard: boolean) => {
    if (finished.current || busy) return;
    if (name === "" || name === entry.name) {
      finished.current = true;
      events.onRenameCancel(fromKeyboard);
      return;
    }
    setBusy(true);
    const renamed = await events.onRenameSubmit(entry, name, fromKeyboard);
    if (renamed) {
      finished.current = true;
      return;
    }
    setBusy(false);
    // Typed again after the reason shows; leaving the field gives up instead.
    if (fromKeyboard) {
      inputRef.current?.focus({ preventScroll: true });
    } else {
      finished.current = true;
      events.onRenameCancel(false);
    }
  };

  return (
    <input
      ref={inputRef}
      className="rename-field"
      value={name}
      readOnly={busy}
      spellCheck={false}
      aria-label={`New name for ${entry.name}`}
      onChange={(event) => setName(event.target.value)}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Enter") {
          event.preventDefault();
          void submit(true);
        } else if (event.key === "Escape") {
          event.preventDefault();
          finished.current = true;
          events.onRenameCancel(true);
        }
      }}
      onBlur={() => void submit(false)}
      onPointerDown={stopAtField}
      onMouseDown={stopAtField}
      onClick={stopAtField}
      onDoubleClick={stopAtField}
      onContextMenu={stopAtField}
    />
  );
}

interface FileListProps extends FileListActions {
  pane: PaneController;
  active: boolean;
  rowHeight: number;
  hiddenColumns: readonly DetailColumn[];
  columnWidths: ColumnWidths;
  /** While a heading's edge is dragged, then with `done` on release. */
  onColumnWidthsChange: (widths: ColumnWidths, done: boolean) => void;
  dateFormat: DateFormat;
  striped: boolean;
  fontSize: number;
  /** While files are dragged over this list: the folder under the pointer, or "" for the
   * list itself. */
  dropFolder?: string;
  /** Entries cut to the clipboard, shown faded until they are pasted. */
  cutPaths?: ReadonlySet<string>;
  /** The entry whose row shows the name field. */
  renaming?: string | null;
}

export function FileList({
  pane,
  active,
  rowHeight,
  hiddenColumns,
  columnWidths,
  onColumnWidthsChange,
  dateFormat,
  striped,
  fontSize,
  dropFolder,
  cutPaths,
  renaming,
  ...actions
}: FileListProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);
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
      detailColumn(key, dateFormat, columnWidths),
    );
  }, [pane.listing, hiddenColumns, dateFormat, columnWidths]);
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

  // The row being renamed stays rendered when scrolled away, so the name typed so far is kept.
  const renamingIndex = renaming ? entries.findIndex((entry) => entry.path === renaming) : -1;
  const renamingRow = renamingIndex >= 0 ? renamingIndex + rowOffset : -1;
  const rangeExtractor = useCallback(
    (range: Range) => {
      const rows = defaultRangeExtractor(range);
      if (renamingRow < 0 || rows.includes(renamingRow)) return rows;
      return [...rows, renamingRow].sort((first, second) => first - second);
    },
    [renamingRow],
  );

  const virtualizer = useVirtualizer({
    count: entries.length + rowOffset,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 16,
    rangeExtractor,
  });

  useEffect(() => {
    if (renamingRow >= 0) virtualizer.scrollToIndex(renamingRow, { align: "auto" });
  }, [renamingRow, virtualizer]);

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

  // A slow second click on the only selected row's name renames it after a moment, unless
  // anything else happens first.
  const press = useRef<{ path: string; x: number; y: number; renames: boolean } | null>(null);
  const renameTimer = useRef(0);
  const latestPane = useRef(pane);
  useLayoutEffect(() => {
    latestPane.current = pane;
  });
  const cancelSlowRename = useCallback(() => {
    window.clearTimeout(renameTimer.current);
    renameTimer.current = 0;
  }, []);
  useEffect(() => {
    window.addEventListener("blur", cancelSlowRename);
    return () => {
      window.removeEventListener("blur", cancelSlowRename);
      cancelSlowRename();
    };
  }, [cancelSlowRename]);
  useEffect(cancelSlowRename, [pane.listing?.path, cancelSlowRename]);

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
    cancelSlowRename();
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
        case "F2": {
          const [only] = pane.selectedEntries();
          if (pane.selection.size === 1 && only) actions.onRename(only);
          return true;
        }
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

  const toBaseWidth = (shown: number) => Math.round((shown * BASE_FONT_SIZE) / fontSize);

  /**
   * The narrowest and widest `column` can be, in pixels on screen: at most as much wider as the
   * name column, which takes the room left, can give.
   */
  const widthLimits = (column: Column): [number, number] => {
    const nameHeading = headerRef.current?.querySelector('[data-column="name"]');
    const room = nameHeading ? nameHeading.getBoundingClientRect().width - widthOf(NAME_COLUMN) : 0;
    const scale = fontSize / BASE_FONT_SIZE;
    return [
      COLUMN_WIDTH_LIMITS.min * scale,
      Math.min(widthOf(column) + Math.max(0, room), COLUMN_WIDTH_LIMITS.max * scale),
    ];
  };

  const widthsWith = (column: Column, shown: number, [narrowest, widest]: [number, number]) => ({
    ...columnWidths,
    [column.key]: toBaseWidth(Math.min(widest, Math.max(narrowest, shown))),
  });

  // The line left of a heading resizes that column. The name column gives or takes the
  // difference, so the line follows the pointer.
  const startColumnResize = (event: ReactPointerEvent<HTMLElement>, column: Column) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const origin = event.clientX;
    const startWidth = widthOf(column);
    const limits = widthLimits(column);
    let widths: ColumnWidths | null = null;
    let frame = 0;

    const move = (moved: PointerEvent) => {
      // Released where this window could not see it: the column keeps the width it has.
      if (!(moved.buttons & 1)) {
        stop();
        return;
      }
      widths = widthsWith(column, startWidth - (moved.clientX - origin), limits);
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (widths) onColumnWidthsChange(widths, false);
      });
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      window.removeEventListener("blur", stop);
      if (widths) onColumnWidthsChange(widths, true);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
    // Another program took the pointer, such as a screenshot tool, and gets the release.
    window.addEventListener("blur", stop);
  };

  /** Widens or narrows `column` to show its heading and every value in it in full. */
  const fitColumn = (column: Column) => {
    const heading = headerRef.current?.querySelector(
      `[data-column="${column.key}"] > .file-list-heading`,
    );
    const context = document.createElement("canvas").getContext("2d");
    if (!heading || !context) return;
    const fontOf = (style: CSSStyleDeclaration) =>
      `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    const paddingOf = (style: CSSStyleDeclaration) =>
      parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    const headingStyle = getComputedStyle(heading);
    // Headings are drawn in capitals, with the sort arrow beside them.
    const label = column.label.toUpperCase();
    context.font = fontOf(headingStyle);
    let widest =
      context.measureText(label).width +
      (parseFloat(headingStyle.letterSpacing) || 0) * label.length +
      paddingOf(headingStyle) +
      SORT_ICON_ROOM;
    const cell = rootRef.current?.querySelector(`.file-cell.file-${column.key}`);
    if (cell) {
      const cellStyle = getComputedStyle(cell);
      context.font = fontOf(cellStyle);
      const padding = paddingOf(cellStyle);
      for (const entry of entries) {
        widest = Math.max(widest, context.measureText(column.render(entry)).width + padding);
      }
    }
    onColumnWidthsChange(widthsWith(column, Math.ceil(widest) + 1, widthLimits(column)), true);
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
    cancelSlowRename();
    if (event.button !== 0) return;
    const selected = pane.selection.has(entry.path);
    const onName = (event.target as Element).closest(".file-name-text, .file-icon");
    const modified = event.ctrlKey || event.metaKey || event.shiftKey;
    // As in Explorer, only once the list already has the focus, so the click that brings the
    // window or the pane forward never renames.
    press.current = {
      path: entry.path,
      x: event.clientX,
      y: event.clientY,
      renames:
        selected &&
        pane.selection.size === 1 &&
        !modified &&
        event.pointerType !== "touch" &&
        !!(event.target as Element).closest(".file-name-text") &&
        document.hasFocus() &&
        document.activeElement === scrollRef.current,
    };
    if (!selected && !onName && event.pointerType !== "touch") {
      startBox(event);
      return;
    }
    if (!modified && !selected) pane.selectOnly(entry.path);
    if (!modified) actions.onRowPointerDown(event, entry);
  };

  const handleBodyPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    cancelSlowRename();
    // Rows, the parent row aside, decide for themselves; the scrollbar is right of the content.
    const list = event.currentTarget;
    if ((event.target as Element).closest(".file-row:not(.file-row-parent)")) return;
    if (event.clientX >= list.getBoundingClientRect().left + list.clientWidth) return;
    startBox(event);
  };

  /** Waits out a double click after a slow second click on the selected row, then renames. */
  const startSlowRename = (event: MouseEvent, entry: FileEntry) => {
    const pressed = press.current;
    press.current = null;
    const slipped =
      !pressed ||
      Math.abs(event.clientX - pressed.x) > CLICK_SLOP ||
      Math.abs(event.clientY - pressed.y) > CLICK_SLOP;
    if (!pressed?.renames || pressed.path !== entry.path || event.detail !== 1 || slipped) return;
    renameTimer.current = window.setTimeout(() => {
      renameTimer.current = 0;
      const { selection } = latestPane.current;
      const stillAlone = selection.size === 1 && selection.has(entry.path);
      const stillFocused = document.hasFocus() && document.activeElement === scrollRef.current;
      if (stillAlone && stillFocused && !useDragStore.getState().payload) actions.onRename(entry);
    }, SLOW_CLICK_RENAME_MILLIS);
  };

  const handleRowClick = (event: MouseEvent, entry: FileEntry) => {
    cancelSlowRename();
    startSlowRename(event, entry);
    const primary = event.ctrlKey || event.metaKey;
    if (primary && event.shiftKey) pane.addRangeTo(entry.path);
    else if (primary) pane.toggleSelected(entry.path);
    else if (event.shiftKey) pane.selectRangeTo(entry.path);
    else pane.selectOnly(entry.path);
  };

  const handleRowContextMenu = (event: MouseEvent, entry: FileEntry) => {
    cancelSlowRename();
    event.preventDefault();
    event.stopPropagation();
    if (!pane.selection.has(entry.path)) pane.selectOnly(entry.path);
    actions.onContextMenu({ x: event.clientX, y: event.clientY }, entry);
  };

  // Rows get handlers that stay the same from render to render and call the latest ones, so a
  // row renders again only when it shows something new.
  const latestRowEvents = useRef<RowEvents | null>(null);
  useLayoutEffect(() => {
    latestRowEvents.current = {
      onPointerDown: handleRowPointerDown,
      onClick: handleRowClick,
      onOpen: (entry) => {
        cancelSlowRename();
        openEntry(entry);
      },
      onContextMenu: handleRowContextMenu,
      onRenameSubmit: async (entry, newName, fromKeyboard) => {
        const renamed = await actions.onRenameSubmit(entry, newName);
        if (renamed && fromKeyboard) scrollRef.current?.focus({ preventScroll: true });
        return renamed;
      },
      onRenameCancel: (fromKeyboard) => {
        actions.onRenameCancel();
        if (fromKeyboard) scrollRef.current?.focus({ preventScroll: true });
      },
    };
  });
  const rowEvents = useMemo<RowEvents>(
    () => ({
      onPointerDown: (event, entry) => latestRowEvents.current?.onPointerDown(event, entry),
      onClick: (event, entry) => latestRowEvents.current?.onClick(event, entry),
      onOpen: (entry) => latestRowEvents.current?.onOpen(entry),
      onContextMenu: (event, entry) => latestRowEvents.current?.onContextMenu(event, entry),
      onRenameSubmit: (entry, newName, fromKeyboard) =>
        latestRowEvents.current?.onRenameSubmit(entry, newName, fromKeyboard) ??
        Promise.resolve(false),
      onRenameCancel: (fromKeyboard) => latestRowEvents.current?.onRenameCancel(fromKeyboard),
    }),
    [],
  );

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
        ref={headerRef}
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
            <div
              key={column.key}
              className="file-list-column"
              role="columnheader"
              aria-sort={sorted ? (pane.sort.direction === 1 ? "ascending" : "descending") : "none"}
              data-column={column.key}
            >
              <button
                type="button"
                className={`file-list-heading ${column.align === "end" ? "align-end" : ""}`}
                onClick={() => pane.setSortKey(column.key)}
              >
                <span>{column.label}</span>
                {sorted && <SortIcon size={12} />}
              </button>
              {column !== NAME_COLUMN && (
                <div
                  className="column-resizer"
                  role="separator"
                  aria-orientation="vertical"
                  aria-label={`Resize ${column.label}`}
                  title="Drag to resize, double-click to fit"
                  onPointerDown={(event) => startColumnResize(event, column)}
                  onDoubleClick={() => fitColumn(column)}
                />
              )}
            </div>
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
            if (showParentRow && item.index === 0) {
              const parent = pane.listing?.parent ?? undefined;
              return (
                <div
                  key="parent"
                  className={`file-row file-row-parent ${dropFolder && dropFolder === parent ? "is-drop-folder" : ""}`}
                  style={{
                    transform: `translateY(${item.start}px)`,
                    width: `calc(100% - ${DROP_STRIP_WIDTH}px)`,
                    height: rowHeight,
                    gridTemplateColumns,
                  }}
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
            return (
              <FileRow
                key={entry.path}
                entry={entry}
                columns={columns}
                gridTemplateColumns={gridTemplateColumns}
                top={item.start}
                height={rowHeight}
                odd={item.index % 2 === 1}
                selected={pane.selection.has(entry.path)}
                cursor={pane.cursorPath === entry.path}
                cut={cutPaths?.has(entry.path) ?? false}
                dropTarget={isDirLike(entry) && dropFolder === entry.path}
                renaming={renaming === entry.path}
                events={rowEvents}
              />
            );
          })}
          <div ref={selectionBox.boxRef} className="selection-box" aria-hidden />
        </div>
        {entries.length === 0 && <p className="file-list-empty">{emptyMessage}</p>}
      </div>
    </div>
  );
}
