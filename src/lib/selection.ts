// Which rows of a file list a click or a key selects.

import type { FileEntry } from "./types";

/**
 * Where a range selected up to `toPath` starts: at the anchor, the row clicked or stepped from
 * last, else at the cursor, else at `toPath` itself. One that is not on show, such as a row the
 * filter hides, is passed over, since no range can be counted from it.
 */
export function rangeStart(
  shown: readonly FileEntry[],
  anchorPath: string | null,
  cursorPath: string | null,
  toPath: string,
): string {
  const onShow = (path: string | null) =>
    path !== null && shown.some((entry) => entry.path === path);
  return [anchorPath, cursorPath].find(onShow) ?? toPath;
}
