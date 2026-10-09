// Turns a comparison from the backend into what the side-by-side view shows: which rows are on
// screen, where each run of changes starts, and which parts of a line to highlight.

import type { CompareContent, CompareRow, CompareRowKind, Comparison, LineEnding } from "./types";

export type DisplayItem =
  | { kind: "row"; index: number }
  /** Unchanged rows from `start` up to, not including, `end`, folded away. */
  | { kind: "fold"; start: number; end: number };

/** Unchanged rows kept on show around each change when only changes are shown. */
export const CONTEXT_ROWS = 3;

/**
 * Every row, or with `changesOnly` the changed rows and a few unchanged ones around each. The
 * other unchanged rows fold away unless their fold, named by its first row, is expanded.
 */
export function displayItems(
  rows: CompareRow[],
  changesOnly: boolean,
  expanded: ReadonlySet<number> = new Set(),
  context = CONTEXT_ROWS,
): DisplayItem[] {
  if (!changesOnly) return rows.map((_, index) => ({ kind: "row", index }));
  const shown = new Array<boolean>(rows.length).fill(false);
  rows.forEach((row, index) => {
    if (row.kind === "equal") return;
    const last = Math.min(rows.length - 1, index + context);
    for (let near = Math.max(0, index - context); near <= last; near++) shown[near] = true;
  });
  const items: DisplayItem[] = [];
  let index = 0;
  while (index < rows.length) {
    let end = index;
    while (end < rows.length && !shown[end]) end++;
    // A fold takes a line of its own, so hiding a single row would save nothing.
    if (end - index > 1 && !expanded.has(index)) {
      items.push({ kind: "fold", start: index, end });
    } else {
      for (let hidden = index; hidden < end; hidden++) items.push({ kind: "row", index: hidden });
    }
    if (end < rows.length) items.push({ kind: "row", index: end });
    index = end + 1;
  }
  return items;
}

function isChange(item: DisplayItem, rows: CompareRow[]): boolean {
  return item.kind === "row" && rows[item.index].kind !== "equal";
}

/** Positions in `items` where a run of changed rows begins. */
export function changeStarts(items: DisplayItem[], rows: CompareRow[]): number[] {
  const starts: number[] = [];
  items.forEach((item, position) => {
    if (isChange(item, rows) && (position === 0 || !isChange(items[position - 1], rows))) {
      starts.push(position);
    }
  });
  return starts;
}

/** The next run of changes after `from`, or the one before it; null at either end. */
export function adjacentChange(starts: number[], from: number, direction: 1 | -1): number | null {
  if (direction === 1) return starts.find((start) => start > from) ?? null;
  const before = starts.filter((start) => start < from);
  return before.length > 0 ? before[before.length - 1] : null;
}

export interface OverviewMark {
  /** Fractions of the whole list. */
  top: number;
  height: number;
  kind: Exclude<CompareRowKind, "equal">;
}

/** One mark per run of changes, for the strip beside the list. */
export function overviewMarks(items: DisplayItem[], rows: CompareRow[]): OverviewMark[] {
  const marks: OverviewMark[] = [];
  for (const start of changeStarts(items, rows)) {
    let end = start;
    const kinds = new Set<CompareRowKind>();
    while (end < items.length && isChange(items[end], rows)) {
      const item = items[end] as { kind: "row"; index: number };
      kinds.add(rows[item.index].kind);
      end++;
    }
    const kind = kinds.size === 1 ? ([...kinds][0] as OverviewMark["kind"]) : "changed";
    marks.push({ top: start / items.length, height: (end - start) / items.length, kind });
  }
  return marks;
}

export interface Segment {
  text: string;
  changed: boolean;
}

/**
 * A line split into runs inside and outside its changed ranges. A changed line without ranges
 * differs throughout.
 */
export function segments(
  text: string,
  kind: CompareRowKind,
  changes: [number, number][] | undefined,
): Segment[] {
  if (kind !== "changed") return [{ text, changed: false }];
  if (!changes || changes.length === 0) return [{ text, changed: text.length > 0 }];
  const parts: Segment[] = [];
  let position = 0;
  for (const [rawStart, rawEnd] of changes) {
    const start = Math.min(Math.max(rawStart, position), text.length);
    const end = Math.min(Math.max(rawEnd, start), text.length);
    if (start > position) parts.push({ text: text.slice(position, start), changed: false });
    if (end > start) parts.push({ text: text.slice(start, end), changed: true });
    position = end;
  }
  if (position < text.length) parts.push({ text: text.slice(position), changed: false });
  return parts;
}

const LINE_ENDING_NAMES: Record<LineEnding, string> = {
  none: "none",
  lf: "LF (Unix)",
  crlf: "CRLF (Windows)",
  mixed: "mixed",
};

/** Says how the files end their lines when they do it differently. */
export function lineEndingNote(left: LineEnding, right: LineEnding): string | null {
  if (left === right || left === "none" || right === "none") return null;
  return `Line endings differ: ${LINE_ENDING_NAMES[left]} on the left, ${LINE_ENDING_NAMES[right]} on the right.`;
}

function countLines(count: number, what: string): string {
  return `${count} ${what} ${count === 1 ? "line" : "lines"}`;
}

/** One sentence on how the files differ. */
export function verdict(comparison: Comparison, ignoring: boolean): string {
  const { content, identical } = comparison;
  if (identical) return "The files are identical.";
  if (content.kind === "binary") {
    return content.tooLarge
      ? "The files differ. They are too large to show side by side."
      : "The files differ. They are not text, so only their bytes were compared.";
  }
  const parts = [
    content.changed > 0 && countLines(content.changed, "changed"),
    content.added > 0 && countLines(content.added, "added"),
    content.removed > 0 && countLines(content.removed, "removed"),
  ].filter(Boolean);
  if (parts.length > 0) return `${parts.join(", ")}.`;
  if (content.leftLineEnding !== content.rightLineEnding) return "Only the line endings differ.";
  return ignoring
    ? "The files differ only in what is being ignored."
    : "The lines are the same; the files differ in their bytes.";
}

const SWAPPED_KIND: Record<CompareRowKind, CompareRowKind> = {
  equal: "equal",
  changed: "changed",
  added: "removed",
  removed: "added",
};

/** The same comparison with the files the other way round. */
export function swapComparison(comparison: Comparison): Comparison {
  const { content } = comparison;
  const swapped: CompareContent =
    content.kind === "binary"
      ? content
      : {
          ...content,
          leftLines: content.rightLines,
          rightLines: content.leftLines,
          rows: content.rows.map((row) => ({
            kind: SWAPPED_KIND[row.kind],
            left: row.right,
            right: row.left,
            leftChanges: row.rightChanges,
            rightChanges: row.leftChanges,
          })),
          added: content.removed,
          removed: content.added,
          leftLineEnding: content.rightLineEnding,
          rightLineEnding: content.leftLineEnding,
        };
  return {
    ...comparison,
    leftSize: comparison.rightSize,
    rightSize: comparison.leftSize,
    content: swapped,
  };
}
