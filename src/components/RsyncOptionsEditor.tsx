import { useState } from "react";
import { CircleAlert, TriangleAlert } from "lucide-react";
import {
  OPTION_GROUPS,
  changedCount,
  formatSizeArgument,
  linesOf,
  parseSize,
  unavailable,
  withOption,
  type OptionField,
} from "../lib/rsyncOptions";
import type { OptionCheck, SyncDirection, SyncOptions } from "../lib/types";

interface EditorProps {
  options: SyncOptions;
  direction: SyncDirection;
  onChange: (options: SyncOptions) => void;
  /** Groups shown open at first, by id; the first group when absent. */
  initiallyOpen?: string[];
}

/** rsync's options, grouped as its manual groups them, each with the flag it stands for. */
export function RsyncOptionsEditor({ options, direction, onChange, initiallyOpen }: EditorProps) {
  const open = initiallyOpen ?? [OPTION_GROUPS[0].id];
  return (
    <div className="rsync-options">
      {OPTION_GROUPS.map((group) => {
        const changed = changedCount(group, options);
        return (
          <details key={group.id} className="rsync-group" open={open.includes(group.id)}>
            <summary>
              <span>{group.title}</span>
              {changed > 0 && <span className="rsync-group-count">{changed} changed</span>}
            </summary>
            <div className="rsync-fields">
              {group.fields.map((field) => (
                <Field
                  key={field.key}
                  field={field}
                  options={options}
                  disabled={unavailable(field.key, direction)}
                  onChange={(key, value) => onChange(withOption(options, key, value, direction))}
                />
              ))}
            </div>
          </details>
        );
      })}
    </div>
  );
}

interface FieldProps {
  field: OptionField;
  options: SyncOptions;
  disabled: boolean;
  onChange: <K extends keyof SyncOptions>(key: K, value: SyncOptions[K]) => void;
}

function Field({ field, options, disabled, onChange }: FieldProps) {
  const flag =
    "flag" in field && field.flag ? <code className="rsync-flag">{field.flag}</code> : null;
  const hint = field.hint ? <small className="field-hint">{field.hint}</small> : null;
  const className = `rsync-field ${disabled ? "is-disabled" : ""}`;

  if (field.kind === "switch") {
    return (
      <div className={className}>
        <label className={`check ${disabled ? "is-disabled" : ""}`}>
          <input
            type="checkbox"
            checked={options[field.key] && !disabled}
            disabled={disabled}
            onChange={(event) => onChange(field.key, event.target.checked)}
          />
          {field.label}
          {flag}
        </label>
        {hint}
      </div>
    );
  }

  let control;
  switch (field.kind) {
    case "select":
      control = (
        <select
          value={options[field.key]}
          disabled={disabled}
          aria-label={field.label}
          onChange={(event) => onChange(field.key, event.target.value as never)}
        >
          {field.choices.map((choice) => (
            <option key={choice.value} value={choice.value}>
              {choice.label}
            </option>
          ))}
        </select>
      );
      break;
    case "number":
      control = (
        <NumberInput
          value={options[field.key]}
          max={field.max}
          unit={field.unit}
          label={field.label}
          disabled={disabled}
          onChange={(value) => onChange(field.key, value ?? 0)}
        />
      );
      break;
    case "optionalNumber":
      control = (
        <NumberInput
          value={options[field.key]}
          max={field.max}
          unit={field.unit}
          label={field.label}
          disabled={disabled}
          optional
          onChange={(value) => onChange(field.key, value)}
        />
      );
      break;
    case "size":
      control = (
        <SizeInput
          value={options[field.key]}
          label={field.label}
          disabled={disabled}
          onChange={(value) => onChange(field.key, value)}
        />
      );
      break;
    case "text":
      control = (
        <input
          className="mono"
          value={options[field.key]}
          placeholder={field.placeholder}
          aria-label={field.label}
          spellCheck={false}
          autoCapitalize="off"
          disabled={disabled}
          onChange={(event) => onChange(field.key, event.target.value)}
        />
      );
      break;
    case "list":
      control = (
        <ListInput
          value={options[field.key]}
          placeholder={field.placeholder}
          label={field.label}
          disabled={disabled}
          onChange={(value) => onChange(field.key, value)}
        />
      );
      break;
  }
  return (
    <div className={className}>
      <span className="rsync-field-label">
        {field.label}
        {flag}
      </span>
      {control}
      {hint}
    </div>
  );
}

