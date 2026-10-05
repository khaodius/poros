import type { FileEntry } from "./types";

export type SortKey = "name" | "size" | "modified" | "permissions" | "owner";
export interface SortSpec {
  key: SortKey;
  direction: 1 | -1;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function isDirLike(entry: FileEntry): boolean {
  return entry.kind === "dir" || entry.linkTarget === "dir";
}

export function sortEntries(entries: FileEntry[], spec: SortSpec): FileEntry[] {
  const byName = (left: FileEntry, right: FileEntry) => collator.compare(left.name, right.name);
  const column = (left: FileEntry, right: FileEntry): number => {
    switch (spec.key) {
      case "size":
        return left.size - right.size;
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
    const foldersFirst = Number(isDirLike(right)) - Number(isDirLike(left));
    if (foldersFirst !== 0) return foldersFirst;
    return (column(left, right) || byName(left, right)) * spec.direction;
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
