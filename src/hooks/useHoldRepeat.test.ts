import { describe, expect, it } from "vitest";
import { STEPS_PER_SPEED_UP, repeatDelay } from "./useHoldRepeat";

describe("repeatDelay", () => {
  it("pauses after the first step, then speeds up a lot after every ten", () => {
    expect(repeatDelay(1)).toBeGreaterThan(repeatDelay(2));
    expect(repeatDelay(STEPS_PER_SPEED_UP - 1)).toBe(repeatDelay(2));
    for (const tens of [1, 2]) {
      const steps = tens * STEPS_PER_SPEED_UP;
      expect(repeatDelay(steps)).toBeLessThanOrEqual(repeatDelay(steps - 1) / 2.5);
      expect(repeatDelay(steps + STEPS_PER_SPEED_UP - 1)).toBe(repeatDelay(steps));
    }
    expect(repeatDelay(30)).toBeLessThan(repeatDelay(29));
    expect(repeatDelay(500)).toBe(repeatDelay(30));
  });
});
