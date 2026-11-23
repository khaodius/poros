import { useCallback, useLayoutEffect, useRef, type RefObject } from "react";
import { anchoredScrollTop } from "../lib/listAnchor";

interface AnchoredList {
  /** The folder on show. */
  folder: string | undefined;
  /** A value that is another one each time the folder is listed again. */
  listing: unknown;
  /** The rows on show, top to bottom, each named so it can be found again. */
  rowKeys: readonly string[];
  cursor: string | null;
  /** Rows the user is in the middle of something with. */
  held: ReadonlySet<string>;
  rowHeight: number;
}

/**
 * Keeps a file list where it is when the folder on show is listed again: the rows in view stay
 * at the height they were, whatever was added or removed above them. The function returned
 * tells, once, whether the rows it is given were kept in place, and so are not to be scrolled to
 * the cursor as rows that changed otherwise are.
 */
export function useListAnchor(
  scrollRef: RefObject<HTMLElement | null>,
  { folder, listing, rowKeys, cursor, held, rowHeight }: AnchoredList,
): (rowKeys: readonly string[]) => boolean {
  const previous = useRef<Pick<AnchoredList, "folder" | "listing" | "rowKeys" | "cursor">>(null);
  const kept = useRef<readonly string[] | null>(null);

  useLayoutEffect(() => {
    const before = previous.current;
    previous.current = { folder, listing, rowKeys, cursor };
    const scroller = scrollRef.current;
    if (!before || !scroller || folder === undefined) return;
    const listedAgain = before.folder === folder && before.listing !== listing;
    // A cursor put on another row, as on a file just made, is followed there instead.
    if (!listedAgain || (cursor !== null && cursor !== before.cursor)) return;
    kept.current = rowKeys;
    const target = anchoredScrollTop({
      before: before.rowKeys,
      after: rowKeys,
      scrollTop: scroller.scrollTop,
      viewportHeight: scroller.clientHeight,
      rowHeight,
      held: [...held],
      cursor: before.cursor,
    });
    if (target !== null) scroller.scrollTop = target;
  }, [scrollRef, folder, listing, rowKeys, cursor, held, rowHeight]);

  return useCallback((rows: readonly string[]) => {
    const wasKept = kept.current === rows;
    if (wasKept) kept.current = null;
    return wasKept;
  }, []);
}
