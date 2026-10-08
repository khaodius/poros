// Where the Poros windows are on screen, so a tab dragged out of one window can drop into the
// window under the pointer.

import {
  cursorPosition,
  getAllWindows,
  getCurrentWindow,
  type Window,
} from "@tauri-apps/api/window";
import { MAIN_WINDOW } from "./ipc";

/** A window's content area on screen, in physical pixels. */
export interface WindowArea {
  label: string;
  left: number;
  top: number;
  width: number;
  height: number;
  scale: number;
}

/** A point inside a window, in that window's own coordinates. */
export interface WindowPoint {
  label: string;
  x: number;
  y: number;
}

/** A pointer position as a pointer event reports it. */
export interface ScreenPointer {
  clientX: number;
  clientY: number;
  screenX: number;
  screenY: number;
}

let thisWindow: WindowArea | null = null;
let otherWindows: WindowArea[] = [];

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

/** Finds the open windows when a drag starts; they stay put while it lasts. */
export async function measureWindows(): Promise<void> {
  thisWindow = null;
  otherWindows = [];
  try {
    const current = getCurrentWindow().label;
    const areas = await Promise.all((await getAllWindows()).map(areaOf));
    thisWindow = areas.find((area) => area?.label === current) ?? null;
    otherWindows = areas.filter(
      (area): area is WindowArea => area !== null && area.label !== current,
    );
  } catch {
    thisWindow = null;
    otherWindows = [];
  }
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

/** The other window under a pointer event's screen position. */
export function windowUnder(screenX: number, screenY: number): WindowPoint | null {
  const scale = window.devicePixelRatio;
  return windowAt(otherWindows, screenX * scale, screenY * scale);
}

/**
 * The system cursor as a pointer event in this window would report it. Some webviews stop
 * reporting the pointer once it leaves the window, even while a button is held.
 */
export async function cursorPointer(): Promise<ScreenPointer | null> {
  if (!thisWindow) return null;
  try {
    const { x, y } = await cursorPosition();
    const { left, top, scale } = thisWindow;
    return {
      clientX: (x - left) / scale,
      clientY: (y - top) / scale,
      screenX: x / scale,
      screenY: y / scale,
    };
  } catch {
    return null;
  }
}
