// What the preview pane shows for the item under the cursor and how much of a file it reads to
// find out. Everything here decides from what is known, and reads nothing itself.

import { imageDimensions, nameHint, sniff, type ImageFormat, type ImageSize } from "./previewSniff";
import type { FilePreviewSettings, StorageKind } from "./previewSettings";
import type { AppError, FileEntry, FileStamp, Protocol, TextEncoding } from "./types";

const KIBIBYTE = 1024;
const MEBIBYTE = 1024 * KIBIBYTE;

/** The most the backend returns in one read, whatever it is asked for. */
export const HARD_LIMIT_BYTES = 64 * MEBIBYTE;
/** Enough of a file to tell what it is by its signature. */
export const SIGNATURE_BYTES = 512;
/** How much of a file that is not text shows as hexadecimal. */
export const HEX_BYTES = 256;
/** A read for one file starts over at most this often as more is learned about the file. */
export const MAX_READS = 3;
/** Pictures with more pixels show only when asked for: decoded, each pixel takes four bytes. */
export const MAX_SHOWN_PIXELS = 40_000_000;
/** Pictures with more pixels are never decoded for a preview, which could take the web view
 * down with them. */
export const HARD_LIMIT_PIXELS = 120_000_000;
/** Thumbnails are drawn three at a time, so their pictures must be smaller. */
export const MAX_THUMBNAIL_PIXELS = 25_000_000;

export interface Caps {
  imageBytes: number;
  textBytes: number;
}

export function capsOf(settings: FilePreviewSettings): Caps {
  return { imageBytes: settings.maxImageMib * MEBIBYTE, textBytes: settings.textKib * KIBIBYTE };
}

/** Storage where every read is paid for. */
const METERED_PROTOCOLS: Protocol[] = ["s3", "backblazeB2", "googleDrive", "oneDrive", "box"];
const FTP_PROTOCOLS: Protocol[] = ["ftp", "ftps", "ftpsImplicit"];

/**
 * `protocol` is that of the server the bytes come from, which for a vault is the server it is
 * on; undefined for this computer.
 */
export function storageKind(protocol: Protocol | undefined): StorageKind {
  if (protocol === undefined) return "local";
  return METERED_PROTOCOLS.includes(protocol) ? "metered" : "server";
}

/**
 * The storage a pane's files are on: this computer, or a server, whose session may not be known
 * here yet. Until it is, its reads count as paid for.
 */
export function paneStorage(remote: boolean, protocol: Protocol | undefined): StorageKind {
  if (!remote) return "local";
  return protocol ? storageKind(protocol) : "metered";
}

/**
 * How far the sizes in a pane's listing can be believed before a file is read. `exact`: this
 * computer's own, listed just now. `listed`: a server's, where 0 may stand for a size the server
 * does not know. `remembered`: a listing kept from before, which the files may have changed
 * since, so that only the file as it is now decides anything.
 */
export type ListedSizes = "exact" | "listed" | "remembered";

/** A name for a selection, which is another selection when any of its items is. */
export function selectionKey(entries: readonly FileEntry[]): string {
  return JSON.stringify(entries.map((entry) => [entry.path, entry.size, entry.modified]));
}

/** FTP lists times to the minute and in the server's own time zone, so they prove nothing. */
export function listsExactTimes(protocol: Protocol | undefined): boolean {
  return protocol === undefined || !FTP_PROTOCOLS.includes(protocol);
}

export type Subject =
  | { kind: "nothing" }
  | { kind: "file"; entry: FileEntry }
  | { kind: "folder"; entry: FileEntry }
  /** A device, a socket or a link that leads nowhere: nothing to read. */
  | { kind: "special"; entry: FileEntry }
  | { kind: "selection"; entries: FileEntry[] };

/**
 * What the preview is about: several selected items together, else the one under the cursor.
 * Rows in `leftOut`, such as deleted files shown among the others, have nothing to read.
 */
