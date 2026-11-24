// The history versioned S3 buckets and Backblaze B2 keep of each file: what the Versions dialog
// offers for each entry, and how deleted files join a folder's listing.

import { entriesStillListed, formatCount } from "./deletion";
import { formatDate } from "./format";
import type { DateFormat } from "./settings";
import type {
  DeletedEntry,
  DeletedItem,
  DirListing,
  FileEntry,
  ObjectVersion,
  Protocol,
  Versioning,
} from "./types";

/** Mirrors `editor::MAX_FILE_BYTES`: larger files do not open in the editor. */
export const EDITOR_MAX_BYTES = 16 * 1024 * 1024;

/** Mirrors `versions::NO_VERSIONS`. */
export const NO_VERSIONS = "This bucket keeps no versions";

/**
 * Whether a server keeps histories of its files: S3 in buckets with versioning, B2 always. A
 * vault's session is never one: the bucket keeps versions of its encrypted files only.
 */
export function hasVersions(protocol: Protocol | undefined): boolean {
  return protocol === "s3" || protocol === "backblazeB2";
}

/** The bucket a path is in, or null at the top, where the buckets are listed. */
export function bucketOf(path: string): string | null {
  const [bucket] = path.split("/").filter(Boolean);
  return bucket ?? null;
}

/** Whether a path names a bucket itself, as the top of an S3 or B2 server lists them. */
export function isBucket(path: string): boolean {
  return path.split("/").filter(Boolean).length === 1;
}

/** Whether to offer versions: everywhere but a bucket known never to keep any. */
export function keepsVersions(versioning: Versioning | undefined): boolean {
  return versioning !== "off";
}

/** Deleted entries of each listing they were added to, so the pane can tell them apart. */
const deletions = new WeakMap<DirListing, ReadonlyMap<string, DeletedEntry>>();
const NONE: ReadonlyMap<string, DeletedEntry> = new Map();

/** A deleted file or folder as a row of the listing, dated when it was deleted. */
export function deletedAsEntry(deleted: DeletedEntry): FileEntry {
  return {
    name: deleted.name,
    path: deleted.path,
    kind: deleted.isDir ? "dir" : "file",
    size: deleted.size,
    modified: deleted.deleted,
    permissions: null,
    owner: null,
    group: null,
    hidden: deleted.name.startsWith("."),
  };
}

/** The listing with what was deleted from the folder among its entries. */
export function mergeDeleted(listing: DirListing, deleted: DeletedEntry[]): DirListing {
  const taken = new Set(listing.entries.map((entry) => entry.path));
  const added = deleted.filter((entry) => !taken.has(entry.path));
  const merged = { ...listing, entries: [...listing.entries, ...added.map(deletedAsEntry)] };
  deletions.set(merged, new Map(added.map((entry) => [entry.path, entry])));
  return merged;
}

/** The deleted entries of a listing `mergeDeleted` made, by path; none for any other. */
export function deletionsIn(listing: DirListing | null): ReadonlyMap<string, DeletedEntry> {
  return (listing && deletions.get(listing)) ?? NONE;
}

/** The entries of a listing that are there now, without the deleted ones it shows. */
export function liveEntries(listing: DirListing): FileEntry[] {
  const deleted = deletionsIn(listing);
  return deleted.size === 0
    ? listing.entries
    : listing.entries.filter((entry) => !deleted.has(entry.path));
}

/**
 * The entries among `chosen` that `listing` still shows as deleted, as it shows them now. One
 * written again since it was picked is left out, so nothing removes it with its history.
 */
export function entriesStillDeleted(
  chosen: readonly FileEntry[],
  listing: DirListing | null,
): FileEntry[] {
  if (!listing) return [];
  const deleted = deletionsIn(listing);
  return entriesStillListed(chosen, listing.entries).filter((entry) => deleted.has(entry.path));
}

export function deletedItems(entries: readonly FileEntry[]): DeletedItem[] {
  return entries.map((entry) => ({ path: entry.path, isDir: entry.kind === "dir" }));
}

/** What a restore of deleted files says while it runs, as a delete does. */
export function restoreProgressText({
  files,
  total,
  counting,
}: {
  files: number;
  total?: number;
  counting?: boolean;
}): string {
  if (!total) return files > 0 ? `${formatCount(files)} restored` : "Finding what to restore...";
  if (counting) return `${formatCount(files)} restored, ${formatCount(total)} found so far`;
  return `${formatCount(files)} of ${formatCount(total)} restored`;
}

/** How a history entry is marked in the Versions dialog. */
export type VersionStatus = "current" | "deleted" | "marker" | null;

