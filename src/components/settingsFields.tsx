import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { RotateCcw } from "lucide-react";
import { Stepper } from "./Stepper";

interface RowProps {
  label: string;
  hint?: string;
  disabled?: boolean;
  children: ReactNode;
}

export function SettingRow({ label, hint, disabled, children }: RowProps) {
  return (
    <div className={`setting-row ${disabled ? "is-disabled" : ""}`}>
      <div className="setting-text">
        <span className="setting-label">{label}</span>
        {hint && <span className="setting-hint">{hint}</span>}
      </div>
      <div className="setting-control">{children}</div>
    </div>
  );
}

export function SettingGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="setting-group">
      <h3>{title}</h3>
      {children}
    </section>
  );
}

interface SwitchProps {
  label: string;
  hint?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}

export function SwitchSetting({ label, hint, checked, disabled, onChange }: SwitchProps) {
  return (
    <SettingRow label={label} hint={hint} disabled={disabled}>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className={`switch ${checked ? "is-on" : ""}`}
        disabled={disabled}
        onClick={() => onChange(!checked)}
      >
        <span className="switch-thumb" />
      </button>
    </SettingRow>
  );
}

interface CommitInputProps {
  value: string;
  label: string;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  onCommit: (value: string) => void;
}

/** Saves when the box loses focus or Enter is pressed, so the backend's tidying does not fight typing. */
export function CommitInput({
  value,
  label,
  placeholder,
  disabled,
  className,
  onCommit,
}: CommitInputProps) {
  const [typed, setTyped] = useState<string | null>(null);
  const commit = () => {
    if (typed !== null && typed !== value) onCommit(typed);
    setTyped(null);
  };
  return (
    <input
      className={className}
      value={typed ?? value}
      aria-label={label}
      placeholder={placeholder}
      spellCheck={false}
      disabled={disabled}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") commit();
        if (event.key === "Escape" && typed !== null) {
          event.preventDefault();
          setTyped(null);
        }
      }}
    />
  );
}

interface NumberProps {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  unit?: string;
  disabled?: boolean;
  onChange: (value: number) => void;
}

export function NumberSetting({
  label,
  hint,
  value,
  min,
  max,
  unit,
  disabled,
  onChange,
}: NumberProps) {
  return (
    <SettingRow label={label} hint={hint} disabled={disabled}>
      <Stepper
        value={value}
        min={min}
        max={max}
        label={label}
        unit={unit}
        disabled={disabled}
        onChange={onChange}
      />
    </SettingRow>
  );
}

interface SelectProps<T extends string> {
  label: string;
  hint?: string;
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
}

export function SelectSetting<T extends string>({
  label,
  hint,
  value,
  options,
  onChange,
}: SelectProps<T>) {
  return (
    <SettingRow label={label} hint={hint}>
      <select
        value={value}
        aria-label={label}
        onChange={(event) => onChange(event.target.value as T)}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </SettingRow>
  );
}

interface TextSettingProps {
  label: string;
  hint?: string;
  value: string;
  placeholder?: string;
  disabled?: boolean;
  /** For long values such as app IDs. */
  wide?: boolean;
  onCommit: (value: string) => void;
}

export function TextSetting({
  label,
  hint,
  value,
  placeholder,
  disabled,
  wide,
  onCommit,
}: TextSettingProps) {
  return (
    <SettingRow label={label} hint={hint} disabled={disabled}>
      <CommitInput
        value={value}
        label={label}
        placeholder={placeholder}
        disabled={disabled}
        className={wide ? "is-wide" : undefined}
        onCommit={onCommit}
      />
    </SettingRow>
  );
}

const RANGE_SAVE_DELAY_MILLIS = 300;

interface RangeProps {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  format: (value: number) => string;
  /** Shows a value while the slider moves. */
  onPreview: (value: number) => void;
  /** Saves the value the slider settled on. */
  onCommit: (value: number) => void;
  /** Offered as a button when given. */
  onReset?: () => void;
  resetLabel?: string;
}

export function RangeSetting({
  label,
  hint,
  value,
  min,
  max,
  format,
  onPreview,
  onCommit,
  onReset,
  resetLabel,
}: RangeProps) {
  const pending = useRef<{ timer: number; value: number } | null>(null);
  const commit = useRef(onCommit);
  useLayoutEffect(() => {
    commit.current = onCommit;
  });
  useEffect(
    () => () => {
      if (!pending.current) return;
      window.clearTimeout(pending.current.timer);
      commit.current(pending.current.value);
    },
    [],
  );

  const change = (next: number) => {
    onPreview(next);
    if (pending.current) window.clearTimeout(pending.current.timer);
    pending.current = {
      value: next,
      timer: window.setTimeout(() => {
        pending.current = null;
        commit.current(next);
      }, RANGE_SAVE_DELAY_MILLIS),
    };
  };

  return (
    <SettingRow label={label} hint={hint}>
      <span className="range">
        {onReset && (
          <button
            type="button"
            className="icon-button"
            title={resetLabel}
            aria-label={resetLabel}
            onClick={onReset}
          >
            <RotateCcw size={13} />
          </button>
        )}
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          aria-label={label}
          onChange={(event) => change(Number(event.target.value))}
        />
        <span className="range-value">{format(value)}</span>
      </span>
    </SettingRow>
  );
}