export function subjectOf(
  cursor: FileEntry | undefined,
  given: FileEntry[],
  leftOut?: ReadonlySet<string>,
): Subject {
  const selected = leftOut ? given.filter((entry) => !leftOut.has(entry.path)) : given;
  if (selected.length > 1) return { kind: "selection", entries: selected };
  if (cursor && leftOut?.has(cursor.path)) return { kind: "nothing" };
  const entry = cursor ?? selected[0];
  if (!entry) return { kind: "nothing" };
  if (entry.kind === "dir" || entry.linkTarget === "dir") return { kind: "folder", entry };
  if (entry.kind === "file" || entry.linkTarget === "file") return { kind: "file", entry };
  return { kind: "special", entry };
}

/** A link is listed with its own size, not that of the file it leads to. */
export function listsOwnSize(entry: FileEntry): boolean {
  return entry.kind === "file";
}

export type Step =
  | { step: "read"; length: number }
  | { step: "show"; view: "empty" }
  | { step: "show"; view: "image"; format: ImageFormat }
  /** `length` bytes from the start are the text to show; `partial` when the file has more. */
  | { step: "show"; view: "text"; encoding: TextEncoding; length: number; partial: boolean }
  | { step: "show"; view: "binary"; description: string | null }
  /**
   * A picture over the cap in bytes, or with `dimensions` over the cap in pixels. `canLoad`
   * unless it is over a limit no click lifts.
   */
  | { step: "show"; view: "tooLarge"; canLoad: boolean; dimensions: ImageSize | null };

function wholeImage(size: number, caps: Caps, loadAnyway: boolean): Step {
  if (size > HARD_LIMIT_BYTES) {
    return { step: "show", view: "tooLarge", canLoad: false, dimensions: null };
  }
  if (size > caps.imageBytes && !loadAnyway) {
    return { step: "show", view: "tooLarge", canLoad: true, dimensions: null };
  }
  return { step: "read", length: size };
}

/** How many pixels a picture has, as its header says; null where it does not say. */
function pixelsOf(bytes: Uint8Array, format: ImageFormat): number | null {
  const dimensions = imageDimensions(bytes, format);
  return dimensions && dimensions.width * dimensions.height;
}

/**
 * Whether a thumbnail may be drawn of a picture, going by its header. One whose header does not
 * say is left to the decoder, which refuses what it cannot make sense of.
 */
export function fitsThumbnail(bytes: Uint8Array, format: ImageFormat): boolean {
  return (pixelsOf(bytes, format) ?? 0) <= MAX_THUMBNAIL_PIXELS;
}

/** A whole picture: shown, unless its header says it has too many pixels to decode safely. */
function wholePicture(bytes: Uint8Array, format: ImageFormat, loadAnyway: boolean): Step {
  const dimensions = imageDimensions(bytes, format);
  const pixels = dimensions ? dimensions.width * dimensions.height : 0;
  if (pixels > HARD_LIMIT_PIXELS) {
    return { step: "show", view: "tooLarge", canLoad: false, dimensions };
  }
  if (pixels > MAX_SHOWN_PIXELS && !loadAnyway) {
    return { step: "show", view: "tooLarge", canLoad: true, dimensions };
  }
  return { step: "show", view: "image", format };
}

/**
 * The first read of a file, by its name and listed size alone: a picture whole, a file that
 * cannot be text only as far as its signature, anything else as far as text is shown. Where
 * `sizes` says the listed size may not be the file's, that size decides nothing by itself:
 * the file is read, and its size as it is now decides.
 */
export function firstStep(
  entry: FileEntry,
  caps: Caps,
  loadAnyway: boolean,
  sizes: ListedSizes = "exact",
): Step {
  if (!listsOwnSize(entry)) return { step: "read", length: caps.textBytes };
  if (entry.size === 0) {
    return sizes === "exact"
      ? { step: "show", view: "empty" }
      : { step: "read", length: caps.textBytes };
  }
  switch (nameHint(entry.name)) {
    case "image":
      if (sizes !== "remembered") return wholeImage(entry.size, caps, loadAnyway);
      // Whole as listed, which it most likely still is; a picture listed over the cap only
      // far enough to learn its size now.
      return { step: "read", length: entry.size <= caps.imageBytes ? entry.size : SIGNATURE_BYTES };
    case "opaque":
      return { step: "read", length: Math.min(entry.size, SIGNATURE_BYTES) };
    case "unknown":
      return { step: "read", length: Math.min(entry.size, caps.textBytes) };
  }
}

