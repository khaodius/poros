import { describe, expect, it } from "vitest";
import { DEFAULT_PROGRESS_BARS, progressBarStyle, progressLabel } from "./progressBars";
import { DEFAULT_SETTINGS, sanitizeSettings } from "./settings";

const MIB = 1024 * 1024;

describe("progress bar settings", () => {
  it("start plain, as bars looked before they could be changed", () => {
    expect(DEFAULT_SETTINGS.appearance.progressBars).toEqual(DEFAULT_PROGRESS_BARS);
    expect(sanitizeSettings({}).appearance.progressBars).toEqual(DEFAULT_PROGRESS_BARS);
    expect(DEFAULT_PROGRESS_BARS).toMatchObject({ fill: "solid", motion: "none", height: 6 });
  });

  it("keep valid values and replace unknown ones", () => {
    const { progressBars } = sanitizeSettings({
      appearance: {
        progressBars: {
          fill: "glow",
          coloring: "rainbow",
          color: " #A0B1C2 ",
          height: 40,
          motion: "stripes",
          label: "bytes",
          labelInside: true,
          rounded: "yes",
        },
      },
    }).appearance;
    expect(progressBars).toEqual({
      ...DEFAULT_PROGRESS_BARS,
      fill: "glow",
      color: "#a0b1c2",
      height: 18,
      motion: "stripes",
      label: "bytes",
      labelInside: true,
    });
  });

  it("drop colors the picker cannot show and clamp thin bars", () => {
    const { progressBars } = sanitizeSettings({
      appearance: { progressBars: { color: "#abc", height: 0.4 } },
    }).appearance;
    expect(progressBars.color).toBe(DEFAULT_PROGRESS_BARS.color);
    expect(progressBars.height).toBe(2);
  });

  it("turn into custom properties and document attributes", () => {
    expect(
      progressBarStyle({ ...DEFAULT_PROGRESS_BARS, rounded: false, stateColors: false, height: 9 }),
    ).toEqual({
      properties: {
        "--progress-height": "9px",
        "--progress-radius": "0px",
        "--progress-custom": DEFAULT_PROGRESS_BARS.color,
      },
      attributes: {
        progressFill: "solid",
        progressColoring: "theme",
        progressMotion: "none",
        progressStates: "off",
      },
    });
  });
});

describe("progressLabel", () => {
  const running = { state: "running" as const, size: 100 * MIB, transferred: 25 * MIB, speed: MIB };

  it("shows the percent", () => {
    expect(progressLabel(running, "percent", 0.256)).toBe("25%");
  });

  it("shows what has arrived of the size", () => {
    expect(progressLabel(running, "bytes", 0.25)).toBe("25.0 MB of 100.0 MB");
    expect(progressLabel({ ...running, state: "done" }, "bytes", 1)).toBe("100.0 MB");
  });

  it("shows the time left only while the file moves", () => {
    expect(progressLabel(running, "remaining", 0.25)).toBe("1m 15s left");
    expect(progressLabel({ ...running, speed: 0 }, "remaining", 0.25)).toBe("");
    expect(progressLabel({ ...running, state: "paused" }, "remaining", 0.25)).toBe("");
    expect(progressLabel({ ...running, state: "done" }, "remaining", 1)).toBe("Done");
  });

  it("shows nothing when asked to", () => {
    expect(progressLabel(running, "none", 0.25)).toBe("");
  });
});

describe("flying files", () => {
  it("stay grounded until turned on", () => {
    expect(DEFAULT_SETTINGS.appearance.transferAnimation).toBe(false);
    expect(
      sanitizeSettings({ appearance: { transferAnimation: "yes" } }).appearance.transferAnimation,
    ).toBe(false);
    expect(
      sanitizeSettings({ appearance: { transferAnimation: true } }).appearance.transferAnimation,
    ).toBe(true);
  });
});
