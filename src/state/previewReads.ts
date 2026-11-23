// What the preview panes of this window read, and keep of it: file previews and thumbnails in
// bounded caches, and the few thumbnail requests that run at a time.

import { remoteSourceKey } from "../lib/fileSource";
import { filePreview, local, toAppError } from "../lib/ipc";
import { BoundedCache, connectionPrefix, previewKey } from "../lib/previewCache";
import { fitWithin } from "../lib/previewLayout";
import { loadFileView, viewWeight, type FileView } from "../lib/previewLoad";
import { fitsThumbnail, listsOwnSize, type Caps, type ListedSizes } from "../lib/previewPlan";
import { PreviewCancelled, RequestQueue } from "../lib/previewScheduler";
import type { StorageKind } from "../lib/previewSettings";
import { IMAGE_MIME_TYPES, sniff, type ImageFormat } from "../lib/previewSniff";
import type { DirListing, DocumentLocation, FileEntry, PreviewRead } from "../lib/types";

const MEBIBYTE = 1024 * 1024;
/** Thumbnails fetched at once, across every pane. */
const THUMBNAIL_REQUESTS = 3;
/** Drawn for screens up to this dense; denser ones scale the thumbnail up a little. */
const MAX_THUMBNAIL_DENSITY = 2;
/** A folder's listing is asked for again after this long, in case the folder changed. */
const LISTING_LIFETIME_MILLIS = 30_000;

/** Where a pane's files are, as far as reading them for a preview goes. */
export interface PreviewPlace {
  /** Tells connections apart in the caches. */
  connection: string;
  /** Absent for this computer. */
  sessionId?: string;
  storage: StorageKind;
  /** The listing's times can be compared with a file's own. */
  exactTimes: boolean;
  inVault: boolean;
  /**
   * How far the pane's listing can be believed before a file is read. Nothing kept from before
   * is shown for a remembered listing: only a read tells whether a file is still as it was.
   */
  sizes: ListedSizes;
}

export function locationOf(place: PreviewPlace, path: string): DocumentLocation {
  return place.sessionId
    ? { side: "remote", sessionId: place.sessionId, path }
    : { side: "local", path };
}

/**
 * The backend names a read by its window and this name. The random part tells the reads of this
 * page from those it made before the window was reloaded, which may still be remembered there.
 */
const WINDOW_READS = Math.random().toString(36).slice(2, 10);
let readCount = 0;

/** Reads the start of a file, stopping the backend's read too when `signal` says so. */
export async function readStart(
  location: DocumentLocation,
  length: number,
  signal: AbortSignal,
): Promise<PreviewRead> {
  if (signal.aborted) throw new PreviewCancelled();
  const requestId = `${WINDOW_READS}-${++readCount}`;
  const stop = () => void filePreview.cancel(requestId).catch(() => undefined);
  signal.addEventListener("abort", stop, { once: true });
  try {
    return await filePreview.read({ requestId, location, offset: 0, length });
  } catch (caught) {
    const error = toAppError(caught);
    throw signal.aborted || error.kind === "cancelled" ? new PreviewCancelled() : error;
  } finally {
    signal.removeEventListener("abort", stop);
  }
}

const fileViews = new BoundedCache<FileView>({ maxEntries: 48, maxWeight: 64 * MEBIBYTE });

/** How often each connection's previews were forgotten: a read that ends after that keeps
 * nothing of what it read. */
const forgetCounts = new Map<string, number>();

function forgetCount(connection: string): number {
  return forgetCounts.get(connection) ?? 0;
}

function viewKey(place: PreviewPlace, entry: FileEntry, caps: Caps, loadAnyway: boolean): string {
  return previewKey(place.connection, entry, `${caps.imageBytes}:${caps.textBytes}:${loadAnyway}`);
}

export function cachedFileView(
  place: PreviewPlace,
  entry: FileEntry,
  caps: Caps,
  loadAnyway: boolean,
): FileView | undefined {
  if (place.sizes === "remembered") return undefined;
  return fileViews.get(viewKey(place, entry, caps, loadAnyway));
}

/** Reads a file and works out what to show of it. What was read is kept for the next time,
 * unless the file turned out to have changed since its folder was listed. */
