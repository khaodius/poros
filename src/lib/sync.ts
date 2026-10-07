// Pure helpers for the folder synchronization dialog; the planning itself is in
// src-tauri/src/sync.

import type { PathStyle } from "./fileSource";
import { formatSize, pluralize } from "./format";
import type {
  CompareMode,
  SyncAction,
  SyncChoice,
  SyncCounts,
  SyncDirection,
  SyncItem,
  SyncRunSummary,
} from "./types";

/** What each item will do: its planned action, a side picked for a conflict, or nothing. */
export type Choices = ReadonlyMap<number, SyncAction | null>;

export type ItemFilter = "all" | "upload" | "download" | "delete" | "conflict";

/** Every item goes ahead as planned, except conflicts, which wait for a decision. */
export function initialChoices(items: SyncItem[]): Map<number, SyncAction | null> {
  return new Map(items.map((item) => [item.id, item.action === "conflict" ? null : item.action]));
}

/** The actions an item can be given; a file against a folder has to be sorted out by hand. */
export function possibleActions(item: SyncItem): SyncAction[] {
  if (item.action !== "conflict") return [item.action];
  return item.reason === "bothChanged" ? ["upload", "download"] : [];
}

export function chosenItems(items: SyncItem[], choices: Choices): SyncChoice[] {
  return items.flatMap((item) => {
    const action = choices.get(item.id);
    return action ? [{ id: item.id, action }] : [];
  });
}

export function matchesFilter(item: SyncItem, filter: ItemFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "delete":
      return item.action === "deleteLocal" || item.action === "deleteRemote";
    default:
      return item.action === filter;
  }
}

export interface ActionTotal {
  items: number;
  files: number;
  bytes: number;
}

export interface ChosenTotals {
  upload: ActionTotal;
  download: ActionTotal;
  delete: ActionTotal;
  /** Conflicts still without a decision. */
  undecided: number;
}

export function chosenTotals(items: SyncItem[], choices: Choices): ChosenTotals {
  const empty = (): ActionTotal => ({ items: 0, files: 0, bytes: 0 });
  const totals: ChosenTotals = {
    upload: empty(),
    download: empty(),
    delete: empty(),
    undecided: 0,
  };
  for (const item of items) {
    const action = choices.get(item.id);
    if (!action) {
      if (item.action === "conflict" && possibleActions(item).length > 0) totals.undecided++;
      continue;
    }
    const total =
      action === "upload" ? totals.upload : action === "download" ? totals.download : totals.delete;
    // A conflict settled by copying moves the file on the chosen side.
    const facts =
      item.action === "conflict" ? (action === "upload" ? item.local : item.remote) : null;
    total.items++;
    total.files += item.files;
    total.bytes += facts ? facts.size : item.bytes;
  }
  return totals;
}

/** `12 files, 3.4 MB`, or the folders when only empty folders are left. */
export function describeTotal(total: ActionTotal): string {
  if (total.files === 0) return pluralize(total.items, "folder");
  return `${pluralize(total.files, "file")}, ${formatSize(total.bytes)}`;
}

/** What the chosen items will do, for the line beside the Synchronize button. */
export function describePlanned(totals: ChosenTotals): string {
  const parts: string[] = [];
  if (totals.upload.items > 0) parts.push(`Upload ${describeTotal(totals.upload)}`);
  if (totals.download.items > 0) parts.push(`Download ${describeTotal(totals.download)}`);
  if (totals.delete.items > 0) parts.push(`Delete ${pluralize(totals.delete.items, "item")}`);
  if (totals.undecided > 0) {
    parts.push(
      `${pluralize(totals.undecided, "conflict")} left as ${totals.undecided === 1 ? "it is" : "they are"}`,
    );
  }
  return parts.length > 0 ? `${parts.join(". ")}.` : "Nothing selected.";
}

export const DIRECTION_OPTIONS: { value: SyncDirection; label: string; hint: string }[] = [
  {
    value: "upload",
    label: "Local to server",
    hint: "Makes the server folder match the local one.",
  },
  {
    value: "download",
    label: "Server to local",
    hint: "Makes the local folder match the server one.",
  },
  {
    value: "both",
    label: "Both ways",
    hint: "Copies new files each way; where both sides have a file, the newer one wins.",
  },
];

export const COMPARE_OPTIONS: { value: CompareMode; label: string }[] = [
  { value: "sizeAndTime", label: "Size and modification time" },
  { value: "sizeOnly", label: "Size only" },
  { value: "checksum", label: "Contents (reads files of equal size)" },
  { value: "always", label: "Copy every file" },
];

export const ACTION_LABELS: Record<SyncAction, string> = {
  upload: "Upload",
  download: "Download",
  deleteLocal: "Delete local",
  deleteRemote: "Delete on server",
  conflict: "Conflict",
};

export function describeReason(item: SyncItem): string {
  switch (item.reason) {
    case "new":
      return item.action === "upload" ? "Only in the local folder" : "Only on the server";
    case "changed":
      return "Size or time differs";
    case "contentDiffers":
      return "Contents differ";
    case "always":
      return "Copied without comparing";
    case "newer":
      return item.action === "upload" ? "Newer in the local folder" : "Newer on the server";
    case "extraneous":
      return item.action === "deleteRemote" ? "Not in the local folder" : "Not on the server";
    case "typeDiffers":
      return item.local?.isDir
        ? "Folder here, file on the server"
        : "File here, folder on the server";
    case "bothChanged":
      return "Differs, with the same time on both sides";
  }
}

/** What was left alone, for the line under the list. */
export function describeCounts(counts: SyncCounts, direction: SyncDirection): string {
  const parts: string[] = [];
  if (counts.unchanged > 0) parts.push(`${counts.unchanged} unchanged`);
  if (counts.kept > 0) parts.push(`${counts.kept} kept as they are`);
  if (counts.extraOnTarget > 0) {
    const where = direction === "download" ? "only in the local folder" : "only on the server";
    parts.push(`${counts.extraOnTarget} ${where}, not deleted`);
  }
  if (counts.excluded > 0) parts.push(`${counts.excluded} excluded`);
  if (counts.passedOver > 0) parts.push(`${counts.passedOver} passed over`);
  return parts.join(", ");
}

export function describeRun(summary: SyncRunSummary): string {
  const parts = [
    `Queued ${pluralize(summary.queuedFiles, "file")} (${formatSize(summary.queuedBytes)})`,
  ];
  if (summary.deleted > 0) parts.push(`deleted ${pluralize(summary.deleted, "item")}`);
  if (summary.createdFolders > 0) {
    parts.push(`created ${pluralize(summary.createdFolders, "folder")}`);
  }
  let text = parts.join(", ");
  if (summary.failures.length > 0) {
    text += `. ${pluralize(summary.failures.length, "change")} failed; the log has the details`;
  }
  return `${text}.`;
}

/** Patterns from the exclude box, one per line. */
export function excludePatterns(text: string): string[] {
  return text.split(/\r?\n/).filter((line) => line.trim() !== "");
}

/** Whether `path` is `root` or inside it. */
export function isWithin(path: string, root: string, style: PathStyle): boolean {
  if (style === "windows") {
    const normalize = (value: string) => value.replace(/\//g, "\\").toLowerCase();
    return startsWithFolder(normalize(path), normalize(root), "\\");
  }
  return startsWithFolder(path, root, "/");
}

function startsWithFolder(path: string, root: string, separator: string): boolean {
  const folder = root.endsWith(separator) ? root.slice(0, -separator.length) : root;
  return path === root || path === folder || path.startsWith(`${folder}${separator}`);
}
