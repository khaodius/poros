import { describe, expect, it } from "vitest";
import { boxBetween, edgeScrollSpeed, rowsInBox } from "./selectionBox";

describe("boxBetween", () => {
  const bounds = { width: 400, height: 600 };

  it("spans the two corners whichever way the box is drawn", () => {
    const box = { left: 20, top: 30, width: 100, height: 50 };
    expect(boxBetween({ x: 20, y: 30 }, { x: 120, y: 80 }, bounds)).toEqual(box);
    expect(boxBetween({ x: 120, y: 80 }, { x: 20, y: 30 }, bounds)).toEqual(box);
  });

  it("stays inside the content", () => {
    expect(boxBetween({ x: 300, y: 500 }, { x: 520, y: 900 }, bounds)).toEqual({
      left: 300,
      top: 500,
      width: 100,
      height: 100,
    });
    expect(boxBetween({ x: 50, y: 40 }, { x: -30, y: -70 }, bounds)).toEqual({
      left: 0,
      top: 0,
      width: 50,
      height: 40,
    });
  });
});

describe("rowsInBox", () => {
  const box = (top: number, height: number, left = 10) => ({ left, top, width: 40, height });

  it("finds every row the box touches", () => {
    expect(rowsInBox(box(25, 50), 20, 10, 300)).toEqual({ first: 1, last: 3 });
  });

  it("takes a flat box as touching the row it crosses", () => {
    expect(rowsInBox(box(45, 0), 20, 10, 300)).toEqual({ first: 2, last: 2 });
  });

  it("leaves out the row below a box ending on the line between them", () => {
    expect(rowsInBox(box(5, 35), 20, 10, 300)).toEqual({ first: 0, last: 1 });
  });

  it("stops at the last row", () => {
    expect(rowsInBox(box(150, 200), 20, 10, 300)).toEqual({ first: 7, last: 9 });
    expect(rowsInBox(box(210, 40), 20, 10, 300)).toBeNull();
  });

  it("touches nothing right of the rows or in an empty list", () => {
    expect(rowsInBox(box(25, 50, 300), 20, 10, 300)).toBeNull();
    expect(rowsInBox(box(0, 50), 20, 0, 300)).toBeNull();
  });
});

describe("edgeScrollSpeed", () => {
  it("is still away from the edges", () => {
    expect(edgeScrollSpeed(300, 100, 500)).toBe(0);
  });

  it("scrolls up near the top and down near the bottom, faster further in", () => {
    const nearTop = edgeScrollSpeed(120, 100, 500);
    const pastTop = edgeScrollSpeed(60, 100, 500);
    expect(nearTop).toBeLessThan(0);
    expect(pastTop).toBeLessThan(nearTop);
    const nearBottom = edgeScrollSpeed(480, 100, 500);
    expect(nearBottom).toBeGreaterThan(0);
    expect(edgeScrollSpeed(560, 100, 500)).toBeGreaterThan(nearBottom);
  });

  it("stops getting faster far past the edge", () => {
    expect(edgeScrollSpeed(-2000, 100, 500)).toBe(edgeScrollSpeed(-1000, 100, 500));
  });
});
