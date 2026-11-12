// How progress bars look: the fill, its color, thickness and corners, what moves on a bar while
// its work runs, and the text a transfer's bar shows. Part of the appearance settings.

import { formatDuration, formatSize } from "./format";
import type { JobSnapshot } from "./types";

export type ProgressFill = "solid" | "gradient" | "striped" | "segmented" | "glow";
export const PROGRESS_FILLS: ProgressFill[] = ["solid", "gradient", "striped", "segmented", "glow"];

/** `theme` uses the theme's progress color, `direction` its upload and download colors. */
export type ProgressColoring = "theme" | "direction" | "custom";
export const PROGRESS_COLORINGS: ProgressColoring[] = ["theme", "direction", "custom"];

export type ProgressMotion = "none" | "stripes" | "shimmer" | "pulse";
export const PROGRESS_MOTIONS: ProgressMotion[] = ["none", "stripes", "shimmer", "pulse"];

export type ProgressLabel = "percent" | "bytes" | "remaining" | "none";
export const PROGRESS_LABELS: ProgressLabel[] = ["percent", "bytes", "remaining", "none"];

export interface ProgressBarSettings {
  fill: ProgressFill;
  coloring: ProgressColoring;
  /** Used when `coloring` is `custom`. */
  color: string;
  /** Pixels, for the bars in the transfer list. */
  height: number;
  rounded: boolean;
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
  rounded: true,
  motion: "none",
  label: "percent",
  labelInside: false,
  stateColors: true,
};

/** Takes settings already merged over the defaults, so every field has the right type. */
export function sanitizeProgressBars(stored: ProgressBarSettings): ProgressBarSettings {
  const pick = <T extends string>(value: T, allowed: T[], fallback: T): T =>
    allowed.includes(value) ? value : fallback;
  const height = Number.isFinite(stored.height)
    ? Math.round(stored.height)
    : DEFAULT_PROGRESS_BARS.height;
  return {
    fill: pick(stored.fill, PROGRESS_FILLS, DEFAULT_PROGRESS_BARS.fill),
    coloring: pick(stored.coloring, PROGRESS_COLORINGS, DEFAULT_PROGRESS_BARS.coloring),
    color: HEX_COLOR.test(stored.color.trim())
      ? stored.color.trim().toLowerCase()
      : DEFAULT_PROGRESS_BARS.color,
    height: Math.min(PROGRESS_HEIGHT_LIMITS.max, Math.max(PROGRESS_HEIGHT_LIMITS.min, height)),
    rounded: stored.rounded,
    motion: pick(stored.motion, PROGRESS_MOTIONS, DEFAULT_PROGRESS_BARS.motion),
    label: pick(stored.label, PROGRESS_LABELS, DEFAULT_PROGRESS_BARS.label),
    labelInside: stored.labelInside,
    stateColors: stored.stateColors,
  };
}

/** The custom properties and attributes on the document that the stylesheet draws bars from. */
export function progressBarStyle(settings: ProgressBarSettings): {
  properties: Record<string, string>;
  attributes: Record<string, string>;
} {
  return {
    properties: {
      "--progress-height": `${settings.height}px`,
      "--progress-radius": settings.rounded ? "999px" : "0px",
      "--progress-custom": settings.color,
    },
    attributes: {
      progressFill: settings.fill,
      progressColoring: settings.coloring,
      progressMotion: settings.motion,
      progressStates: settings.stateColors ? "on" : "off",
    },
  };
}

/** The text a transfer's bar shows: empty when there is nothing to say for its state. */
export function progressLabel(
  job: Pick<JobSnapshot, "state" | "size" | "transferred" | "speed">,
  label: ProgressLabel,
  fraction: number,
): string {
  switch (label) {
    case "percent":
      return `${Math.floor(fraction * 100)}%`;
    case "bytes":
      if (job.state === "done") return formatSize(job.size);
      return `${formatSize(Math.min(job.transferred, job.size))} of ${formatSize(job.size)}`;
    case "remaining": {
      if (job.state === "done") return "Done";
      if (job.state !== "running" || job.speed <= 0) return "";
      const left = Math.max(0, job.size - job.transferred);
      return `${formatDuration(Math.ceil(left / job.speed))} left`;
    }
    case "none":
      return "";
  }
}
