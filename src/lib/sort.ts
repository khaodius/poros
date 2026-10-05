import type { FileEntry } from "./types";

export type SortKey = "name" | "size" | "modified" | "permissions" | "owner";
export interface SortSpec {
  key: SortKey;
  dir: 1 | -1;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

export function isDirLike(entry: FileEntry): boolean {
  return entry.kind === "dir" || entry.linkTarget === "dir";
}

/** Folders always first, then by the chosen column; ties broken by natural name order. */
export function sortEntries(entries: FileEntry[], spec: SortSpec): FileEntry[] {
  const byName = (a: FileEntry, b: FileEntry) => collator.compare(a.name, b.name);
  const column = (a: FileEntry, b: FileEntry): number => {
    switch (spec.key) {
      case "size":
        return a.size - b.size;
      case "modified":
        return (a.modified ?? 0) - (b.modified ?? 0);
      case "permissions":
        return (a.permissions ?? 0) - (b.permissions ?? 0);
      case "owner":
        return collator.compare(a.owner ?? "", b.owner ?? "");
      case "name":
        return 0;
    }
  };
  return [...entries].sort((a, b) => {
    const dirs = Number(isDirLike(b)) - Number(isDirLike(a));
    if (dirs !== 0) return dirs;
    return (column(a, b) || byName(a, b)) * spec.dir;
  });
}

export function filterEntries(
  entries: FileEntry[],
  { showHidden, query }: { showHidden: boolean; query: string },
): FileEntry[] {
  const q = query.trim().toLowerCase();
  return entries.filter(
    (e) => (showHidden || !e.hidden) && (q === "" || e.name.toLowerCase().includes(q)),
  );
}