/**
 * What to do once `bytes` have come from the start of the file, which is `fileSize` bytes long
 * now: show them, or read again with more known about the file.
 */
export function nextStep(
  name: string,
  bytes: Uint8Array,
  fileSize: number,
  caps: Caps,
  loadAnyway: boolean,
): Step {
  const whole = bytes.length >= fileSize;
  const head = bytes.length > caps.textBytes ? bytes.subarray(0, caps.textBytes) : bytes;
  const sniffed = sniff(head, name, whole && head.length === bytes.length);
  switch (sniffed.kind) {
    case "empty":
      return { step: "show", view: "empty" };
    case "image":
      return whole
        ? wholePicture(bytes, sniffed.format, loadAnyway)
        : wholeImage(fileSize, caps, loadAnyway);
    case "text": {
      const wanted = Math.min(fileSize, caps.textBytes);
      if (bytes.length < wanted) return { step: "read", length: wanted };
      const { encoding } = sniffed;
      const partial = head.length < fileSize;
      return { step: "show", view: "text", encoding, length: head.length, partial };
    }
    case "binary":
      return { step: "show", view: "binary", description: sniffed.description };
  }
}

/**
 * Whether the file read is no longer the one its folder listed, by its size and, where the
 * listing's times can be trusted, its time. A server's 0 tells nothing about the size.
 */
export function changedSinceListed(
  entry: FileEntry,
  stamp: FileStamp,
  exactTimes: boolean,
  sizes: ListedSizes = "exact",
): boolean {
  if (!listsOwnSize(entry)) return false;
  const sizeUnknown = sizes === "listed" && entry.size === 0;
  if (!sizeUnknown && entry.size !== stamp.size) return true;
  if (!exactTimes || entry.modified === null || stamp.modified === null) return false;
  return Math.abs(stamp.modified / 1000 - entry.modified) > 1;
}

/**
 * Why a preview could not be read, as far as the pane tells them apart. A connection is "busy"
 * with another request for a moment, or "lent" to transfers for as long as they run.
 */
export type Problem = "denied" | "gone" | "lost" | "locked" | "busy" | "lent" | "other";

export function problemOf(error: AppError, inVault: boolean): Problem {
  switch (error.kind) {
    case "permissionDenied":
      return "denied";
    case "notFound":
      return "gone";
    case "busy":
      return "busy";
    case "lent":
      return "lent";
    case "sessionNotFound":
      // A vault's session ends when the vault is locked.
      return inVault ? "locked" : "lost";
    case "disconnected":
    case "connection":
    case "timeout":
      return "lost";
    default:
      return "other";
  }
}

/** Whether an item is a file whose name says it is a picture the preview shows. */
export function isPreviewablePicture(entry: FileEntry): boolean {
  const isFile = entry.kind === "file" || entry.linkTarget === "file";
  return isFile && nameHint(entry.name) === "image";
}

/** Whether a file may be a picture the grid can draw, going by what its folder lists. */
export function mayHaveThumbnail(entry: FileEntry, caps: Caps): boolean {
  if (!isPreviewablePicture(entry)) return false;
  return !listsOwnSize(entry) || (entry.size > 0 && entry.size <= caps.imageBytes);
}

export interface Summary {
  folders: number;
  files: number;
  /** The files' sizes together; folders count for nothing. */
  bytes: number;
  /** Files that may be pictures, in the order given. */
  pictures: FileEntry[];
}

/** What a folder holds or a selection is made of. */
export function summarize(entries: readonly FileEntry[], caps: Caps): Summary {
  const summary: Summary = { folders: 0, files: 0, bytes: 0, pictures: [] };
  for (const entry of entries) {
    if (entry.kind === "dir" || entry.linkTarget === "dir") {
      summary.folders++;
      continue;
    }
    summary.files++;
    if (listsOwnSize(entry)) summary.bytes += entry.size;
    if (mayHaveThumbnail(entry, caps)) summary.pictures.push(entry);
  }
  return summary;
}
