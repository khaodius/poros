// How far a picture in the preview zooms, and where the view goes as it does. Scales are of the
// picture's own size: 1 shows each of its pixels as one pixel of the page.

export interface PictureSize {
  width: number;
  height: number;
}

/** The space left around a picture in the preview, on every side. */
export const PICTURE_MARGIN = 12;
/** As far as the picture zooms in, unless fitting it in the pane takes more. */
export const MAX_ZOOM = 16;
/** A picture fitted in the pane zooms in to at least this many times that size. */
const MIN_ZOOM_RANGE = 4;
/** The scales the zoom buttons and keys go through, besides the one that fits the pane. */
const ZOOM_STEPS = [0.05, 0.1, 0.25, 1 / 3, 0.5, 2 / 3, 1, 1.5, 2, 3, 4, 6, 8, 12, 16];
/** A step of the buttons and keys goes at least this much further, so none is too small to see. */
const MIN_STEP_RATIO = 1.15;
/** How much a pixel of touchpad scrolling zooms. */
const WHEEL_ZOOM_PER_PIXEL = 0.006;
/**
 * One wheel event zooms by no more than this. A mouse wheel's notch, which systems report as
 * anything from 40 to 120 pixels, zooms by about as much on every one of them.
 */
const MAX_WHEEL_FACTOR = 1.25;
/** Pinching on a touchpad reaches the page as a wheel with Ctrl held, as fine as it is pinched. */
const PINCH_ZOOM_PER_PIXEL = 0.01;
const MAX_PINCH_FACTOR = 2;
/** Wheel distances given in lines or pages, in pixels. */
const LINE_PIXELS = 16;
const PAGE_PIXELS = 400;
/** Scales this close are the same, so a step never stops a hair short of the one it aims at. */
const SAME_SCALE = 1e-3;

/**
 * The scale that fits a picture in `room`, the space the preview gives it, with a margin all
 * round. Only a drawing, which scales without losing detail, grows past its own size.
 */
export function fitScale(own: PictureSize, room: PictureSize, grow: boolean): number | null {
  const width = room.width - 2 * PICTURE_MARGIN;
  const height = room.height - 2 * PICTURE_MARGIN;
  if (own.width <= 0 || own.height <= 0 || width <= 0 || height <= 0) return null;
  const fits = Math.min(width / own.width, height / own.height);
  return grow ? fits : Math.min(1, fits);
}

export interface ZoomLimits {
  min: number;
  max: number;
}

/**
 * How far a picture that fits the pane at `fit` zooms out and in: never smaller than it fits,
 * or than its own size where that is smaller still, and at least a few times larger.
 */
export function zoomLimits(fit: number): ZoomLimits {
  return { min: Math.min(fit, 1), max: Math.max(MAX_ZOOM, fit * MIN_ZOOM_RANGE) };
}

export function clampZoom(scale: number, limits: ZoomLimits): number {
  return Math.min(limits.max, Math.max(limits.min, scale));
}

export function sameScale(first: number, second: number): boolean {
  return Math.abs(first - second) <= SAME_SCALE * Math.max(first, second);
}

/**
 * The next scale from `scale` in `direction` (1 in, -1 out): the next of the usual steps far
 * enough on to see, or the scale that fits the pane where that comes first. Short of a limit
 * with no step left, it goes to the limit.
 */
export function stepZoom(scale: number, direction: 1 | -1, fit: number): number {
  const limits = zoomLimits(fit);
  const stops = [...ZOOM_STEPS, fit]
    .filter((stop) => stop >= limits.min && stop <= limits.max)
    .sort((left, right) => left - right);
  const further = (stop: number) =>
    !sameScale(stop, scale) && (direction === 1 ? stop > scale : stop < scale);
  // The scale that fits the pane is never passed over, however near it is.
  const visible = (stop: number) =>
    sameScale(stop, fit) ||
    (direction === 1 ? stop >= scale * MIN_STEP_RATIO : stop <= scale / MIN_STEP_RATIO);
  const ahead = stops.filter((stop) => further(stop) && visible(stop));
  const next = direction === 1 ? ahead[0] : ahead[ahead.length - 1];
  return next ?? (direction === 1 ? limits.max : limits.min);
}

/**
 * How much one wheel event zooms by: up and away from the user zooms in. `deltaMode` is the
 * event's own, 0 for pixels, 1 for lines and 2 for pages; `pinch` for a touchpad pinch.
 */
export function wheelZoomFactor(deltaY: number, deltaMode: number, pinch: boolean): number {
  const pixels = deltaY * (deltaMode === 1 ? LINE_PIXELS : deltaMode === 2 ? PAGE_PIXELS : 1);
  const factor = Math.exp(-pixels * (pinch ? PINCH_ZOOM_PER_PIXEL : WHEEL_ZOOM_PER_PIXEL));
  const most = pinch ? MAX_PINCH_FACTOR : MAX_WHEEL_FACTOR;
  return Math.min(most, Math.max(1 / most, factor));
}

/**
 * Along one side of the view: how long the scrolled area is for a picture `length` long, and
 * where the picture starts in it. A picture smaller than the view sits in its middle.
 */
export function pictureSpan(view: number, length: number): { extent: number; start: number } {
  const extent = Math.max(view, length + 2 * PICTURE_MARGIN);
  return { extent, start: (extent - length) / 2 };
}

export interface Axis {
  /** The length of the view. */
  view: number;
  /** How far the view is scrolled. */
  scroll: number;
  /** Where in the view the point to keep is, as the pointer, or the middle for a key. */
  anchor: number;
}

/**
 * Where the view scrolls to, along one side, as the picture grows from `before` to `after`
 * pixels long: the point of the picture at `anchor` stays there, as far as the scrolled area
 * reaches.
 */
export function anchoredScroll(axis: Axis, before: number, after: number): number {
  const { view, scroll, anchor } = axis;
  const was = pictureSpan(view, before);
  const share = before > 0 ? (scroll + anchor - was.start) / before : 0.5;
  const kept = Math.min(1, Math.max(0, share));
  const now = pictureSpan(view, after);
  const wanted = now.start + kept * after - anchor;
  return Math.min(now.extent - view, Math.max(0, wanted));
}
