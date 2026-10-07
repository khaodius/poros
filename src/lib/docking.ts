// Geometry for dragging a pane around the dock: where the lifted pane floats, which group it
// covers enough to make room for it, and on which side of that group it lands.

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
/** How much of a group the lifted pane covers, or of itself when it is the smaller, before
 * the group moves aside. */
export const COVER_SHARE = 0.5;
/** Over its own group, a pane must move this far off center, in halves of the group's size,
 * before it splits the group. */
const CENTER_ZONE = 0.3;
/** The lifted pane is never larger than this share of the dock, so the layout under it shows. */
const FLOATING_MAX = { width: 0.5, height: 0.6 };

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

function overlapArea(first: Rect, second: Rect): number {
  const width =
    Math.min(first.left + first.width, second.left + second.width) -
    Math.max(first.left, second.left);
  const height =
    Math.min(first.top + first.height, second.top + second.height) -
    Math.max(first.top, second.top);
  return width > 0 && height > 0 ? width * height : 0;
}

/** How much of the smaller rectangle the two share, from 0 to 1. */
export function coverage(first: Rect, second: Rect): number {
  const smaller = Math.min(first.width * first.height, second.width * second.height);
  return smaller > 0 ? overlapArea(first, second) / smaller : 0;
}

/** How far the floating rectangle's center is from the target's, in halves of its size. */
function centerOffset(target: Rect, floating: Rect): Point {
  return {
    x: (floating.left + floating.width / 2 - (target.left + target.width / 2)) / (target.width / 2),
    y:
      (floating.top + floating.height / 2 - (target.top + target.height / 2)) / (target.height / 2),
  };
}

export type EdgeSide = Exclude<DropSide, "center">;

/** The side of `target` the floating rectangle's center lies toward. */
export function sideWithin(target: Rect, floating: Rect): EdgeSide {
  const { x, y } = centerOffset(target, floating);
  if (Math.abs(x) >= Math.abs(y)) return x < 0 ? "left" : "right";
  return y < 0 ? "top" : "bottom";
}

export function nearCenter(target: Rect, floating: Rect): boolean {
  const { x, y } = centerOffset(target, floating);
  return Math.max(Math.abs(x), Math.abs(y)) < CENTER_ZONE;
}

/** The group the floating pane covers most, once it covers enough to make room. */
export function coveredGroup(groups: GroupRect[], floating: Rect): GroupRect | null {
  let best: GroupRect | null = null;
  let bestCoverage = COVER_SHARE;
  for (const entry of groups) {
    const covered = coverage(entry.rect, floating);
    if (covered >= bestCoverage) {
      best = entry;
      bestCoverage = covered;
    }
  }
  return best;
}

const OPPOSITE: Record<EdgeSide, EdgeSide> = {
  left: "right",
  right: "left",
  top: "bottom",
  bottom: "top",
};

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

/**
 * The side of a covered group to open for a dragged tab. When the pane already sits on that
 * side, opening it would change nothing, so the group moves over to the pane's place instead.
 */
export function sideToMakeRoom(
  root: LayoutNode,
  tabId: string,
  groupId: string,
  side: EdgeSide,
): EdgeSide {
  if (groupOfTab(root, tabId)?.id === groupId) return side;
  return sameArrangement(dockTab(root, tabId, groupId, side), root) ? OPPOSITE[side] : side;
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
