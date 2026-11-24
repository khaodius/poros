// How progress bars look: the fill, its color, thickness and shape, what moves on a bar while
// its work runs, and the text a transfer's bar shows. Part of the appearance settings.

import { formatDuration, formatPercent, formatSize } from "./format";
import type { JobSnapshot } from "./types";
import { t } from "../i18n";

export type ProgressFill =
  | "solid"
  | "gradient"
  | "striped"
  | "segmented"
  | "glow"
  | "candy"
  | "chevrons"
  | "aurora"
  | "neon"
  | "wave"
  | "line"
  | "dots"
  | "blocks";
export const PROGRESS_FILLS: ProgressFill[] = [
  "solid",
  "gradient",
  "striped",
  "segmented",
  "glow",
  "candy",
  "chevrons",
  "aurora",
  "neon",
  "wave",
  "line",
  "dots",
  "blocks",
];

/** `theme` uses the theme's progress color, `direction` its upload and download colors. */
export type ProgressColoring = "theme" | "direction" | "custom";
export const PROGRESS_COLORINGS: ProgressColoring[] = ["theme", "direction", "custom"];

export type ProgressMotion =
  "none" | "stripes" | "shimmer" | "pulse" | "scan" | "ripple" | "comet" | "sparkle" | "breathe";
export const PROGRESS_MOTIONS: ProgressMotion[] = [
  "none",
  "stripes",
  "shimmer",
  "pulse",
  "scan",
  "ripple",
  "comet",
  "sparkle",
  "breathe",
];

/** The ends of a bar: fully round, slightly rounded, square, or cut at a slant. */
export type ProgressShape = "pill" | "soft" | "square" | "slanted";
export const PROGRESS_SHAPES: ProgressShape[] = ["pill", "soft", "square", "slanted"];

export type ProgressLabel = "percent" | "bytes" | "remaining" | "none";
export const PROGRESS_LABELS: ProgressLabel[] = ["percent", "bytes", "remaining", "none"];

export interface ProgressBarSettings {
  fill: ProgressFill;
  coloring: ProgressColoring;
  /** Used when `coloring` is `custom`. */
  color: string;
  /** Pixels, for the bars in the transfer list. */
  height: number;
  shape: ProgressShape;
  /** Plays on bars whose work is running. */
  motion: ProgressMotion;
  label: ProgressLabel;
  /** The text sits on the bar, which the stylesheet grows to fit it, rather than beside it. */
  labelInside: boolean;
  /** Finished, paused and failed transfers take the theme's colors for those states. */
  stateColors: boolean;
}

export const PROGRESS_HEIGHT_LIMITS = { min: 2, max: 18 };
const DEFAULT_CUSTOM_COLOR = "#a46cff";
/** The color picker takes and gives `#rrggbb`. */
const HEX_COLOR = /^#[0-9a-f]{6}$/i;

export const DEFAULT_PROGRESS_BARS: ProgressBarSettings = {
  fill: "solid",
  coloring: "theme",
  color: DEFAULT_CUSTOM_COLOR,
  height: 6,
  shape: "pill",
  motion: "none",
  label: "percent",
  labelInside: false,
  stateColors: true,
};

/**
 * Takes settings already merged over the defaults, so every field has the right type, and the
 * stored values they came from, which may be older than the fields.
 */
export function sanitizeProgressBars(
  stored: ProgressBarSettings,
  source: Record<string, unknown> = {},
): ProgressBarSettings {
  const pick = <T extends string>(value: T, allowed: T[], fallback: T): T =>
    allowed.includes(value) ? value : fallback;
  const height = Number.isFinite(stored.height)
    ? Math.round(stored.height)
    : DEFAULT_PROGRESS_BARS.height;
  // Before shapes could be picked, a switch chose between rounded and square ends.
  const shapeBefore = source.rounded === false ? "square" : DEFAULT_PROGRESS_BARS.shape;
  return {
    fill: pick(stored.fill, PROGRESS_FILLS, DEFAULT_PROGRESS_BARS.fill),
    coloring: pick(stored.coloring, PROGRESS_COLORINGS, DEFAULT_PROGRESS_BARS.coloring),
    color: HEX_COLOR.test(stored.color.trim())
      ? stored.color.trim().toLowerCase()
      : DEFAULT_PROGRESS_BARS.color,
    height: Math.min(PROGRESS_HEIGHT_LIMITS.max, Math.max(PROGRESS_HEIGHT_LIMITS.min, height)),
    shape:
      typeof source.shape === "string"
        ? pick(stored.shape, PROGRESS_SHAPES, shapeBefore)
        : shapeBefore,
    motion: pick(stored.motion, PROGRESS_MOTIONS, DEFAULT_PROGRESS_BARS.motion),
    label: pick(stored.label, PROGRESS_LABELS, DEFAULT_PROGRESS_BARS.label),
    labelInside: stored.labelInside,
    stateColors: stored.stateColors,
  };
}

/**
 * The settings as written to disk. 0.7.0 knew no shapes, only a switch for rounded ends, so the
 * switch is written beside the shape: after a downgrade, ends with corners keep them.
 */
export function storableProgressBars(
  settings: ProgressBarSettings,
): ProgressBarSettings & { rounded: boolean } {
  return { ...settings, rounded: settings.shape === "pill" || settings.shape === "soft" };
}

/** Dots and the thin line are round by nature, so a bar's shape changes nothing about them. */
export function fillTakesShape(fill: ProgressFill): boolean {
  return fill !== "dots" && fill !== "line";
}

