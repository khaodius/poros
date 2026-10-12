import { describe, expect, it } from "vitest";
import {
  DOCK_GAP,
  between,
  changesLayout,
  dropPlacement,
  dropSide,
  floatingFrame,
  groupNearest,
  groupRects,
  placementRect,
  tearingOut,
  type Rect,
} from "./docking";
import { group, layoutPlacements, type DropSide, type LayoutNode } from "./layout";

const rect = (left: number, top: number, width: number, height: number): Rect => ({
  left,
  top,
  width,
  height,
});

const DOCK = rect(0, 40, 1008, 600);

function sideBySide(): LayoutNode {
  return {
    type: "split",
    id: "root",
    direction: "row",
    children: [
      { ...group([{ id: "a", kind: "local" }]), id: "left" },
      { ...group([{ id: "b", kind: "welcome" }]), id: "right" },
    ],
    sizes: [0.5, 0.5],
  };
}

describe("docking geometry", () => {
  it("places groups on screen with the gap between them", () => {
    expect(groupRects(sideBySide(), DOCK)).toEqual([
      { groupId: "left", rect: rect(0, 40, 500, 600) },
      { groupId: "right", rect: rect(508, 40, 500, 600) },
    ]);
  });

  it("finds the group under a point, or the closest across a gap", () => {
    const groups = groupRects(sideBySide(), DOCK);
    expect(groupNearest(groups, { x: 200, y: 300 })?.groupId).toBe("left");
    expect(groupNearest(groups, { x: 503, y: 300 })?.groupId).toBe("left");
    expect(groupNearest(groups, { x: 506, y: 300 })?.groupId).toBe("right");
    expect(groupNearest([], { x: 0, y: 0 })).toBeNull();
  });

  it("marks the half of a group a split takes, and all of it for a merge", () => {
    const right = layoutPlacements(sideBySide(), DOCK_GAP).get("right")!;
    const marker = (side: DropSide) => placementRect(dropPlacement(right, side), DOCK);
    expect(marker("center")).toEqual(rect(508, 40, 500, 600));
    expect(marker("left")).toEqual(rect(508, 40, 246, 600));
    expect(marker("right")).toEqual(rect(762, 40, 246, 600));
    expect(marker("top")).toEqual(rect(508, 40, 500, 296));
    expect(marker("bottom")).toEqual(rect(508, 344, 500, 296));
  });

  it("merges in the middle third of a group and splits in the outer thirds", () => {
    const target = rect(0, 0, 400, 400);
    expect(dropSide(target, { x: 200, y: 200 })).toBe("center");
    expect(dropSide(target, { x: 140, y: 260 })).toBe("center");
    expect(dropSide(target, { x: 120, y: 200 })).toBe("left");
    expect(dropSide(target, { x: 300, y: 250 })).toBe("right");
    expect(dropSide(target, { x: 200, y: 100 })).toBe("top");
    expect(dropSide(target, { x: 250, y: 300 })).toBe("bottom");
  });

  it("holds the zone the pointer is in until it is clearly in another", () => {
    const target = rect(0, 0, 400, 400);
    // The bottom third starts at y 266.7.
    expect(dropSide(target, { x: 200, y: 260 })).toBe("center");
    expect(dropSide(target, { x: 200, y: 260 }, "bottom")).toBe("bottom");
    expect(dropSide(target, { x: 200, y: 250 }, "bottom")).toBe("center");
    expect(dropSide(target, { x: 200, y: 275 }, "center")).toBe("center");
    expect(dropSide(target, { x: 200, y: 285 }, "center")).toBe("bottom");
    // Near the corner, between the right and bottom edges.
    expect(dropSide(target, { x: 330, y: 338 })).toBe("bottom");
    expect(dropSide(target, { x: 330, y: 338 }, "right")).toBe("right");
    expect(dropSide(target, { x: 330, y: 370 }, "right")).toBe("bottom");
  });

  it("ignores drops that would leave the layout as it is", () => {
    const root = sideBySide();
    expect(changesLayout(root, "a", "right", "center")).toBe(true);
    // Left of the right group is where the pane already is.
    expect(changesLayout(root, "a", "right", "left")).toBe(false);
    expect(changesLayout(root, "a", "right", "right")).toBe(true);
    expect(changesLayout(root, "a", "left", "bottom")).toBe(false);

    const shared: LayoutNode = {
      ...group([
        { id: "a", kind: "local" },
        { id: "b", kind: "welcome" },
      ]),
      id: "only",
    };
    expect(changesLayout(shared, "a", "only", "center")).toBe(false);
    expect(changesLayout(shared, "a", "only", "right")).toBe(true);
  });

  it("shrinks a large pane around the pointer that holds it", () => {
    const { grab, size } = floatingFrame(rect(0, 40, 1008, 600), DOCK, { x: 504, y: 55 });
    expect(size).toEqual({ width: 504, height: 360 });
    expect(grab).toEqual({ x: 252, y: 15 });
  });

  it("turns a lifted pane into a window only as the pointer nears the window's edge", () => {
    const viewport = { width: 1024, height: 700 };
    expect(tearingOut({ x: 512, y: 300 }, DOCK, viewport)).toBe(0);
    // The tab bars just below the title bar are clear of it.
    expect(tearingOut({ x: 512, y: 60 }, DOCK, viewport)).toBe(0);
    expect(tearingOut({ x: 512, y: 20 }, DOCK, viewport)).toBe(0.5);
    expect(tearingOut({ x: 984, y: 300 }, DOCK, viewport)).toBe(0.5);
    expect(tearingOut({ x: 1024, y: 300 }, DOCK, viewport)).toBe(1);
    expect(tearingOut({ x: -30, y: 300 }, DOCK, viewport)).toBe(1);
    expect(tearingOut({ x: 512, y: 690 }, DOCK, viewport)).toBeGreaterThan(0.8);
  });

  it("moves a rect part of the way to another", () => {
    expect(between(rect(0, 0, 100, 50), rect(100, 50, 300, 250), 0.25)).toEqual(
      rect(25, 12.5, 150, 100),
    );
  });
});
