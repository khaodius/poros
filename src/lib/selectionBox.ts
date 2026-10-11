// The box dragged across a file list to select the rows it touches, in the coordinates of the
// list's content, so it stays put on the rows as the list scrolls under it.

import type { Point, Rect, Size } from "./docking";

/** How near the list's top or bottom edge the pointer starts scrolling it. */
const EDGE_ZONE = 32;
/** Past this far into the edge zone, or beyond the edge, scrolling gets no faster. */
const MAX_EDGE_DEPTH = 160;
/** Pixels a millisecond the list scrolls for each pixel the pointer is into the edge zone. */
const SPEED_PER_PIXEL = 0.012;

/** The box with corners at `start` and `end`, kept inside the content's `bounds`. */
export function boxBetween(start: Point, end: Point, bounds: Size): Rect {
  const clampX = (x: number) => Math.min(Math.max(x, 0), bounds.width);
  const clampY = (y: number) => Math.min(Math.max(y, 0), bounds.height);
  const left = clampX(Math.min(start.x, end.x));
  const top = clampY(Math.min(start.y, end.y));
  return {
    left,
    top,
    width: clampX(Math.max(start.x, end.x)) - left,
    height: clampY(Math.max(start.y, end.y)) - top,
  };
}

/**
 * The first and last of `rowCount` rows stacked from the top that `box` touches, or null when
 * it touches none. Rows are `rowsWidth` wide, leaving the rest of the width to no row.
 */
export function rowsInBox(
  box: Rect,
  rowHeight: number,
  rowCount: number,
  rowsWidth: number,
): { first: number; last: number } | null {
  if (rowCount === 0 || box.left >= rowsWidth) return null;
  const first = Math.floor(box.top / rowHeight);
  // A box ending on the line between two rows touches only the upper one.
  const last = Math.min(
    rowCount - 1,
    Math.max(first, Math.ceil((box.top + box.height) / rowHeight) - 1),
  );
  return first <= last ? { first, last } : null;
}

/**
 * How fast, in pixels a millisecond, a box dragged near or past the top or bottom edge of a list
 * scrolls it: faster the further in the pointer is. Negative scrolls up.
 */
export function edgeScrollSpeed(pointerY: number, top: number, bottom: number): number {
  const above = top + EDGE_ZONE - pointerY;
  const below = pointerY - (bottom - EDGE_ZONE);
  const depth = above > 0 ? -above : below > 0 ? below : 0;
  return Math.sign(depth) * Math.min(Math.abs(depth), MAX_EDGE_DEPTH) * SPEED_PER_PIXEL;
}
