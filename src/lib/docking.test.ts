import { describe, expect, it } from "vitest";
import {
  coverage,
  coveredGroup,
  floatingFrame,
  floatingRect,
  groupRects,
  nearCenter,
  sideToMakeRoom,
  sideWithin,
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

  it("measures coverage against the smaller rectangle", () => {
    const big = rect(0, 0, 400, 400);
    expect(coverage(big, rect(300, 0, 200, 100))).toBe(0.5);
    expect(coverage(rect(0, 0, 100, 100), big)).toBe(1);
    expect(coverage(big, rect(400, 0, 100, 100))).toBe(0);
  });

  it("makes room only once half is covered", () => {
    const groups = groupRects(sideBySide(), DOCK);
    const pane = { grab: { x: 0, y: 0 }, size: { width: 500, height: 360 } };
    const coveredAt = (x: number) => coveredGroup(groups, floatingRect(pane, { x, y: 40 }));
    expect(coveredAt(200)?.groupId).toBe("left");
    // Mostly over the gap and the left group: the right group stays put.
    expect(coveredAt(250)?.groupId).toBe("left");
    expect(coveredAt(270)?.groupId).toBe("right");
    expect(coveredAt(700)?.groupId).toBe("right");
    expect(coveredAt(1000)).toBeNull();
  });

  it("picks the side the floating pane leans toward", () => {
    const target = rect(0, 0, 400, 400);
    expect(sideWithin(target, rect(-50, 150, 100, 100))).toBe("left");
    expect(sideWithin(target, rect(150, 320, 100, 100))).toBe("bottom");
    expect(sideWithin(target, rect(300, 0, 100, 100))).toBe("right");
    expect(sideWithin(target, rect(160, 0, 100, 100))).toBe("top");
    expect(nearCenter(target, rect(150, 150, 100, 100))).toBe(true);
    expect(nearCenter(target, rect(150, 250, 100, 100))).toBe(false);
  });

  it("moves a covered pane over to the dragged pane's place", () => {
    const root = sideBySide();
    // Left of the right group is where the pane already is, so the groups trade places.
    expect(sideToMakeRoom(root, "a", "right", "left")).toBe("right");
    expect(sideToMakeRoom(root, "a", "right", "bottom")).toBe("bottom");
    expect(sideToMakeRoom(root, "b", "left", "right")).toBe("left");
  });

  it("shrinks a large pane around the pointer that holds it", () => {
    const { grab, size } = floatingFrame(rect(0, 40, 1008, 600), DOCK, { x: 504, y: 55 });
    expect(size).toEqual({ width: 504, height: 360 });
    expect(grab).toEqual({ x: 252, y: 15 });
  });
});
