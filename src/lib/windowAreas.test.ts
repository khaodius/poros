import { describe, expect, it } from "vitest";
import { onScreen, windowAt, type WindowArea } from "./windowAreas";

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

describe("onScreen", () => {
  const screens = [
    { left: 0, top: 0, width: 1920, height: 1040 },
    { left: 1920, top: 0, width: 2560, height: 1400 },
  ];
  const opened = (left: number, top: number) => ({ left, top, width: 800, height: 500 });

  it("leaves a window that fits where it is", () => {
    expect(onScreen(screens, 300, 300, opened(240, 284))).toEqual(opened(240, 284));
  });

  it("moves a window hanging off the bottom or right back onto the screen", () => {
    expect(onScreen(screens, 1800, 1000, opened(1740, 984))).toEqual(opened(1120, 540));
  });

  it("moves a window hanging off the top or left back onto the screen", () => {
    expect(onScreen(screens, 20, 5, opened(-40, -11))).toEqual(opened(0, 0));
  });

  it("keeps the window on the screen under the pointer", () => {
    expect(onScreen(screens, 1950, 1300, opened(1890, 1284))).toEqual(opened(1920, 900));
  });

  it("leaves the window be when the pointer is on no screen", () => {
    expect(onScreen(screens, -50, -50, opened(-110, -66))).toEqual(opened(-110, -66));
  });
});
