import { useState, type ReactNode } from "react";

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

export function NumberSetting(props: NumberProps) {
  // Remounts with the stored value, so a clamped or reloaded value replaces what was typed.
  return <NumberInput key={props.value} {...props} />;
}

function NumberInput({ label, hint, value, min, max, unit, disabled, onChange }: NumberProps) {
  const [text, setText] = useState(String(value));
  const commit = () => {
    const parsed = Number(text);
    const next = Number.isFinite(parsed) && text.trim() !== "" ? parsed : value;
    const clamped = Math.min(max, Math.max(min, Math.round(next)));
    setText(String(clamped));
    if (clamped !== value) onChange(clamped);
  };
  return (
    <SettingRow label={label} hint={hint} disabled={disabled}>
      <span className="number-input">
        <input
          value={text}
          inputMode="numeric"
          aria-label={label}
          disabled={disabled}
          onChange={(event) => setText(event.target.value.replace(/[^\d]/g, ""))}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
          }}
        />
        <span className="number-unit">{unit}</span>
      </span>
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
