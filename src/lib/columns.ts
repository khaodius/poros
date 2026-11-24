import type { FileEntry } from "./types";
import { t } from "../i18n";

/** File list columns besides the name, in the order they show. */
export const DETAIL_COLUMNS = ["size", "type", "modified", "permissions", "owner"] as const;
export type DetailColumn = (typeof DETAIL_COLUMNS)[number];

export const COLUMN_LABELS: Record<DetailColumn, string> = {
  get size() {
    return t("common.size");
  },
  get type() {
    return t("fileList.type");
  },
  get modified() {
    return t("fileList.modified");
  },
  get permissions() {
    return t("fileList.permissions");
  },
  get owner() {
    return t("fileList.owner");
  },
};

/** The detail columns a listing has values for: local listings on Windows have no owners. */
export function availableColumns(entries: readonly FileEntry[]): DetailColumn[] {
  return DETAIL_COLUMNS.filter(
    (key) =>
      (key !== "permissions" || entries.some((entry) => entry.permissions !== null)) &&
      (key !== "owner" || entries.some((entry) => entry.owner !== null)),
  );
}

/** Widths picked by dragging a heading's edge, in pixels at the base font size. */
export type ColumnWidths = Partial<Record<DetailColumn, number>>;

export const COLUMN_WIDTH_LIMITS = { min: 40, max: 640 };

/** Keeps the widths of known columns, rounded and within the limits. */
export function sanitizeColumnWidths(stored: unknown): ColumnWidths {
  if (!stored || typeof stored !== "object" || Array.isArray(stored)) return {};
  const source = stored as Record<string, unknown>;
  const widths: ColumnWidths = {};
  for (const key of DETAIL_COLUMNS) {
    const width = source[key];
    if (typeof width !== "number" || !Number.isFinite(width)) continue;
    widths[key] = Math.min(
      COLUMN_WIDTH_LIMITS.max,
      Math.max(COLUMN_WIDTH_LIMITS.min, Math.round(width)),
    );
  }
  return widths;
}
