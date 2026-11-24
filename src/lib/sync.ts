// Pure helpers for the folder synchronization dialog; the planning itself is in
// src-tauri/src/sync.

import type { PathStyle } from "./fileSource";
import { formatSize } from "./format";
import type {
  SyncAction,
  SyncChoice,
  SyncCounts,
  SyncDirection,
  SyncFacts,
  SyncItem,
  SyncRunSummary,
} from "./types";
import { t } from "../i18n";

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
  if (total.files === 0) return t("sync.folders", { count: total.items });
  return t("sync.files2", { count: total.files, size: formatSize(total.bytes) });
}

/** What the chosen items will do, for the line beside the Synchronize button. */
export function describePlanned(totals: ChosenTotals): string {
  const parts: string[] = [];
  if (totals.upload.items > 0) {
    parts.push(t("sync.plannedUpload", { total: describeTotal(totals.upload) }));
  }
  if (totals.download.items > 0) {
    parts.push(t("sync.plannedDownload", { total: describeTotal(totals.download) }));
  }
  if (totals.delete.items > 0) parts.push(t("sync.deleteItems", { count: totals.delete.items }));
  if (totals.undecided > 0) parts.push(t("sync.conflictsLeft", { count: totals.undecided }));
  return parts.length > 0
    ? t("sync.plannedSentences", { parts: parts.join(t("sync.plannedSeparator")) })
    : t("sync.nothingSelected");
}

export const DIRECTION_OPTIONS: { value: SyncDirection; label: string; hint: string }[] = [
  {
    value: "upload",
    get label() {
      return t("sync.localToServer");
    },
    get hint() {
      return t("sync.makesTheServerFolderMatch");
    },
  },
  {
    value: "download",
    get label() {
      return t("sync.serverToLocal");
    },
    get hint() {
      return t("sync.makesTheLocalFolderMatch");
    },
  },
  {
    value: "both",
    get label() {
      return t("sync.bothWays");
    },
    get hint() {
      return t("sync.copiesNewFilesEachWay");
    },
  },
];

export const ACTION_LABELS: Record<SyncAction, string> = {
  get upload() {
    return t("common.upload");
  },
  get download() {
    return t("common.download");
  },
  get deleteLocal() {
    return t("sync.deleteLocal");
  },
  get deleteRemote() {
    return t("sync.deleteOnServer");
  },
  get conflict() {
    return t("sync.conflict");
  },
};

export function describeReason(item: SyncItem): string {
  switch (item.reason) {
    case "new":
      return item.action === "upload" ? t("sync.onlyInTheLocalFolder") : t("sync.onlyOnTheServer");
    case "changed":
      return t("sync.sizeOrTimeDiffers");
    case "contentDiffers":
      return t("sync.contentsDiffer");
    case "always":
      return t("sync.copiedWithoutComparing");
    case "newer":
      return item.action === "upload"
        ? t("sync.newerInTheLocalFolder")
        : t("sync.newerOnTheServer");
    case "extraneous":
      return item.action === "deleteRemote"
        ? t("sync.notInTheLocalFolder")
        : t("sync.notOnTheServer");
    case "typeDiffers":
      return t("sync.typeDiffers", {
        here: t(KIND_HERE[kindOf(item.local)]),
        there: t(KIND_THERE[kindOf(item.remote)]),
      });
    case "bothChanged":
      return t("sync.differsWithTheSameTimeOnBothSides");
    case "append":
      return t("sync.shorterOnTheTarget");
    case "attributes":
      return t("sync.sameContentsOnlyTheTimeOrPermissions");
  }
}

type ItemKind = "nothing" | "folder" | "link" | "file";

/** What is on this side, to start a sentence. */
const KIND_HERE = {
  nothing: "sync.nothingHere",
  folder: "sync.folderHere",
  link: "sync.linkHere",
  file: "sync.fileHere",
} as const satisfies Record<ItemKind, string>;

/** What is on the other side, inside a sentence. */
const KIND_THERE = {
  nothing: "sync.nothingThere",
  folder: "sync.folderThere",
  link: "sync.linkThere",
  file: "sync.fileThere",
} as const satisfies Record<ItemKind, string>;

function kindOf(facts: SyncFacts | undefined): ItemKind {
  if (!facts) return "nothing";
  if (facts.isDir) return "folder";
  return facts.isLink ? "link" : "file";
}

/** What was left alone, for the line under the list. */
export function describeCounts(counts: SyncCounts, direction: SyncDirection): string {
  const parts: string[] = [];
  if (counts.unchanged > 0) parts.push(t("sync.countUnchanged", { count: counts.unchanged }));
  if (counts.kept > 0) parts.push(t("sync.countKeptAsTheyAre", { count: counts.kept }));
  if (counts.extraOnTarget > 0) {
    parts.push(
      direction === "download"
        ? t("sync.countOnlyInTheLocalFolderNotDeleted", { count: counts.extraOnTarget })
        : t("sync.countOnlyOnTheServerNotDeleted", { count: counts.extraOnTarget }),
    );
  }
  if (counts.protected > 0) {
    parts.push(t("sync.countProtectedFromDeletion", { count: counts.protected }));
  }
  if (counts.heldBack > 0) {
    parts.push(t("sync.deletionsHeldBackByThe", { count: counts.heldBack }));
  }
  if (counts.notCreated > 0) parts.push(t("sync.countNewNotCreated", { count: counts.notCreated }));
  if (counts.sizeLimited > 0) {
    parts.push(t("sync.countOutsideTheSizeLimits", { count: counts.sizeLimited }));
  }
  if (counts.pruned > 0) parts.push(t("sync.emptyFoldersLeftOut", { count: counts.pruned }));
  if (counts.excluded > 0) parts.push(t("sync.countExcluded", { count: counts.excluded }));
  if (counts.passedOver > 0) parts.push(t("sync.countPassedOver", { count: counts.passedOver }));
  return parts.join(t("common.listSeparator"));
}

export function describeRun(summary: SyncRunSummary): string {
  if (summary.dryRun) {
    const parts = [
      t("sync.wouldCopyFiles", {
        count: summary.queuedFiles,
        size: formatSize(summary.queuedBytes),
      }),
    ];
    if (summary.deleted > 0) parts.push(t("sync.deleteItems2", { count: summary.deleted }));
    if (summary.createdFolders > 0) {
      parts.push(t("sync.createFolders", { count: summary.createdFolders }));
    }
    return t("sync.dryRunSummary", { parts: parts.join(t("common.listSeparator")) });
  }
  const parts = [
    t("sync.queuedFiles", { count: summary.queuedFiles, size: formatSize(summary.queuedBytes) }),
  ];
  if (summary.deleted > 0) parts.push(t("sync.deletedItems", { count: summary.deleted }));
  if (summary.deletingLater > 0) {
    parts.push(t("sync.itemsToDeleteOnceCopying", { count: summary.deletingLater }));
  }
  if (summary.createdFolders > 0) {
    parts.push(t("sync.createdFolders", { count: summary.createdFolders }));
  }
  if (summary.createdLinks > 0) parts.push(t("sync.createdLinks", { count: summary.createdLinks }));
  if (summary.updated > 0) parts.push(t("sync.updatedFiles", { count: summary.updated }));
  if (summary.removedSources > 0) {
    parts.push(t("sync.removedUnchangedSourceFiles", { count: summary.removedSources }));
  }
  const done = parts.join(t("common.listSeparator"));
  return summary.failures.length > 0
    ? t("sync.runSummaryWithFailures", { done, count: summary.failures.length })
    : t("sync.runSummary", { done });
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