interface NumberInputProps {
  value: number | null;
  max: number;
  unit?: string;
  label: string;
  disabled: boolean;
  /** Blank means no value. */
  optional?: boolean;
  onChange: (value: number | null) => void;
}

function NumberInput({ value, max, unit, label, disabled, optional, onChange }: NumberInputProps) {
  const [typed, setTyped] = useState<string | null>(null);
  return (
    <span className="rsync-number">
      <input
        type="number"
        min={0}
        max={max}
        step={1}
        value={typed ?? (value === null ? "" : String(value))}
        placeholder={optional ? "None" : undefined}
        aria-label={label}
        disabled={disabled}
        onChange={(event) => {
          const text = event.target.value;
          setTyped(text);
          if (text.trim() === "") {
            if (optional) onChange(null);
            return;
          }
          const number = Number(text);
          if (Number.isFinite(number)) onChange(Math.min(max, Math.max(0, Math.round(number))));
        }}
        onBlur={() => setTyped(null)}
      />
      {unit && <span className="rsync-unit">{unit}</span>}
    </span>
  );
}

interface SizeInputProps {
  value: number | null;
  label: string;
  disabled: boolean;
  onChange: (value: number | null) => void;
}

/** A size typed as rsync takes it, kept as typed while the box has focus. */
function SizeInput({ value, label, disabled, onChange }: SizeInputProps) {
  const [typed, setTyped] = useState<string | null>(null);
  const invalid = typed !== null && parseSize(typed) === undefined;
  return (
    <input
      className={`mono ${invalid ? "has-error" : ""}`}
      value={typed ?? formatSizeArgument(value)}
      placeholder="No limit"
      aria-label={label}
      aria-invalid={invalid}
      spellCheck={false}
      disabled={disabled}
      onChange={(event) => {
        setTyped(event.target.value);
        const size = parseSize(event.target.value);
        if (size !== undefined) onChange(size);
      }}
      onBlur={() => setTyped(null)}
    />
  );
}

interface ListInputProps {
  value: string[];
  placeholder?: string;
  label: string;
  disabled: boolean;
  onChange: (value: string[]) => void;
}

/** One item per line, kept as typed while the box has focus so blank lines can be typed. */
function ListInput({ value, placeholder, label, disabled, onChange }: ListInputProps) {
  const [typed, setTyped] = useState<string | null>(null);
  return (
    <textarea
      className="mono"
      value={typed ?? value.join("\n")}
      rows={Math.min(6, Math.max(2, value.length + 1))}
      placeholder={placeholder}
      aria-label={label}
      spellCheck={false}
      disabled={disabled}
      onChange={(event) => {
        setTyped(event.target.value);
        onChange(linesOf(event.target.value));
      }}
      onBlur={() => setTyped(null)}
    />
  );
}

interface ArgumentsProps {
  value: string;
  label?: string;
  hint?: string;
  placeholder?: string;
  onChange: (value: string) => void;
}

/** rsync arguments typed as on its command line. */
export function RsyncArgumentsField({
  value,
  label = "Extra rsync arguments",
  hint = "Typed as on rsync's command line; they apply over the options above.",
  placeholder = "--delete-after --chmod=D755,F644 --exclude=*.log",
  onChange,
}: ArgumentsProps) {
  return (
    <label className="field">
      <span>{label}</span>
      <input
        className="mono"
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(event) => onChange(event.target.value)}
      />
      <small className="field-hint">{hint}</small>
    </label>
  );
}

interface CheckViewProps {
  check: OptionCheck | null;
  /** The command line, as `commandPreview` gives it. */
  command?: string;
}

/** The command the options amount to, and what the backend noted about them. */
export function OptionCheckView({ check, command }: CheckViewProps) {
  if (!check) return null;
  return (
    <div className="rsync-check">
      {command && <code className="rsync-command selectable">{command}</code>}
      {check.errors.map((message) => (
        <p key={message} className="rsync-note is-error">
          <CircleAlert size={14} />
          {message}
        </p>
      ))}
      {check.warnings.map((message) => (
        <p key={message} className="rsync-note warning-text">
          <TriangleAlert size={14} />
          {message}
        </p>
      ))}
    </div>
  );
}
