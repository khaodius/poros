import { describe, expect, it } from "vitest";
import { heldWindowCorner, windowAt, type WindowArea } from "./windowAreas";

const area = (label: string, left: number, top: number, scale = 1): WindowArea => ({
  label,
  left,
  top,
  width: 800,
  height: 600,
  scale,
});

describe("windowAt", () => {
  it("finds the window under a point, in its own coordinates", () => {
    expect(windowAt([area("main", 100, 50)], 150, 80)).toEqual({ label: "main", x: 50, y: 30 });
    expect(windowAt([area("main", 100, 50)], 950, 80)).toBeNull();
  });

  it("scales physical pixels to the window's own", () => {
    expect(windowAt([area("main", 200, 100, 2)], 400, 300)).toEqual({
      label: "main",
      x: 100,
      y: 100,
    });
  });

  it("prefers a torn-out window over the main window it sits on", () => {
    const areas = [area("main", 0, 0), area("workspace-a", 400, 300)];
    expect(windowAt(areas, 500, 400)?.label).toBe("workspace-a");
    expect(windowAt(areas, 100, 100)?.label).toBe("main");
  });
});

describe("heldWindowCorner", () => {
  // A 125% screen with a 100% screen to its left.
  const screens = [
    { left: 0, top: 0, width: 2560, height: 1392, scale: 1.25 },
    { left: -1920, top: 0, width: 1920, height: 1040, scale: 1 },
  ];
  const grab = { x: 60, y: 16 };

  it("follows the pointer, held at the grab point in the screen's own scale", () => {
    expect(heldWindowCorner(screens, { x: 1000, y: 500 }, grab, 1)).toEqual({ x: 925, y: 480 });
    expect(heldWindowCorner(screens, { x: -1000, y: 500 }, grab, 1)).toEqual({ x: -1060, y: 484 });
  });

  it("keeps following the pointer near the right and bottom edges", () => {
    expect(heldWindowCorner(screens, { x: 2550, y: 1380 }, grab, 1)).toEqual({
      x: 2475,
      y: 1360,
    });
  });

  it("keeps the top bar from starting above or left of the screen", () => {
    expect(heldWindowCorner(screens, { x: 20, y: 5 }, grab, 1)).toEqual({ x: 0, y: 0 });
    expect(heldWindowCorner(screens, { x: -1900, y: 5 }, grab, 1)).toEqual({ x: -1920, y: 0 });
  });

  it("uses the given scale when the pointer is on no screen", () => {
    expect(heldWindowCorner(screens, { x: 5000, y: 50 }, grab, 2)).toEqual({ x: 4880, y: 18 });
  });
});