/** Corner radii in the stylesheet's terms, where an em is the bar's thickness. */
const SHAPE_RADIUS: Record<ProgressShape, string> = {
  pill: "999px",
  soft: "max(2px, 0.25em)",
  square: "0px",
  slanted: "0px",
};

/**
 * The custom properties and attributes the stylesheet draws bars from: on the document for
 * every bar, or on an element inside it for the bars within, as the settings' swatches do.
 */
export function progressBarStyle(settings: ProgressBarSettings): {
  properties: Record<string, string>;
  attributes: Record<string, string>;
} {
  const shape = fillTakesShape(settings.fill) ? settings.shape : "square";
  return {
    properties: {
      "--progress-height": `${settings.height}px`,
      "--progress-radius": SHAPE_RADIUS[shape],
      "--progress-custom": settings.color,
    },
    attributes: {
      "data-progress-fill": settings.fill,
      "data-progress-coloring": settings.coloring,
      "data-progress-motion": settings.motion,
      "data-progress-shape": shape,
      "data-progress-states": settings.stateColors ? "on" : "off",
    },
  };
}

/**
 * How far a bar's fill reaches, from 0 to 1, as the custom property its track takes. The
 * stylesheet sizes the fill from it, so fills made of cells can grow a whole cell at a time.
 */
export function progressReach(fraction: number): Record<string, string> {
  const percent = Math.min(100, Math.max(0, fraction * 100));
  return { "--progress-percent": `${percent}%` };
}

/** Everything a preset decides: all of a look but the custom color, which stays the user's. */
export type ProgressLook = Omit<ProgressBarSettings, "color">;

export interface ProgressPreset {
  id: string;
  name: string;
  look: ProgressLook;
}

function lookOf(settings: ProgressBarSettings): ProgressLook {
  const rest: Partial<ProgressBarSettings> = { ...settings };
  delete rest.color;
  return rest as ProgressLook;
}

const look = (change: Partial<ProgressLook>): ProgressLook => ({
  ...lookOf(DEFAULT_PROGRESS_BARS),
  ...change,
});

/** Named looks offered in one click. The first is how bars look until they are changed. */
export const PROGRESS_PRESETS: ProgressPreset[] = [
  { id: "classic", get name() {
      return t("appearance.presetClassic");
    }, look: look({}) },
  {
    id: "showcase",
    get name() {
      return t("appearance.presetGradient");
    },
    look: look({
      fill: "gradient",
      coloring: "direction",
      motion: "shimmer",
      height: 14,
      label: "bytes",
      labelInside: true,
    }),
  },
  {
    id: "candy",
    get name() {
      return t("appearance.presetCandyCane");
    },
    look: look({ fill: "candy", coloring: "direction", motion: "stripes", height: 10 }),
  },
  {
    id: "neon",
    get name() {
      return t("appearance.presetNeon");
    },
    look: look({ fill: "neon", motion: "breathe", height: 8, label: "remaining" }),
  },
  {
    id: "comet",
    get name() {
      return t("appearance.presetComet");
    },
    look: look({ fill: "line", coloring: "direction", motion: "comet", height: 3 }),
  },
  {
    id: "tide",
    get name() {
      return t("appearance.presetTide");
    },
    look: look({
      fill: "wave",
      coloring: "direction",
      motion: "ripple",
      shape: "soft",
      height: 16,
      label: "bytes",
      labelInside: true,
    }),
  },
  {
    id: "aurora",
    get name() {
      return t("appearance.presetAurora");
    },
    look: look({ fill: "aurora", motion: "sparkle", height: 8 }),
  },
  {
    id: "conveyor",
    get name() {
      return t("appearance.presetConveyor");
    },
    look: look({
      fill: "chevrons",
      coloring: "direction",
      motion: "stripes",
      shape: "slanted",
      height: 12,
    }),
  },
  {
    id: "arcade",
    get name() {
      return t("appearance.presetArcade");
    },
    look: look({ fill: "blocks", motion: "scan", shape: "square", height: 10 }),
  },
  {
    id: "beads",
    get name() {
      return t("appearance.presetBeads");
    },
    look: look({ fill: "dots", coloring: "direction", motion: "pulse", height: 8 }),
  },
];

const LOOK_KEYS = Object.keys(lookOf(DEFAULT_PROGRESS_BARS)) as (keyof ProgressLook)[];

/** The preset the settings match in everything a preset decides, if any. */
export function matchingPreset(settings: ProgressBarSettings): ProgressPreset | null {
  return (
    PROGRESS_PRESETS.find((preset) =>
      LOOK_KEYS.every((key) => preset.look[key] === settings[key]),
    ) ?? null
  );
}

/** The text a transfer's bar shows: empty when there is nothing to say for its state. */
export function progressLabel(
  job: Pick<JobSnapshot, "state" | "size" | "transferred" | "speed">,
  label: ProgressLabel,
  fraction: number,
): string {
  switch (label) {
    case "percent":
      return formatPercent(Math.floor(fraction * 100));
    case "bytes":
      if (job.state === "done") return formatSize(job.size);
      return t("transfers.sizeOfSize", {
        transferred: formatSize(Math.min(job.transferred, job.size)),
        size: formatSize(job.size),
      });
    case "remaining": {
      if (job.state === "done") return t("transfers.done");
      if (job.state !== "running" || job.speed <= 0) return "";
      const left = Math.max(0, job.size - job.transferred);
      return t("transfers.timeLeft", { duration: formatDuration(Math.ceil(left / job.speed)) });
    }
    case "none":
      return "";
  }
}