export async function readFileView(
  place: PreviewPlace,
  entry: FileEntry,
  caps: Caps,
  loadAnyway: boolean,
  signal: AbortSignal,
): Promise<FileView> {
  const location = locationOf(place, entry.path);
  const since = forgetCount(place.connection);
  const view = await loadFileView(
    entry,
    {
      read: (length, readSignal) => readStart(location, length, readSignal),
      caps,
      loadAnyway,
      exactTimes: place.exactTimes,
      sizes: place.sizes,
    },
    signal,
  );
  const changed = "changed" in view && view.changed;
  if (!changed && forgetCount(place.connection) === since) {
    fileViews.set(viewKey(place, entry, caps, loadAnyway), view, viewWeight(view));
  }
  return view;
}

interface KeptListing {
  listing: DirListing;
  listedAt: number;
}

const folderListings = new BoundedCache<KeptListing>({ maxEntries: 24, maxWeight: 50_000 });

export function cachedFolder(place: PreviewPlace, folder: FileEntry): DirListing | undefined {
  if (place.sizes === "remembered") return undefined;
  const kept = folderListings.get(previewKey(place.connection, folder));
  if (!kept || Date.now() - kept.listedAt > LISTING_LIFETIME_MILLIS) return undefined;
  return kept.listing;
}

/** Lists a folder for its summary. A server's folder is not listed while transfers use the
 * connection. */
export async function listFolder(place: PreviewPlace, folder: FileEntry): Promise<DirListing> {
  const since = forgetCount(place.connection);
  const listing = place.sessionId
    ? await filePreview.list(place.sessionId, folder.path)
    : await local.list(folder.path);
  if (forgetCount(place.connection) !== since) return listing;
  folderListings.set(
    previewKey(place.connection, folder),
    { listing, listedAt: Date.now() },
    Math.max(1, listing.entries.length),
  );
  return listing;
}

export interface Thumbnail {
  /** A small picture to show, or null for a file that turned out not to be a picture. */
  url: string | null;
  /** The size of the picture itself. */
  width: number;
  height: number;
}

const thumbnails = new BoundedCache<Thumbnail>(
  { maxEntries: 600, maxWeight: 32 * MEBIBYTE },
  (thumbnail) => {
    if (thumbnail.url) URL.revokeObjectURL(thumbnail.url);
  },
);
const thumbnailRequests = new RequestQueue(THUMBNAIL_REQUESTS);

function thumbnailKey(place: PreviewPlace, entry: FileEntry, box: number): string {
  return previewKey(place.connection, entry, `thumbnail:${box}`);
}

export function cachedThumbnail(
  place: PreviewPlace,
  entry: FileEntry,
  box: number,
): Thumbnail | undefined {
  if (place.sizes === "remembered") return undefined;
  return thumbnails.get(thumbnailKey(place, entry, box));
}

/** Forgets a thumbnail whose picture would not show, so it is drawn again when next asked for. */
export function forgetThumbnail(place: PreviewPlace, entry: FileEntry, box: number): void {
  thumbnails.delete(thumbnailKey(place, entry, box));
}

async function decoded(blob: Blob): Promise<{
  picture: CanvasImageSource;
  width: number;
  height: number;
  release: () => void;
}> {
  try {
    // Decoded off the main thread, turned as the picture's own orientation says.
    const bitmap = await createImageBitmap(blob);
    const { width, height } = bitmap;
    return { picture: bitmap, width, height, release: () => bitmap.close() };
  } catch {
    // Drawings, and anything else only an image element decodes. Nothing in the file runs:
    // an image element never runs scripts or loads what the picture points at.
    const url = URL.createObjectURL(blob);
    const image = new Image();
    image.decoding = "async";
    image.src = url;
    try {
      await image.decode();
    } catch (caught) {
      URL.revokeObjectURL(url);
      throw caught;
    }
    return {
      picture: image,
      width: image.naturalWidth,
      height: image.naturalHeight,
      release: () => URL.revokeObjectURL(url),
    };
  }
}

