import type { ProgressLabel } from "../lib/progressBars";

interface ProgressBarProps {
  /** From 0 to 1. */
  fraction: number;
  /** Shown beside the bar, or on it with `labelInside`. */
  label: string;
  /** What the label says, which sets how much room beside the bar it takes. */
  labelKind: ProgressLabel;
  labelInside: boolean;
  /** The work is not running, so the bar's motion holds still. */
  still?: boolean;
}

/**
 * A transfer's bar. Text on the bar is drawn twice, the copy over the fill in the color that
 * reads on it, clipped to where the fill reaches.
 */
export function ProgressBar({
  fraction,
  label,
  labelKind,
  labelInside,
  still = false,
}: ProgressBarProps) {
  const percent = Math.min(100, Math.max(0, fraction * 100));
  const inside = labelInside && labelKind !== "none";
  return (
    <>
      <span
        className={`progress-track ${inside ? "has-label" : ""} ${still ? "is-still" : ""}`}
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.floor(percent)}
        aria-valuetext={label || undefined}
      >
        <span className="progress-fill" style={{ width: `${percent}%` }} />
        {inside && label !== "" && (
          <>
            <span className="progress-label" aria-hidden>
              {label}
            </span>
            <span
              className="progress-label is-over"
              style={{ clipPath: `inset(0 ${100 - percent}% 0 0)` }}
              aria-hidden
            >
              {label}
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
