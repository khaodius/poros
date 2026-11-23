import { describe, expect, it } from "vitest";
import {
  DEFAULT_PREVIEW_SHARE,
  SIDE_BY_SIDE_MIN_WIDTH,
  clampPreviewShare,
  fitWithin,
  previewSide,
  shareAtPointer,
} from "./previewLayout";

describe("previewSide", () => {
  it("puts the preview below the list in a pane too narrow for both side by side", () => {
    expect(previewSide(900)).toBe("side");
    expect(previewSide(SIDE_BY_SIDE_MIN_WIDTH)).toBe("side");
    expect(previewSide(SIDE_BY_SIDE_MIN_WIDTH - 1)).toBe("below");
    expect(previewSide(0)).toBe("below");
  });
});

describe("shareAtPointer", () => {
  it("gives the preview what lies past the divider, within limits", () => {
    expect(shareAtPointer(700, 100, 1000)).toBeCloseTo(0.4);
    expect(shareAtPointer(1090, 100, 1000)).toBe(0.2);
    expect(shareAtPointer(100, 100, 1000)).toBe(0.7);
    expect(shareAtPointer(50, 100, 0)).toBe(DEFAULT_PREVIEW_SHARE);
    expect(clampPreviewShare(5)).toBe(0.7);
    expect(clampPreviewShare(-1)).toBe(0.2);
  });
});

describe("fitWithin", () => {
  it("scales a picture down to the box, keeping its shape, and never up", () => {
    expect(fitWithin(4000, 3000, 96)).toEqual({ width: 96, height: 72 });
    expect(fitWithin(300, 1200, 96)).toEqual({ width: 24, height: 96 });
    expect(fitWithin(16, 16, 96)).toEqual({ width: 16, height: 16 });
    expect(fitWithin(10_000, 3, 96)).toEqual({ width: 96, height: 1 });
    // A drawing with no size of its own fills the box.
    expect(fitWithin(0, 0, 96)).toEqual({ width: 96, height: 96 });
  });
});
