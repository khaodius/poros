// Gliding scroll for the mouse wheel: each step of the wheel eases the list to where the step
// takes it, and steps that come while it glides add to where it is going.

/** How long the list glides after a step of the wheel, in milliseconds. */
export const GLIDE_MILLIS = 200;
/**
 * Wheel steps scroll at least this far. Touchpads and wheels that turn freely send many small
 * movements, already smooth, which scroll as they come.
 */
export const WHEEL_STEP_MIN = 32;
/** Pixels in a line, for wheels that count lines rather than pixels. */
const LINE_PIXELS = 40;

export interface Glide {
  from: number;
  to: number;
  /** When it started, in milliseconds on the page's clock. */
  start: number;
}

/** How far a wheel event scrolls, in pixels down; `pageHeight` is the height of the list. */
export function wheelDistance(
  event: Pick<WheelEvent, "deltaY" | "deltaMode">,
  pageHeight: number,
): number {
  if (event.deltaMode === 1) return event.deltaY * LINE_PIXELS;
  if (event.deltaMode === 2) return event.deltaY * pageHeight;
  return event.deltaY;
}

/** Fast at first, slowing to a stop. */
export function easeOut(progress: number): number {
  const clamped = Math.min(1, Math.max(0, progress));
  return 1 - (1 - clamped) ** 3;
}

/** Where a glide has got to at `now`, and whether it is over. */
export function glideAt(glide: Glide, now: number): { position: number; done: boolean } {
  const progress = (now - glide.start) / GLIDE_MILLIS;
  if (progress >= 1) return { position: glide.to, done: true };
  return { position: glide.from + (glide.to - glide.from) * easeOut(progress), done: false };
}

/**
 * The glide for a wheel step of `distance` from `position`, going on from `current` if one is
 * under way, and stopping at either end of the list, `limit` being the furthest it scrolls.
 */
export function nextGlide(
  current: Glide | undefined,
  position: number,
  distance: number,
  limit: number,
  now: number,
): Glide {
  const to = Math.min(limit, Math.max(0, (current?.to ?? position) + distance));
  return { from: position, to, start: now };
}
