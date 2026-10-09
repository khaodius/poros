// Where the Poros windows are on screen, so a tab dragged out of one window can drop into the
// window under the pointer.

import {
  availableMonitors,
  cursorPosition,
  getAllWindows,
  getCurrentWindow,
  type Window,
} from "@tauri-apps/api/window";
import { DRAG_PREVIEW_WINDOW, MAIN_WINDOW } from "./ipc";

/** A window's content area on screen, in physical pixels. */
export interface WindowArea {
  label: string;
  left: number;
  top: number;
  width: number;
  height: number;
  scale: number;
}

/** A part of the screen, in physical pixels. */
export interface ScreenRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** A point inside a window, in that window's own coordinates. */
export interface WindowPoint {
  label: string;
  x: number;
  y: number;
}

/**
 * A point on the screen in physical pixels. Windows are placed in these: logical pixels differ
 * from one monitor to the next, and a pointer event's screenX means something else again on a
 * monitor scaled differently from the window.
 */
export interface ScreenPoint {
  x: number;
  y: number;
}

/** A dragging pointer: in this window's coordinates, and where it is on the screen. */
export interface DragPointer {
  clientX: number;
  clientY: number;
  screen: ScreenPoint;
}

/** A screen less its taskbars and docks, and how much it scales logical pixels. */
export interface WorkArea extends ScreenRect {
  scale: number;
}

let thisWindow: WindowArea | null = null;
let otherWindows: WindowArea[] = [];
let workAreas: WorkArea[] = [];

async function areaOf(entry: Window): Promise<WindowArea | null> {
  const [position, size, scale, visible, minimized] = await Promise.all([
    entry.innerPosition(),
    entry.innerSize(),
    entry.scaleFactor(),
    entry.isVisible(),
    entry.isMinimized(),
  ]);
  if (!visible || minimized) return null;
  const { x: left, y: top } = position;
  return { label: entry.label, left, top, width: size.width, height: size.height, scale };
}

/** Finds the open windows and screens when a drag starts; they stay put while it lasts. */
export async function measureWindows(): Promise<void> {
  thisWindow = null;
  otherWindows = [];
  workAreas = [];
  try {
    const current = getCurrentWindow().label;
    const [all, monitors] = await Promise.all([getAllWindows(), availableMonitors()]);
    const workspaces = all.filter((entry) => entry.label !== DRAG_PREVIEW_WINDOW);
    const areas = await Promise.all(workspaces.map(areaOf));
    thisWindow = areas.find((area) => area?.label === current) ?? null;
    otherWindows = areas.filter(
      (area): area is WindowArea => area !== null && area.label !== current,
    );
    workAreas = monitors.map(({ workArea: { position, size }, scaleFactor }) => ({
      left: position.x,
      top: position.y,
      width: size.width,
      height: size.height,
      scale: scaleFactor,
    }));
  } catch {
    thisWindow = null;
    otherWindows = [];
    workAreas = [];
  }
}

/**
 * Where a window held by the pointer at `grab` (logical pixels from its top left corner) has its
 * top left corner, in physical pixels: it follows the pointer, except that its top bar never
 * starts above or left of the screen the pointer is on.
 */
export function heldWindowCorner(
  areas: WorkArea[],
  pointer: ScreenPoint,
  grab: { x: number; y: number },
  fallbackScale: number,
): ScreenPoint {
  const area = areas.find(
    (entry) =>
      pointer.x >= entry.left &&
      pointer.x < entry.left + entry.width &&
      pointer.y >= entry.top &&
      pointer.y < entry.top + entry.height,
  );
  const scale = area?.scale ?? fallbackScale;
  const corner = {
    x: Math.round(pointer.x - grab.x * scale),
    y: Math.round(pointer.y - grab.y * scale),
  };
  if (!area) return corner;
  return { x: Math.max(corner.x, area.left), y: Math.max(corner.y, area.top) };
}

/** Where a window held by the pointer at `grab` opens, in physical pixels. */
export function landingCorner(pointer: ScreenPoint, grab: { x: number; y: number }): ScreenPoint {
  return heldWindowCorner(workAreas, pointer, grab, window.devicePixelRatio || 1);
}

/**
 * The window at a point on screen, in physical pixels. Torn-out windows open above the main
 * window, so where the two overlap the torn-out one is the window the pointer is over.
 */
export function windowAt(areas: WindowArea[], x: number, y: number): WindowPoint | null {
  const hits = areas.filter(
    (area) =>
      x >= area.left && x < area.left + area.width && y >= area.top && y < area.top + area.height,
  );
  const hit = hits.find((area) => area.label !== MAIN_WINDOW) ?? hits[0];
  if (!hit) return null;
  return { label: hit.label, x: (x - hit.left) / hit.scale, y: (y - hit.top) / hit.scale };
}

/** The other window under a point on the screen. */
export function windowUnder({ x, y }: ScreenPoint): WindowPoint | null {
  return windowAt(otherWindows, x, y);
}

/**
 * A pointer event's position, with its place on the screen worked out from this window's own
 * position rather than the event's screenX, which a differently scaled monitor skews.
 */
export function dragPointer(event: {
  clientX: number;
  clientY: number;
  screenX: number;
  screenY: number;
}): DragPointer {
  const { clientX, clientY } = event;
  if (!thisWindow) {
    const scale = window.devicePixelRatio || 1;
    return { clientX, clientY, screen: { x: event.screenX * scale, y: event.screenY * scale } };
  }
  const { left, top, scale } = thisWindow;
  return { clientX, clientY, screen: { x: left + clientX * scale, y: top + clientY * scale } };
}

/**
 * The system cursor as a pointer event in this window would report it. Some webviews stop
 * reporting the pointer once it leaves the window, even while a button is held.
 */
export async function cursorPointer(): Promise<DragPointer | null> {
  if (!thisWindow) return null;
  try {
    const { x, y } = await cursorPosition();
    const { left, top, scale } = thisWindow;
    return { clientX: (x - left) / scale, clientY: (y - top) / scale, screen: { x, y } };
  } catch {
    return null;
  }
}
