import { describe, expect, it } from "vitest";
import { GLIDE_MILLIS, easeOut, glideAt, nextGlide, wheelDistance } from "./smoothScroll";

describe("wheelDistance", () => {
  it("counts pixels, lines and pages in pixels", () => {
    expect(wheelDistance({ deltaY: 100, deltaMode: 0 }, 500)).toBe(100);
    expect(wheelDistance({ deltaY: -3, deltaMode: 1 }, 500)).toBe(-120);
    expect(wheelDistance({ deltaY: 1, deltaMode: 2 }, 500)).toBe(500);
  });
});

describe("glides", () => {
  it("ease from where they start to where they go, and stop there", () => {
    const glide = { from: 0, to: 100, start: 1000 };
    expect(glideAt(glide, 1000)).toEqual({ position: 0, done: false });
    const halfway = glideAt(glide, 1000 + GLIDE_MILLIS / 2).position;
    expect(halfway).toBeGreaterThan(50);
    expect(halfway).toBeLessThan(100);
    expect(glideAt(glide, 1000 + GLIDE_MILLIS)).toEqual({ position: 100, done: true });
    expect(easeOut(-1)).toBe(0);
    expect(easeOut(2)).toBe(1);
  });

  it("add a step to where the glide under way is going", () => {
    const first = nextGlide(undefined, 200, 100, 1000, 0);
    expect(first).toEqual({ from: 200, to: 300, start: 0 });
    expect(nextGlide(first, 240, 100, 1000, 50)).toEqual({ from: 240, to: 400, start: 50 });
    expect(nextGlide(first, 240, -100, 1000, 50).to).toBe(200);
  });

  it("stop at either end of the list", () => {
    expect(nextGlide(undefined, 950, 100, 1000, 0).to).toBe(1000);
    expect(nextGlide(undefined, 30, -100, 1000, 0).to).toBe(0);
  });
});
