import type { CSSProperties } from "react";
import { progressReach, type ProgressLabel } from "../lib/progressBars";

interface ProgressBarProps {
  /** From 0 to 1, or null for work of unknown length, whose bar keeps moving. */
  fraction: number | null;
  /** Shown beside the bar, or on it with `labelInside`. */
  label: string;
  /** What the label says, which sets how much room beside the bar it takes. */
  labelKind: ProgressLabel;
  labelInside: boolean;
  /** The work is not running, so the bar's motion holds still. */
  still?: boolean;
}

/**
 * A transfer's bar. The stylesheet takes how far it reaches from a custom property, so a look
 * can round the reach to its cells. Text on the bar is drawn twice, the copy over the fill in
 * the color that reads on it, clipped to where the fill reaches.
 */
export function ProgressBar({
  fraction,
  label,
  labelKind,
  labelInside,
  still = false,
}: ProgressBarProps) {
  const known = fraction !== null;
  const inside = labelInside && labelKind !== "none";
  const classes = ["progress-track"];
  if (inside) classes.push("has-label");
  if (still) classes.push("is-still");
  if (!known) classes.push("is-indeterminate");
  return (
    <>
      <span
        className={classes.join(" ")}
        style={known ? (progressReach(fraction) as CSSProperties) : undefined}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={known ? Math.floor(Math.min(1, Math.max(0, fraction)) * 100) : undefined}
        aria-valuetext={label || undefined}
      >
        <span className="progress-fill" />
        {inside && label !== "" && (
          <>
            <span className="progress-label" aria-hidden>
              <span>{label}</span>
            </span>
            <span className="progress-label is-over" aria-hidden>
              <span>{label}</span>
            </span>
          </>
        )}
      </span>
      {!inside && labelKind !== "none" && (
        <span className="progress-text" data-label={labelKind}>
          {label}
        </span>
      )}
    </>
  );
}
