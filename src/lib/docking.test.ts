import { describe, expect, it } from "vitest";
import {
  changesLayout,
  dropSide,
  floatingFrame,
  groupNearest,
  groupRects,
  type Rect,
} from "./docking";
import { group, type LayoutNode } from "./layout";

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

  it("merges in the middle of a group and splits toward its edges", () => {
    const target = rect(0, 0, 400, 400);
    expect(dropSide(target, { x: 200, y: 200 })).toBe("center");
    expect(dropSide(target, { x: 110, y: 290 })).toBe("center");
    expect(dropSide(target, { x: 90, y: 200 })).toBe("left");
    expect(dropSide(target, { x: 320, y: 250 })).toBe("right");
    expect(dropSide(target, { x: 200, y: 20 })).toBe("top");
    expect(dropSide(target, { x: 250, y: 380 })).toBe("bottom");
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
});
