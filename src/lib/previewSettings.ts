// The preview pane's settings, kept in the interface section: how much it reads, how large its
// thumbnails are, and whether it loads by itself on each kind of storage.

/** Where files are, as far as reading them for a preview goes. */
export type StorageKind = "local" | "server" | "metered";
export const STORAGE_KINDS: StorageKind[] = ["local", "server", "metered"];

/** A preview loads once the cursor rests on a file, or only on a click of "Show preview". */
export type PreviewLoad = "rest" | "click";
const PREVIEW_LOADS: PreviewLoad[] = ["rest", "click"];

export interface FilePreviewSettings {
  /** The largest picture fetched without asking, in MiB. */
  maxImageMib: number;
  /** How much of a text file is read, in KiB. */
  textKib: number;
  /** The longer side of a thumbnail, in pixels. */
  thumbnailSize: number;
  load: Record<StorageKind, PreviewLoad>;
}

export const PREVIEW_LIMITS = {
  /** No more than the backend returns in one read. */
  maxImageMib: { min: 1, max: 64 },
  textKib: { min: 16, max: 4096 },
  thumbnailSize: { min: 48, max: 192 },
};

export const DEFAULT_FILE_PREVIEW: FilePreviewSettings = {
  maxImageMib: 16,
  textKib: 256,
  thumbnailSize: 96,
  // Storage that charges for each read waits to be asked.
  load: { local: "rest", server: "rest", metered: "click" },
};

function clamp(value: number, { min, max }: { min: number; max: number }, fallback: number) {
  return Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : fallback;
}

/** Brings stored values within their limits, so a hand-edited settings file still loads. */
export function sanitizeFilePreview(stored: FilePreviewSettings): FilePreviewSettings {
  const defaults = DEFAULT_FILE_PREVIEW;
  const load = { ...defaults.load };
  for (const kind of STORAGE_KINDS) {
    if (PREVIEW_LOADS.includes(stored.load[kind])) load[kind] = stored.load[kind];
  }
  return {
    maxImageMib: clamp(stored.maxImageMib, PREVIEW_LIMITS.maxImageMib, defaults.maxImageMib),
    textKib: clamp(stored.textKib, PREVIEW_LIMITS.textKib, defaults.textKib),
    thumbnailSize: clamp(
      stored.thumbnailSize,
      PREVIEW_LIMITS.thumbnailSize,
      defaults.thumbnailSize,
    ),
    load,
  };
}
