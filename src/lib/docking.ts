// Geometry for dragging a pane around the dock: where the lifted pane floats, which group it is
// over, and whether it merges into that group or splits it.

import {
  allGroups,
  dockTab,
  groupOfTab,
  layoutPlacements,
  type DropSide,
  type Extent,
  type LayoutNode,
  type Placement,
} from "./layout";

export interface Point {
  x: number;
  y: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Rect {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Pixels between split children, the thickness of the resize handles. */
export const DOCK_GAP = 8;
/** Matches the .tab-bar height. */
export const TAB_BAR_HEIGHT = 31;
/** In halves of a group's size: a pane centered this close to a group's center merges into it
 * as a tab, and farther out it splits the group on the side it leans toward. */
const MERGE_ZONE = 0.5;
/** The lifted pane is never larger than this share of the dock, so the layout under it shows. */
const FLOATING_MAX = { width: 0.5, height: 0.6 };
/** How near an edge of the window, in pixels, a lifted pane starts turning into a window. */
const EDGE_ZONE = 80;

export function resolveExtent({ share, pixels }: Extent, total: number): number {
  return share * total + pixels;
}

export function placementRect(placement: Placement, dock: Rect): Rect {
  return {
    left: dock.left + resolveExtent(placement.left, dock.width),
    top: dock.top + resolveExtent(placement.top, dock.height),
    width: resolveExtent(placement.width, dock.width),
    height: resolveExtent(placement.height, dock.height),
  };
}

export interface GroupRect {
  groupId: string;
  rect: Rect;
}

export function groupRects(root: LayoutNode, dock: Rect): GroupRect[] {
  const placements = layoutPlacements(root, DOCK_GAP);
  return allGroups(root).map((entry) => ({
    groupId: entry.id,
    rect: placementRect(placements.get(entry.id)!, dock),
  }));
}

function distanceTo(rect: Rect, point: Point): number {
  const x = Math.max(rect.left - point.x, 0, point.x - (rect.left + rect.width));
  const y = Math.max(rect.top - point.y, 0, point.y - (rect.top + rect.height));
  return Math.hypot(x, y);
}

export function rectCenter({ left, top, width, height }: Rect): Point {
  return { x: left + width / 2, y: top + height / 2 };
}

/** The group under a point, or the closest one when the point falls in the gap between groups. */
export function groupNearest(groups: GroupRect[], point: Point): GroupRect | null {
  let nearest: GroupRect | null = null;
  let nearestDistance = Infinity;
  for (const entry of groups) {
    const distance = distanceTo(entry.rect, point);
    if (distance < nearestDistance) {
      nearest = entry;
      nearestDistance = distance;
    }
  }
  return nearest;
}

/** Where a pane centered on `point` drops into `target`: its middle merges, its edges split. */
export function dropSide(target: Rect, point: Point): DropSide {
  const center = rectCenter(target);
  const x = (point.x - center.x) / (target.width / 2);
  const y = (point.y - center.y) / (target.height / 2);
  if (Math.max(Math.abs(x), Math.abs(y)) < MERGE_ZONE) return "center";
  if (Math.abs(x) >= Math.abs(y)) return x < 0 ? "left" : "right";
  return y < 0 ? "top" : "bottom";
}

/** Whether two layouts hold the same tabs in the same arrangement, whatever their sizes. */
function sameArrangement(first: LayoutNode, second: LayoutNode): boolean {
  if (first.type === "group" || second.type === "group") {
    return (
      first.type === "group" &&
      second.type === "group" &&
      first.tabs.map((tab) => tab.id).join() === second.tabs.map((tab) => tab.id).join()
    );
  }
  return (
    first.direction === second.direction &&
    first.children.length === second.children.length &&
    first.children.every((child, index) => sameArrangement(child, second.children[index]))
  );
}

/** Whether dropping the tab there would change anything; merging into its own group never does. */
export function changesLayout(
  root: LayoutNode,
  tabId: string,
  groupId: string,
  side: DropSide,
): boolean {
  if (side === "center" && groupOfTab(root, tabId)?.id === groupId) return false;
  return !sameArrangement(dockTab(root, tabId, groupId, side), root);
}

/**
 * The size a pane takes while dragged by a point on its tab bar, and where that point sits in
 * it, scaled with the pane so the pointer keeps its place along the tab bar.
 */
export function floatingFrame(frame: Rect, dock: Rect, start: Point): { grab: Point; size: Size } {
  const size = {
    width: Math.min(frame.width, dock.width * FLOATING_MAX.width),
    height: Math.min(frame.height, dock.height * FLOATING_MAX.height),
  };
  const grab = {
    x: (start.x - frame.left) * (size.width / frame.width),
    y: Math.min(start.y - frame.top, size.height),
  };
  return { grab, size };
}

export function floatingRect({ grab, size }: { grab: Point; size: Size }, pointer: Point): Rect {
  return { left: pointer.x - grab.x, top: pointer.y - grab.y, ...size };
}

/** Eases from 0 to 1, slow at both ends. */
export function smoothstep(value: number): number {
  const t = Math.min(1, Math.max(0, value));
  return t * t * (3 - 2 * t);
}

/**
 * How far a lifted pane has turned into the window it opens as when dropped off Poros, from 0
 * to 1: it starts as the pointer nears an edge of the window and is done at the edge, where the
 * preview on the desktop takes over at the same size and place. Above the dock the title bar
 * is the whole ramp, which keeps the tab bars below it clear.
 */
export function tearingOut(pointer: Point, dock: Rect, viewport: Size): number {
  const near = (distance: number, zone: number) => 1 - distance / Math.max(1, zone);
  return smoothstep(
    Math.max(
      near(pointer.y, dock.top),
      near(pointer.x, EDGE_ZONE),
      near(viewport.width - pointer.x, EDGE_ZONE),
      near(viewport.height - pointer.y, EDGE_ZONE),
    ),
  );
}

/** The rect `t` of the way from one to another. */
export function between(from: Rect, to: Rect, t: number): Rect {
  const at = (start: number, end: number) => start + (end - start) * t;
  return {
    left: at(from.left, to.left),
    top: at(from.top, to.top),
    width: at(from.width, to.width),
    height: at(from.height, to.height),
  };
}
