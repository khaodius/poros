import { useRef, useState } from "react";
import { Minus, Plus } from "lucide-react";
import { useHoldRepeat } from "../hooks/useHoldRepeat";

interface StepperProps {
  value: number;
  min: number;
  max: number;
  label: string;
  unit?: string;
  disabled?: boolean;
  /** Called once a typed value is committed or a held button is let go. */
  onChange: (value: number) => void;
}

const PAGE_STEP = 10;

/**
 * A number box between minus and plus buttons that repeat, speeding up, while held. The buttons
 * stay enabled during a hold, because a disabled button would swallow the release.
 */
export function Stepper({ value, min, max, label, unit, disabled, onChange }: StepperProps) {
  const [typed, setTyped] = useState<string | null>(null);
  const [held, setHeld] = useState<number | null>(null);
  const heldValue = useRef<number | null>(null);
  const clamp = (next: number) => Math.min(max, Math.max(min, Math.round(next)));

  const commit = (next: number) => {
    const clamped = clamp(next);
    if (clamped !== value) onChange(clamped);
  };
  const holdStep = (by: number) => {
    const next = clamp((heldValue.current ?? value) + by);
    heldValue.current = next;
    setHeld(next);
  };
  const release = () => {
    const next = heldValue.current;
    heldValue.current = null;
    setHeld(null);
    if (next !== null) commit(next);
  };
  const decrease = useHoldRepeat(() => holdStep(-1), release);
  const increase = useHoldRepeat(() => holdStep(1), release);
  const commitTyped = () => {
    if (typed) commit(Number(typed));
    setTyped(null);
  };

  return (
    <span className={`stepper ${disabled ? "is-disabled" : ""}`} role="group" aria-label={label}>
      <button
        type="button"
        aria-label={`Decrease ${label.toLowerCase()}`}
        disabled={disabled || (held === null && value <= min)}
        {...decrease}
        onClick={(event) => event.detail === 0 && commit(value - 1)}
      >
        <Minus size={12} />
      </button>
      <span className="stepper-value">
        <input
          value={typed ?? held ?? value}
          inputMode="numeric"
          aria-label={label}
          disabled={disabled}
          style={{ width: `${String(max).length + 1.5}ch` }}
          onFocus={(event) => event.currentTarget.select()}
          onChange={(event) =>
            setTyped(event.target.value.replace(/\D/g, "").slice(0, String(max).length))
          }
          onBlur={commitTyped}
          onKeyDown={(event) => {
            const steps: Record<string, number> = {
              ArrowUp: 1,
              ArrowDown: -1,
              PageUp: PAGE_STEP,
              PageDown: -PAGE_STEP,
            };
            if (event.key === "Enter") event.currentTarget.blur();
            else if (event.key === "Escape") setTyped(null);
            else if (event.key in steps) {
              event.preventDefault();
              setTyped(null);
              commit(value + steps[event.key]);
            }
          }}
        />
        {unit && <span className="stepper-unit">{unit}</span>}
      </span>
      <button
        type="button"
        aria-label={`Increase ${label.toLowerCase()}`}
        disabled={disabled || (held === null && value >= max)}
        {...increase}
        onClick={(event) => event.detail === 0 && commit(value + 1)}
      >
        <Plus size={12} />
      </button>
    </span>
  );
}
