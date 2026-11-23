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

/**
 * The entry a rename acts on: the one selected row on show, when just one is. Rows the filter or
 * the hidden files switch keep out of sight stay selected but are left alone, as every other
 * action leaves them.
 */
export function onlySelected(
  shown: readonly FileEntry[],
  selection: ReadonlySet<string>,
): FileEntry | undefined {
  const selected = shown.filter((entry) => selection.has(entry.path));
  return selected.length === 1 ? selected[0] : undefined;
}
