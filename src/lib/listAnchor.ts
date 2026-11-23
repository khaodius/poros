// Keeps a file list where it is when the listing of the folder on show is replaced by a newer
// one: a row the user looks at stays at the height it was, whatever was added or removed above
// it. Every row of a list is one height, so its place is its number times that height.

export interface AnchorQuery {
  /** The rows before the listing changed, top to bottom, each named so it can be found again. */
  before: readonly string[];
  /** The rows now. */
  after: readonly string[];
  /** How far the list was scrolled, in pixels. */
  scrollTop: number;
  viewportHeight: number;
  rowHeight: number;
  /** Rows the user is in the middle of something with: a name being typed, an open menu. */
  held: readonly string[];
  /** The cursor's row. */
  cursor: string | null;
}

/**
 * How far to scroll the list so that it stays in place, or null to leave it as it is. The row
 * kept still is one the user is busy with, else the cursor's, else the first one in view; a row
 * out of view does not count, and a list at its top stays there, so new first rows show.
 */
export function anchoredScrollTop(query: AnchorQuery): number | null {
  const { before, after, scrollTop, viewportHeight, rowHeight, held, cursor } = query;
  if (rowHeight <= 0) return null;
  const first = Math.max(0, Math.ceil(scrollTop / rowHeight));
  const last = Math.min(before.length - 1, Math.floor((scrollTop + viewportHeight) / rowHeight));
  const inView = (row: string): number | null => {
    const index = before.indexOf(row);
    return index >= first && index <= last ? index : null;
  };
  const places = new Map(after.map((row, index) => [row, index]));
  const stays = (index: number | null): index is number =>
    index !== null && places.has(before[index]);

  let anchor = held.map(inView).find(stays);
  if (anchor === undefined) {
    if (scrollTop <= 0) return null;
    const cursorRow = cursor === null ? null : inView(cursor);
    if (stays(cursorRow)) anchor = cursorRow;
  }
  for (let index = first; anchor === undefined && index <= last; index += 1) {
    if (stays(index)) anchor = index;
  }
  if (anchor === undefined) return null;
  const offset = anchor * rowHeight - scrollTop;
  const target = Math.max(0, places.get(before[anchor])! * rowHeight - offset);
  return target === scrollTop ? null : target;
}
