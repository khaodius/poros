import { describe, expect, it } from "vitest";
import { windowAt, type WindowArea } from "./windowAreas";

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
