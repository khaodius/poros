import type { FileEntry } from "./types";

/** File list columns besides the name, in the order they show. */
export const DETAIL_COLUMNS = ["size", "type", "modified", "permissions", "owner"] as const;
export type DetailColumn = (typeof DETAIL_COLUMNS)[number];

export const COLUMN_LABELS: Record<DetailColumn, string> = {
  size: "Size",
  type: "Type",
  modified: "Modified",
  permissions: "Permissions",
  owner: "Owner",
};

/** The detail columns a listing has values for: local listings on Windows have no owners. */
export function availableColumns(entries: readonly FileEntry[]): DetailColumn[] {
  return DETAIL_COLUMNS.filter(
    (key) =>
      (key !== "permissions" || entries.some((entry) => entry.permissions !== null)) &&
      (key !== "owner" || entries.some((entry) => entry.owner !== null)),
  );
}
