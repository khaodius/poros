import { describe, expect, it } from "vitest";
import {
  MAX_ZOOM,
  PICTURE_MARGIN,
  anchoredScroll,
  clampZoom,
  fitScale,
  pictureSpan,
  sameScale,
  stepZoom,
  wheelZoomFactor,
  zoomLimits,
} from "./previewZoom";

const MARGINS = 2 * PICTURE_MARGIN;

describe("fitScale", () => {
  it("shrinks a picture larger than the pane to fit inside the margin", () => {
    const fit = fitScale(
      { width: 1600, height: 1067 },
      { width: 400 + MARGINS, height: 900 },
      false,
    );
    expect(fit).toBeCloseTo(0.25);
  });

  it("leaves a small picture at its own size, but grows a drawing", () => {
    const room = { width: 200 + MARGINS, height: 200 + MARGINS };
    expect(fitScale({ width: 50, height: 20 }, room, false)).toBe(1);
    expect(fitScale({ width: 50, height: 20 }, room, true)).toBe(4);
  });

  it("has nothing to fit in a pane with no room or for a picture with no size", () => {
    expect(
      fitScale({ width: 100, height: 100 }, { width: MARGINS, height: 300 }, false),
    ).toBeNull();
    expect(fitScale({ width: 0, height: 100 }, { width: 300, height: 300 }, false)).toBeNull();
  });
});

describe("zoomLimits", () => {
  it("zooms out no further than the picture fits or its own size", () => {
    expect(zoomLimits(0.25)).toEqual({ min: 0.25, max: MAX_ZOOM });
    expect(zoomLimits(1).min).toBe(1);
    expect(zoomLimits(10)).toEqual({ min: 1, max: 40 });
  });

  it("keeps a scale within the limits", () => {
    const limits = zoomLimits(0.5);
    expect(clampZoom(0.1, limits)).toBe(0.5);
    expect(clampZoom(100, limits)).toBe(MAX_ZOOM);
    expect(clampZoom(2, limits)).toBe(2);
  });
});

describe("stepZoom", () => {
  it("goes through the usual steps and stops at the scale that fits on the way", () => {
    expect(stepZoom(0.3, 1, 0.3)).toBe(0.5);
    expect(stepZoom(0.5, -1, 0.3)).toBeCloseTo(1 / 3);
    expect(stepZoom(1 / 3, -1, 0.3)).toBe(0.3);
    expect(stepZoom(0.7, 1, 0.42)).toBe(1);
    expect(stepZoom(0.7, -1, 0.42)).toBe(0.5);
    expect(stepZoom(2, -1, 0.42)).toBe(1.5);
    expect(stepZoom(0.4, 1, 0.42)).toBe(0.42);
  });

  it("passes over a step too near to see", () => {
    // From 23%, 25% is hardly larger: a third is the first step that shows.
    expect(stepZoom(0.234, 1, 0.234)).toBeCloseTo(1 / 3);
    expect(stepZoom(1.1, 1, 0.5)).toBe(1.5);
    expect(stepZoom(0.9999999, 1, 0.25)).toBe(1.5);
    expect(sameScale(1, 1.0000001)).toBe(true);
    expect(sameScale(1, 1.01)).toBe(false);
  });

  it("goes to a limit with no step left before it, and stays there", () => {
    expect(stepZoom(0.3, -1, 0.3)).toBe(0.3);
    expect(stepZoom(MAX_ZOOM, 1, 0.3)).toBe(MAX_ZOOM);
    expect(stepZoom(15, 1, 0.3)).toBe(MAX_ZOOM);
    expect(stepZoom(0.31, -1, 0.3)).toBe(0.3);
    expect(stepZoom(16, 1, 10)).toBe(40);
  });
});

describe("wheelZoomFactor", () => {
  it("zooms by the same step for a notch of the wheel, however far the system says it went", () => {
    for (const notch of [40, 53, 100, 120]) {
      expect(wheelZoomFactor(-notch, 0, false)).toBe(1.25);
      expect(wheelZoomFactor(notch, 0, false)).toBe(0.8);
    }
    expect(wheelZoomFactor(-3, 1, false)).toBe(1.25);
    expect(wheelZoomFactor(1, 2, false)).toBe(0.8);
  });

  it("follows a touchpad as far as it scrolls", () => {
    expect(wheelZoomFactor(-10, 0, false)).toBeCloseTo(Math.exp(0.06));
    expect(wheelZoomFactor(4, 0, false)).toBeCloseTo(Math.exp(-0.024));
  });

  it("follows a pinch more closely, and never jumps by more than double", () => {
    expect(wheelZoomFactor(-10, 0, true)).toBeCloseTo(Math.exp(0.1));
    expect(wheelZoomFactor(-50, 0, true)).toBeCloseTo(Math.exp(0.5));
    expect(wheelZoomFactor(5000, 0, true)).toBe(0.5);
  });
});

describe("pictureSpan", () => {
  it("centres a picture smaller than the view and scrolls one larger, margin included", () => {
    expect(pictureSpan(500, 300)).toEqual({ extent: 500, start: 100 });
    expect(pictureSpan(500, 1000)).toEqual({ extent: 1000 + MARGINS, start: PICTURE_MARGIN });
  });
});

describe("anchoredScroll", () => {
  it("keeps the point under the pointer where it is as the picture grows", () => {
    // A 400 pixel picture in a 500 pixel view starts at 50; the pointer is on its middle.
    const scroll = anchoredScroll({ view: 500, scroll: 0, anchor: 250 }, 400, 800);
    const now = pictureSpan(500, 800);
    expect(now.start + 400 - scroll).toBe(250);
  });

  it("keeps a point off centre in place too", () => {
    const before = 1000;
    const after = 2000;
    const axis = { view: 500, scroll: 300, anchor: 100 };
    const point = (axis.scroll + axis.anchor - pictureSpan(500, before).start) / before;
    const scroll = anchoredScroll(axis, before, after);
    expect(pictureSpan(500, after).start + point * after - scroll).toBeCloseTo(axis.anchor);
  });

  it("stays within the scrolled area", () => {
    expect(anchoredScroll({ view: 500, scroll: 0, anchor: 100 }, 400, 450)).toBe(0);
    const end = pictureSpan(500, 4000).extent - 500;
    expect(anchoredScroll({ view: 500, scroll: end, anchor: 0 }, 4000, 1000)).toBe(
      pictureSpan(500, 1000).extent - 500,
    );
    expect(anchoredScroll({ view: 500, scroll: 900, anchor: 250 }, 2000, 300)).toBe(0);
  });

  it("keeps the picture's edge under a pointer in the margin beside it", () => {
    const left = anchoredScroll({ view: 500, scroll: 0, anchor: 10 }, 400, 800);
    expect(pictureSpan(500, 800).start - left).toBe(10);
    const right = anchoredScroll({ view: 500, scroll: 0, anchor: 490 }, 400, 800);
    expect(pictureSpan(500, 800).start + 800 - right).toBe(490);
  });
});
