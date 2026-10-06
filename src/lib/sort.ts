import type { FileEntry } from "./types";

export const SORT_KEYS = ["name", "size", "type", "modified", "permissions", "owner"] as const;
export type SortKey = (typeof SORT_KEYS)[number];
export interface SortSpec {
  key: SortKey;
  direction: 1 | -1;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function isDirLike(entry: FileEntry): boolean {
  return entry.kind === "dir" || entry.linkTarget === "dir";
}

/** What the type column shows: "Folder", the extension in capitals, or "File". */
export function typeLabel(entry: FileEntry): string {
  if (isDirLike(entry)) return "Folder";
  if (entry.linkTarget === "broken") return "Broken link";
  const dot = entry.name.lastIndexOf(".");
  // A leading dot marks a hidden file, not an extension.
  return dot > 0 && dot < entry.name.length - 1 ? entry.name.slice(dot + 1).toUpperCase() : "File";
}

/**
 * Sorts by `spec`, keeping folders on top when asked. Entries that tie are ordered by name; when
 * sorting by type, names stay A to Z within each type whichever way the types run.
 */
export function sortEntries(
  entries: FileEntry[],
  spec: SortSpec,
  foldersFirst = true,
): FileEntry[] {
  const byName = (left: FileEntry, right: FileEntry) => collator.compare(left.name, right.name);
  const column = (left: FileEntry, right: FileEntry): number => {
    switch (spec.key) {
      case "size":
        return left.size - right.size;
      case "type":
        return collator.compare(typeLabel(left), typeLabel(right));
      case "modified":
        return (left.modified ?? 0) - (right.modified ?? 0);
      case "permissions":
        return (left.permissions ?? 0) - (right.permissions ?? 0);
      case "owner":
        return collator.compare(left.owner ?? "", right.owner ?? "");
      case "name":
        return 0;
    }
  };
  return [...entries].sort((left, right) => {
    if (foldersFirst) {
      const folderOrder = Number(isDirLike(right)) - Number(isDirLike(left));
      if (folderOrder !== 0) return folderOrder;
    }
    const byColumn = column(left, right) * spec.direction;
    if (byColumn !== 0) return byColumn;
    return spec.key === "type" ? byName(left, right) : byName(left, right) * spec.direction;
  });
}

export function filterEntries(
  entries: FileEntry[],
  { showHidden, query }: { showHidden: boolean; query: string },
): FileEntry[] {
  const needle = query.trim().toLowerCase();
  return entries.filter(
    (entry) =>
      (showHidden || !entry.hidden) && (needle === "" || entry.name.toLowerCase().includes(needle)),
  );
}
