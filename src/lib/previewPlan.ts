// What the preview pane shows for the item under the cursor and how much of a file it reads to
// find out. Everything here decides from what is known, and reads nothing itself.

import { nameHint, sniff, type ImageFormat } from "./previewSniff";
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

/** What the preview is about: several selected items together, else the one under the cursor. */
export function subjectOf(cursor: FileEntry | undefined, selected: FileEntry[]): Subject {
  if (selected.length > 1) return { kind: "selection", entries: selected };
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
  /** A picture over the cap. `canLoad` unless it is more than one read can return. */
  | { step: "show"; view: "tooLarge"; canLoad: boolean };

function wholeImage(size: number, caps: Caps, loadAnyway: boolean): Step {
  if (size > HARD_LIMIT_BYTES) return { step: "show", view: "tooLarge", canLoad: false };
  if (size > caps.imageBytes && !loadAnyway) {
    return { step: "show", view: "tooLarge", canLoad: true };
  }
  return { step: "read", length: size };
}

/**
 * The first read of a file, by its name and listed size alone: a picture whole, a file that
 * cannot be text only as far as its signature, anything else as far as text is shown.
 */
export function firstStep(entry: FileEntry, caps: Caps, loadAnyway: boolean): Step {
  if (!listsOwnSize(entry)) return { step: "read", length: caps.textBytes };
  if (entry.size === 0) return { step: "show", view: "empty" };
  switch (nameHint(entry.name)) {
    case "image":
      return wholeImage(entry.size, caps, loadAnyway);
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
        ? { step: "show", view: "image", format: sniffed.format }
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
 * listing's times can be trusted, its time.
 */
export function changedSinceListed(
  entry: FileEntry,
  stamp: FileStamp,
  exactTimes: boolean,
): boolean {
  if (!listsOwnSize(entry)) return false;
  if (entry.size !== stamp.size) return true;
  if (!exactTimes || entry.modified === null || stamp.modified === null) return false;
  return Math.abs(stamp.modified / 1000 - entry.modified) > 1;
}

/** Why a preview could not be read, as far as the pane tells them apart. */
export type Problem = "denied" | "gone" | "lost" | "locked" | "busy" | "other";

export function problemOf(error: AppError, inVault: boolean): Problem {
  switch (error.kind) {
    case "permissionDenied":
      return "denied";
    case "notFound":
      return "gone";
    case "busy":
      return "busy";
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

/** Whether a file may be a picture the grid can draw, going by what its folder lists. */
export function mayHaveThumbnail(entry: FileEntry, caps: Caps): boolean {
  const isFile = entry.kind === "file" || entry.linkTarget === "file";
  if (!isFile || nameHint(entry.name) !== "image") return false;
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
