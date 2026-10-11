// Where a file list's cursor goes when the rows under it change.

import type { FileEntry } from "./types";

/** The entry `path` is, or is inside: the folder to put the cursor on when coming back out of
 * `path`. Paths use either separator, so this serves local and server listings alike. */
export function entryLeadingTo(entries: readonly FileEntry[], path: string): string | undefined {
  return entries.find(
    (entry) =>
      path === entry.path ||
      path.startsWith(`${entry.path}/`) ||
      path.startsWith(`${entry.path}\\`),
  )?.path;
}

/** The row to put the cursor on once `removed` are gone: the cursor's own row if it stays,
 * otherwise the next row that stays after the first one removed, else the last before it. */
export function entryAfterRemoval(
  entries: readonly FileEntry[],
  removed: ReadonlySet<string>,
  cursorPath: string | null,
): string | undefined {
  if (cursorPath !== null && !removed.has(cursorPath)) return cursorPath;
  const cursorIndex = entries.findIndex((entry) => entry.path === cursorPath);
  const start =
    cursorIndex >= 0 ? cursorIndex : entries.findIndex((entry) => removed.has(entry.path));
  if (start < 0) return undefined;
  const stays = (entry: FileEntry) => !removed.has(entry.path);
  const after = entries.slice(start).find(stays);
  return (after ?? entries.slice(0, start).reverse().find(stays))?.path;
}
