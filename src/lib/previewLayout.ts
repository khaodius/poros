// Where a pane's preview goes and how much of the pane it takes. Every pane opens its preview
// with the share chosen last, as the panel below the panes does.

const STORAGE_KEY = "poros.previewShare";
export const DEFAULT_PREVIEW_SHARE = 0.4;
const SHARE_LIMITS = { min: 0.2, max: 0.7 };
/** A pane narrower than this has its preview below the list instead of beside it. */
export const SIDE_BY_SIDE_MIN_WIDTH = 520;

export type PreviewSide = "side" | "below";

export function previewSide(paneWidth: number): PreviewSide {
  return paneWidth >= SIDE_BY_SIDE_MIN_WIDTH ? "side" : "below";
}

export function clampPreviewShare(share: number): number {
  return Math.min(SHARE_LIMITS.max, Math.max(SHARE_LIMITS.min, share));
}

/**
 * The preview's share of the pane with the divider at `pointer`, measured from where the pane
 * starts along the axis the two are laid out on.
 */
export function shareAtPointer(pointer: number, paneStart: number, paneLength: number): number {
  if (paneLength <= 0) return DEFAULT_PREVIEW_SHARE;
  return clampPreviewShare(1 - (pointer - paneStart) / paneLength);
}

export function storedPreviewShare(): number {
  try {
    const stored = Number(localStorage.getItem(STORAGE_KEY));
    return stored > 0 ? clampPreviewShare(stored) : DEFAULT_PREVIEW_SHARE;
  } catch {
    return DEFAULT_PREVIEW_SHARE;
  }
}

export function storePreviewShare(share: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(share));
  } catch {
    // The preview then opens at its default size next time.
  }
}

/**
 * The size a picture `width` by `height` is shown at to fit a space `room` across, keeping its
 * shape. Only a drawing, which scales without losing detail, may `grow` past its own size.
 */
export function fitInside(
  width: number,
  height: number,
  room: { width: number; height: number },
  grow: boolean,
) {
  if (width <= 0 || height <= 0 || room.width <= 0 || room.height <= 0) return null;
  const fits = Math.min(room.width / width, room.height / height);
  const scale = grow ? fits : Math.min(1, fits);
  return {
    width: Math.max(1, Math.floor(width * scale)),
    height: Math.max(1, Math.floor(height * scale)),
  };
}

/** The size a picture `width` by `height` is drawn at to fit in a square `box` wide. */
export function fitWithin(width: number, height: number, box: number) {
  const longest = Math.max(width, height);
  if (longest <= 0) return { width: box, height: box };
  const scale = Math.min(1, box / longest);
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}