export function versionStatus(version: ObjectVersion): VersionStatus {
  if (version.kind === "deleteMarker") return version.latest ? "deleted" : "marker";
  return version.latest ? "current" : null;
}

export const STATUS_LABELS: Record<Exclude<VersionStatus, null>, string> = {
  current: "Current",
  deleted: "Deleted",
  marker: "Delete marker",
};

/** Endings of files the editor has nothing to show for. */
const BINARY_EXTENSIONS = new Set(
  (
    "png jpg jpeg gif webp bmp ico tif tiff heic avif psd mp4 mkv mov avi webm m4v wmv flv " +
    "mp3 flac wav ogg m4a aac opus zip tar gz tgz bz2 xz zst 7z rar iso deb rpm dmg msi exe " +
    "dll so dylib bin img pdf doc docx odt xls xlsx ods ppt pptx odp parquet sqlite db " +
    "p12 pfx jar class wasm woff woff2 ttf otf"
  ).split(" "),
);

/** Whether a file's name says it holds text, as far as names say anything. */
export function isTextName(name: string): boolean {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return true;
  return !BINARY_EXTENSIONS.has(name.slice(dot + 1).toLowerCase());
}

export type VersionAction = "restore" | "download" | "open" | "compare" | "delete";

/**
 * What can be done with one entry of a file's history, newest first in `history`. A marker has
 * no content, so it can only be removed; text within the editor's limit opens and compares with
 * the file as it is now, where there is one.
 */
export function versionActions(
  version: ObjectVersion,
  history: readonly ObjectVersion[],
  fileName: string,
): ReadonlySet<VersionAction> {
  const actions = new Set<VersionAction>(["delete"]);
  if (version.kind === "deleteMarker") return actions;
  actions.add("download");
  if (!version.latest) actions.add("restore");
  const readable = isTextName(fileName) && version.size <= EDITOR_MAX_BYTES;
  if (readable) actions.add("open");
  const current = history.find((entry) => entry.latest);
  const comparable =
    current !== undefined &&
    current !== version &&
    current.kind === "version" &&
    current.size <= EDITOR_MAX_BYTES;
  if (readable && comparable) actions.add("compare");
  return actions;
}

/**
 * What deleting one entry of a file's history, newest first in `history`, does to the file as
 * it shows, from the entry under it; null for an older entry, whose going changes nothing.
 */
export function afterDeleting(
  version: ObjectVersion,
  history: readonly ObjectVersion[],
): string | null {
  if (!version.latest) return null;
  const index = history.findIndex((entry) => entry.versionId === version.versionId);
  const next = index < 0 ? undefined : history[index + 1];
  if (version.kind === "deleteMarker") {
    if (!next) return "Nothing of the file is left after it.";
    return next.kind === "version"
      ? "The file comes back as its newest version."
      : "The file stays deleted: another delete marker is under this one.";
  }
  if (!next) return "This is the only version: the file will be gone.";
  return next.kind === "version"
    ? "The version before it becomes the current file."
    : "The file will show as deleted: the entry before it is a delete marker.";
}

const localeWithSeconds = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "medium",
});

/** When a version was stored, to the second: versions often come minutes apart or less. */
export function formatVersionTime(epochSeconds: number | null, dateFormat: DateFormat): string {
  if (epochSeconds === null) return "Unknown time";
  if (dateFormat === "locale") return localeWithSeconds.format(new Date(epochSeconds * 1000));
  return formatDate(epochSeconds, "seconds");
}

/**
 * The name an older version gets when downloaded: the file's own with the version's time before
 * the extension, so it does not take the place of a copy of the current file.
 */
export function versionFileName(name: string, version: ObjectVersion): string {
  if (version.latest || version.modified === null) return name;
  const stamp = formatDate(version.modified, "seconds").replaceAll(":", "-");
  const dot = name.lastIndexOf(".");
  return dot > 0 ? `${name.slice(0, dot)} (${stamp})${name.slice(dot)}` : `${name} (${stamp})`;
}

/** The first characters of an ETag, which tell versions apart; the rest only clutters. */
export function shortTag(etag: string): string {
  return etag.length > 10 ? `${etag.slice(0, 8)}...` : etag;
}

/** What the Versions dialog says about the bucket, or null when there is nothing to add. */
export function versioningNote(versioning: Versioning | undefined): string | null {
  switch (versioning) {
    case "suspended":
      return "Versioning is suspended on this bucket: what it kept stays, and new writes replace the version marked null.";
    case "unknown":
      return "These keys may not see whether the bucket keeps versions, so some actions may be refused.";
    default:
      return null;
  }
}
