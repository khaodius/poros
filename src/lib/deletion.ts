// What a delete on a server says while it runs, and what a delete asked for acts on.

import type { FileEntry } from "./types";

export interface DeleteProgress {
  /** Entries deleted so far. */
  files: number;
  total?: number;
  counting?: boolean;
  failed?: number;
}

const countFormat = new Intl.NumberFormat("en-US");

export const formatCount = (count: number) => countFormat.format(count);

/** `1,204 of 5,000 deleted`, or while entries are still turning up, how many so far. */
export function deleteProgressText({ files, total, counting }: DeleteProgress): string {
  if (!total) return files > 0 ? `${formatCount(files)} deleted` : "Counting what to delete...";
  if (counting) return `${formatCount(files)} deleted, ${formatCount(total)} found so far`;
  return `${formatCount(files)} of ${formatCount(total)} deleted`;
}

/** How far along a delete is, from 0 to 1, or null while entries are still turning up. */
export function deleteFraction({ files, total, counting, failed }: DeleteProgress): number | null {
  if (!total || counting) return null;
  return Math.min(1, (files + (failed ?? 0)) / total);
}

/**
 * The entries among `chosen` that the folder still lists, as it lists them now. A delete picked
 * from a listing that was replaced since acts on these: what is gone is not counted, and what
 * turned from a file into a folder is told as one.
 */
export function entriesStillListed(
  chosen: readonly FileEntry[],
  listed: readonly FileEntry[],
): FileEntry[] {
  const now = new Map(listed.map((entry) => [entry.path, entry]));
  return chosen.flatMap((entry) => now.get(entry.path) ?? []);
}