async function scaledDown(
  picture: CanvasImageSource,
  width: number,
  height: number,
): Promise<Blob | null> {
  try {
    if (typeof OffscreenCanvas !== "undefined") {
      const canvas = new OffscreenCanvas(width, height);
      const context = canvas.getContext("2d");
      if (!context) return null;
      context.imageSmoothingQuality = "high";
      context.drawImage(picture, 0, 0, width, height);
      return await canvas.convertToBlob({ type: "image/webp", quality: 0.85 });
    }
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) return null;
    context.imageSmoothingQuality = "high";
    context.drawImage(picture, 0, 0, width, height);
    return await new Promise((resolve) => canvas.toBlob(resolve, "image/webp", 0.85));
  } catch {
    // Some web views will not hand back a canvas a drawing was painted on.
    return null;
  }
}

/** Draws a picture at thumbnail size. The weight is what the thumbnail keeps in memory. */
async function drawThumbnail(
  bytes: Uint8Array,
  format: ImageFormat,
  box: number,
): Promise<{ thumbnail: Thumbnail; weight: number }> {
  const blob = new Blob([bytes as BlobPart], { type: IMAGE_MIME_TYPES[format] });
  const { picture, width, height, release } = await decoded(blob);
  try {
    const density = Math.min(MAX_THUMBNAIL_DENSITY, window.devicePixelRatio || 1);
    const size = fitWithin(width, height, Math.round(box * density));
    const small = await scaledDown(picture, size.width, size.height);
    // The picture itself where it could not be scaled down, as with some drawings.
    const shown = small ?? blob;
    return {
      thumbnail: { url: URL.createObjectURL(shown), width, height },
      weight: Math.max(1, shown.size),
    };
  } finally {
    release();
  }
}

/**
 * The thumbnail of a file its folder lists as a picture, read and drawn when its turn comes.
 * Rejects with `PreviewCancelled` when `cancelThumbnail` drops it first.
 */
export function requestThumbnail(
  place: PreviewPlace,
  entry: FileEntry,
  box: number,
  caps: Caps,
): Promise<Thumbnail> {
  const key = thumbnailKey(place, entry, box);
  return thumbnailRequests.request(key, async (signal) => {
    const since = forgetCount(place.connection);
    const length = listsOwnSize(entry) ? Math.min(entry.size, caps.imageBytes) : caps.imageBytes;
    const read = await readStart(locationOf(place, entry.path), length, signal);
    const sniffed = sniff(read.bytes, entry.name, true);
    const whole = read.bytes.length >= read.stamp.size;
    let drawn: { thumbnail: Thumbnail; weight: number } = {
      thumbnail: { url: null, width: 0, height: 0 },
      weight: 1,
    };
    // One with more pixels than decode safely, a few at a time, stays a plain icon.
    if (whole && sniffed.kind === "image" && fitsThumbnail(read.bytes, sniffed.format)) {
      drawn = await drawThumbnail(read.bytes, sniffed.format, box).catch(() => drawn);
    }
    if (forgetCount(place.connection) !== since) {
      if (drawn.thumbnail.url) URL.revokeObjectURL(drawn.thumbnail.url);
      throw new PreviewCancelled();
    }
    thumbnails.set(key, drawn.thumbnail, drawn.weight);
    return drawn.thumbnail;
  });
}

/** Drops a thumbnail that is no longer on show from the line, or stops its read. */
export function cancelThumbnail(place: PreviewPlace, entry: FileEntry, box: number): void {
  thumbnailRequests.cancel(thumbnailKey(place, entry, box));
}

/**
 * Forgets everything the previews read over a session, as it closes or its vault is locked: a
 * vault's files are kept here decrypted, and nothing of them may stay once the vault is locked.
 * Thumbnails still on their way are stopped, and their pictures let go.
 */
export function forgetSessionPreviews(sessionId: string): void {
  const connection = remoteSourceKey(sessionId);
  forgetCounts.set(connection, forgetCount(connection) + 1);
  const prefix = connectionPrefix(connection);
  thumbnailRequests.cancelMatching((key) => key.startsWith(prefix));
  fileViews.deletePrefixed(prefix);
  folderListings.deletePrefixed(prefix);
  thumbnails.deletePrefixed(prefix);
}
