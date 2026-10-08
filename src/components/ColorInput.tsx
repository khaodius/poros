import { useCallback, useEffect, useRef, useState, type InputHTMLAttributes } from "react";

/** How long a picker must rest before the color is saved. */
const SETTLE_MILLIS = 250;

interface ColorInputProps extends Omit<
  InputHTMLAttributes<HTMLInputElement>,
  "type" | "value" | "onChange"
> {
  /** The saved color, as `#rrggbb`. */
  value: string;
  /** Shows a color on the page without saving it, at most once a frame. */
  onPreview: (color: string) => void;
  /** Saves a color once the picker rests or closes. */
  onCommit: (color: string) => void;
}

/**
 * A color input that keeps up with a picker dragged quickly: it shows each frame's color on the
 * page without saving it, and saves only once the picker rests or closes. Saving on every input
 * event re-renders the settings and rewrites the theme file faster than the picker moves.
 */
export function ColorInput({ value, onPreview, onCommit, ...inputProps }: ColorInputProps) {
  const [picked, setPicked] = useState<string | null>(null);
  const [savedValue, setSavedValue] = useState(value);
  if (savedValue !== value) {
    // A newly saved color, from this picker or from elsewhere, replaces the pick.
    setSavedValue(value);
    setPicked(null);
  }

  const inputRef = useRef<HTMLInputElement>(null);
  const handlers = useRef({ onPreview, onCommit });
  const pending = useRef<string | null>(null);
  const frame = useRef(0);
  const settleTimer = useRef(0);

  useEffect(() => {
    handlers.current = { onPreview, onCommit };
  });

  const save = useCallback(() => {
    cancelAnimationFrame(frame.current);
    window.clearTimeout(settleTimer.current);
    frame.current = 0;
    const color = pending.current;
    pending.current = null;
    if (color !== null) handlers.current.onCommit(color);
  }, []);

  // The native change event fires when the picker closes; React's onChange fires on every move.
  useEffect(() => {
    const input = inputRef.current;
    input?.addEventListener("change", save);
    return () => {
      input?.removeEventListener("change", save);
      save();
    };
  }, [save]);

  return (
    <input
      {...inputProps}
      ref={inputRef}
      type="color"
      value={picked ?? value}
      onChange={(event) => {
        const color = event.target.value;
        setPicked(color);
        pending.current = color;
        frame.current ||= requestAnimationFrame(() => {
          frame.current = 0;
          if (pending.current !== null) handlers.current.onPreview(pending.current);
        });
        window.clearTimeout(settleTimer.current);
        settleTimer.current = window.setTimeout(save, SETTLE_MILLIS);
      }}
    />
  );
}
