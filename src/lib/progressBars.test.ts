import { describe, expect, it } from "vitest";
import {
  DEFAULT_PROGRESS_BARS,
  PROGRESS_FILLS,
  PROGRESS_MOTIONS,
  PROGRESS_PRESETS,
  PROGRESS_SHAPES,
  fillTakesShape,
  matchingPreset,
  progressBarStyle,
  progressLabel,
  progressReach,
  storableProgressBars,
  type ProgressBarSettings,
} from "./progressBars";
import { DEFAULT_SETTINGS, sanitizeSettings, storableSettings } from "./settings";

const MIB = 1024 * 1024;

function storedBars(progressBars: Record<string, unknown>): ProgressBarSettings {
  return sanitizeSettings({ appearance: { progressBars } }).appearance.progressBars;
}

describe("progress bar settings", () => {
  it("start plain, as bars looked before they could be changed", () => {
    expect(DEFAULT_SETTINGS.appearance.progressBars).toEqual(DEFAULT_PROGRESS_BARS);
    expect(sanitizeSettings({}).appearance.progressBars).toEqual(DEFAULT_PROGRESS_BARS);
    expect(DEFAULT_PROGRESS_BARS).toMatchObject({
      fill: "solid",
      motion: "none",
      height: 6,
      shape: "pill",
    });
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

  it("keep every fill, motion and shape there is", () => {
    for (const fill of PROGRESS_FILLS) expect(storedBars({ fill }).fill).toBe(fill);
    for (const motion of PROGRESS_MOTIONS) expect(storedBars({ motion }).motion).toBe(motion);
    for (const shape of PROGRESS_SHAPES) expect(storedBars({ shape }).shape).toBe(shape);
    expect(storedBars({ fill: "plaid", motion: "wobble", shape: "star" })).toEqual(
      DEFAULT_PROGRESS_BARS,
    );
  });

  it("turn into custom properties and document attributes", () => {
    expect(
      progressBarStyle({
        ...DEFAULT_PROGRESS_BARS,
        shape: "square",
        stateColors: false,
        height: 9,
      }),
    ).toEqual({
      properties: {
        "--progress-height": "9px",
        "--progress-radius": "0px",
        "--progress-custom": DEFAULT_PROGRESS_BARS.color,
      },
      attributes: {
        "data-progress-fill": "solid",
        "data-progress-coloring": "theme",
        "data-progress-motion": "none",
        "data-progress-shape": "square",
        "data-progress-states": "off",
      },
    });
  });

  it("give each shape its corners, and leave fills with a shape of their own alone", () => {
    const radius = (change: Partial<ProgressBarSettings>) =>
      progressBarStyle({ ...DEFAULT_PROGRESS_BARS, ...change }).properties["--progress-radius"];
    expect(radius({ shape: "pill" })).toBe("999px");
    expect(radius({ shape: "soft" })).toBe("max(2px, 0.25em)");
    expect(radius({ shape: "square" })).toBe("0px");
    expect(radius({ shape: "slanted" })).toBe("0px");

    expect(PROGRESS_FILLS.filter((fill) => !fillTakesShape(fill))).toEqual(["line", "dots"]);
    for (const fill of ["line", "dots"] as const) {
      const style = progressBarStyle({ ...DEFAULT_PROGRESS_BARS, fill, shape: "slanted" });
      expect(style.attributes["data-progress-shape"]).toBe("square");
      expect(style.properties["--progress-radius"]).toBe("0px");
    }
  });
});

describe("progressReach", () => {
  it("sizes the fill from a property on the track, kept between empty and full", () => {
    expect(progressReach(0.625)).toEqual({ "--progress-percent": "62.5%" });
    expect(progressReach(0)).toEqual({ "--progress-percent": "0%" });
    expect(progressReach(1.4)).toEqual({ "--progress-percent": "100%" });
    expect(progressReach(-0.2)).toEqual({ "--progress-percent": "0%" });
  });
});

describe("progress bars stored by 0.7.0", () => {
  // Every value 0.7.0 could write, and the document attributes and properties it set for them.
  const fills = ["solid", "gradient", "striped", "segmented", "glow"];
  const colorings = ["theme", "direction", "custom"];
  const motions = ["none", "stripes", "shimmer", "pulse"];
  const labels = ["percent", "bytes", "remaining", "none"];

  it("load with every choice kept", () => {
    for (const fill of fills) {
      for (const coloring of colorings) {
        for (const motion of motions) {
          for (const label of labels) {
            for (const rounded of [true, false]) {
              const kept = {
                fill,
                coloring,
                color: "#12ab9f",
                height: 11,
                motion,
                label,
                labelInside: !rounded,
                stateColors: rounded,
              };
              expect(storedBars({ ...kept, rounded })).toEqual({
                ...kept,
                shape: rounded ? "pill" : "square",
              });
            }
          }
        }
      }
    }
  });

  it("are drawn from the same attributes and properties", () => {
    for (const rounded of [true, false]) {
      const style = progressBarStyle(
        storedBars({
          fill: "glow",
          coloring: "direction",
          color: "#12ab9f",
          height: 14,
          rounded,
          motion: "shimmer",
          label: "bytes",
          labelInside: true,
          stateColors: true,
        }),
      );
      expect(style.properties).toEqual({
        "--progress-height": "14px",
        "--progress-radius": rounded ? "999px" : "0px",
        "--progress-custom": "#12ab9f",
      });
      expect(style.attributes).toMatchObject({
        "data-progress-fill": "glow",
        "data-progress-coloring": "direction",
        "data-progress-motion": "shimmer",
        "data-progress-states": "on",
      });
    }
  });

  it("have square ends only where the switch for rounded ends was off", () => {
    expect(storedBars({ rounded: false }).shape).toBe("square");
    expect(storedBars({ rounded: true }).shape).toBe("pill");
    expect(storedBars({}).shape).toBe("pill");
  });

  it("follow a shape picked since, whatever the old switch said", () => {
    expect(storedBars({ rounded: false, shape: "slanted" }).shape).toBe("slanted");
    expect(storedBars({ rounded: true, shape: "square" }).shape).toBe("square");
    expect(storedBars({ rounded: false, shape: "star" }).shape).toBe("square");
    expect(storedBars({ rounded: false, shape: 3 }).shape).toBe("square");
  });

  it("stay as loaded when saved and loaded again", () => {
    const loaded = storedBars({ fill: "segmented", rounded: false, motion: "pulse" });
    expect(storedBars({ ...loaded })).toEqual(loaded);
    expect(storedBars({ ...storableProgressBars(loaded) })).toEqual(loaded);
  });

  it("are saved with the switch 0.7.0 reads, so a downgrade keeps square ends", () => {
    const rounded = { pill: true, soft: true, square: false, slanted: false };
    for (const shape of PROGRESS_SHAPES) {
      const bars = { ...DEFAULT_PROGRESS_BARS, fill: "glow" as const, shape };
      const stored = storableProgressBars(bars);
      expect(stored).toEqual({ ...bars, rounded: rounded[shape] });
      // Loaded again by this version, the shape wins over the switch.
      expect(storedBars({ ...stored })).toEqual(bars);
      // Saved again by 0.7.0, which drops the shape and keeps the switch.
      const downgraded: Record<string, unknown> = { ...stored };
      delete downgraded.shape;
      expect(storedBars(downgraded).shape).toBe(rounded[shape] ? "pill" : "square");
    }
  });

  it("are written with the switch whenever the settings are saved", () => {
    const settings = sanitizeSettings({ appearance: { progressBars: { shape: "square" } } });
    const written = storableSettings(settings);
    expect(written.appearance.progressBars).toMatchObject({ shape: "square", rounded: false });
    expect(sanitizeSettings(written)).toEqual(settings);
    expect(settings.appearance.progressBars).not.toHaveProperty("rounded");
  });
});

describe("progress bar presets", () => {
  it("have names and ids of their own", () => {
    expect(PROGRESS_PRESETS.length).toBeGreaterThanOrEqual(6);
    expect(PROGRESS_PRESETS.length).toBeLessThanOrEqual(10);
    expect(new Set(PROGRESS_PRESETS.map((preset) => preset.id)).size).toBe(PROGRESS_PRESETS.length);
    expect(new Set(PROGRESS_PRESETS.map((preset) => preset.name)).size).toBe(
      PROGRESS_PRESETS.length,
    );
  });

  it("are all looks the settings keep as they are, each unlike the others", () => {
    const seen = new Set<string>();
    for (const preset of PROGRESS_PRESETS) {
      const applied = { ...DEFAULT_PROGRESS_BARS, ...preset.look };
      expect(storedBars({ ...applied })).toEqual(applied);
      expect(matchingPreset(applied)).toBe(preset);
      seen.add(JSON.stringify(preset.look));
    }
    expect(seen.size).toBe(PROGRESS_PRESETS.length);
  });

  it("begin with the look bars have until they are changed", () => {
    expect(matchingPreset(DEFAULT_PROGRESS_BARS)).toBe(PROGRESS_PRESETS[0]);
    expect(matchingPreset(storedBars({ rounded: true }))).toBe(PROGRESS_PRESETS[0]);
  });

  it("leave the custom color alone and match whatever it is", () => {
    const preset = PROGRESS_PRESETS[1];
    expect("color" in preset.look).toBe(false);
    const applied = { ...DEFAULT_PROGRESS_BARS, color: "#123456", ...preset.look };
    expect(applied.color).toBe("#123456");
    expect(matchingPreset(applied)).toBe(preset);
  });

  it("match nothing once any part of the look differs", () => {
    expect(matchingPreset({ ...DEFAULT_PROGRESS_BARS, height: 7 })).toBeNull();
    expect(matchingPreset({ ...DEFAULT_PROGRESS_BARS, stateColors: false })).toBeNull();
    expect(matchingPreset({ ...DEFAULT_PROGRESS_BARS, shape: "square" })).toBeNull();
  });

  it("between them show every new fill and most motions", () => {
    const fills = new Set(PROGRESS_PRESETS.map((preset) => preset.look.fill));
    for (const fill of PROGRESS_FILLS.slice(5)) expect(fills).toContain(fill);
    const motions = new Set(PROGRESS_PRESETS.map((preset) => preset.look.motion));
    expect(motions.size).toBeGreaterThanOrEqual(6);
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
