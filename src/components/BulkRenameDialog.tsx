import {
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import {
  COUNTER_DIGIT_LIMITS,
  DATE_STYLES,
  DEFAULT_RULES,
  EXTENSION_CHANGES,
  FIND_MODES,
  FIND_SCOPES,
  LETTER_CASES,
  TEMPLATE_TOKENS,
  TIME_STYLES,
  findHistoryLabel,
  folderName,
  previewRenames,
  problemSummary,
  renamesOf,
  templateIsActive,
  type CounterRule,
  type DateStyle,
  type ExtensionChange,
  type ExtensionRule,
  type FindMode,
  type FindRule,
  type FindScope,
  type LetterCase,
  type NameTarget,
  type RenamePreview,
  type RenameSelection,
  type TemplateRule,
  type TidyRule,
  type TimeStyle,
} from "../lib/bulkRename";
import type { FileOrigin } from "../lib/fileOrigin";
import { pluralize } from "../lib/format";
import { isDirLike } from "../lib/sort";
import type { FileEntry } from "../lib/types";
import { knownTraits, nameTargetOf, runBulkRename, useBulkRenameStore } from "../state/bulkRename";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";
import { RenameTable, type RenameTableRow } from "./RenameTable";

const FIND_MODE_LABELS: Record<FindMode, string> = { text: "Text", regex: "Regex" };
const FIND_SCOPE_LABELS: Record<FindScope, string> = {
  name: "Name",
  extension: "Extension",
  both: "Both",
};
const LETTER_CASE_LABELS: Record<LetterCase, string> = {
  unchanged: "Unchanged",
  lower: "lowercase",
  upper: "UPPERCASE",
  title: "Title Case",
  sentence: "Sentence case",
};
const EXTENSION_LABELS: Record<ExtensionChange, string> = {
  keep: "Keep",
  lower: "lowercase",
  upper: "UPPERCASE",
  replace: "Replace with",
  remove: "Remove",
};
const COUNTER_LIMIT = 999_999_999;

type CounterPlace = "off" | CounterRule["placement"];
const COUNTER_PLACE_LABELS: Record<CounterPlace, string> = {
  off: "No",
  prefix: "Before the name",
  suffix: "After the name",
};

type PreviewFilter = "all" | "changed" | "problems";
const FILTER_LABELS: Record<PreviewFilter, string> = {
  all: "All",
  changed: "Changed",
  problems: "Problems",
};

interface BulkRenameDialogProps {
  origin: FileOrigin;
  /** The folder the items are in. */
  folder: string;
  /** The items to rename, in the order the list shows them. */
  entries: readonly FileEntry[];
  /** Everything the folder lists, selected or not. */
  listed: readonly FileEntry[];
  onClose: () => void;
}

/** Rules on the left, every item's old and new name on the right, and nothing runs until Rename. */
export function BulkRenameDialog({
  origin,
  folder,
  entries,
  listed,
  onClose,
}: BulkRenameDialogProps) {
  const [rules, setRules] = useState(() => useBulkRenameStore.getState().rules);
  const [filter, setFilter] = useState<PreviewFilter>("all");
  const history = useSettingsStore((state) => state.settings.interface.renameHistory);
  const templateRef = useRef<HTMLInputElement>(null);
  /** Where the cursor goes in the template once a token chip has put its token in. */
  const caretAfterToken = useRef<number | null>(null);

  const selection = useMemo<RenameSelection>(() => {
    const chosen = new Set(entries.map((entry) => entry.path));
    return {
      items: entries.map((entry) => ({
        name: entry.name,
        isFolder: isDirLike(entry),
        modified: entry.modified,
      })),
      parent: folderName(folder),
      others: listed.filter((entry) => !chosen.has(entry.path)).map((entry) => entry.name),
    };
  }, [entries, listed, folder]);
  const held = useMemo(() => listed.map((entry) => entry.name), [listed]);

  // Whether case matters in the folder may take asking it, so the preview starts from what its
  // kind says and Rename waits for the answer.
  const [target, setTarget] = useState<NameTarget | null>(null);
  const assumedTarget = useMemo<NameTarget>(() => {
    const traits = knownTraits(origin, folder);
    return { ...traits, caseInsensitive: traits.caseInsensitive ?? false };
  }, [origin, folder]);
  useEffect(() => {
    let wanted = true;
    void nameTargetOf(origin, folder, held).then((found) => {
      if (wanted) setTarget(found);
    });
    return () => {
      wanted = false;
    };
  }, [origin, folder, held]);

  // Typing stays quick with tens of thousands of items: the preview follows a moment behind.
  const previewedRules = useDeferredValue(rules);
  const preview = useMemo(
    () => previewRenames(selection, previewedRules, target ?? assumedTarget),
    [selection, previewedRules, target, assumedTarget],
  );
  const changedCount = preview.rows.length - preview.unchanged;
  const filterCounts: Record<PreviewFilter, number> = {
    all: preview.rows.length,
    changed: changedCount,
    problems: preview.problemCount,
  };
  const shownFilter = filter === "problems" && preview.problemCount === 0 ? "all" : filter;
  /** The positions of the rows the filter keeps, or null when it keeps them all. */
  const shownRows = useMemo(() => {
    if (shownFilter === "all") return null;
    const positions: number[] = [];
    preview.rows.forEach((row, index) => {
      const keep =
        shownFilter === "problems" ? row.status === "problem" : row.status !== "unchanged";
      if (keep) positions.push(index);
    });
    return positions;
  }, [preview, shownFilter]);

  useLayoutEffect(() => {
    const caret = caretAfterToken.current;
    if (caret === null) return;
    caretAfterToken.current = null;
    templateRef.current?.focus();
    templateRef.current?.setSelectionRange(caret, caret);
  }, [rules.template.pattern]);

  const changeFind = (change: Partial<FindRule>) =>
    setRules((current) => ({ ...current, find: { ...current.find, ...change } }));
  const changeTemplate = (change: Partial<TemplateRule>) =>
    setRules((current) => ({ ...current, template: { ...current.template, ...change } }));
  const changeCounter = (change: Partial<CounterRule>) =>
    setRules((current) => ({ ...current, counter: { ...current.counter, ...change } }));
  const changeExtension = (change: Partial<ExtensionRule>) =>
    setRules((current) => ({ ...current, extension: { ...current.extension, ...change } }));
  const changeTidy = (change: Partial<TidyRule>) =>
    setRules((current) => ({ ...current, tidy: { ...current.tidy, ...change } }));

  const insertToken = (token: string) => {
    const input = templateRef.current;
    const pattern = rules.template.pattern;
    const start = input?.selectionStart ?? pattern.length;
    const end = input?.selectionEnd ?? start;
    caretAfterToken.current = start + token.length;
    changeTemplate({ pattern: pattern.slice(0, start) + token + pattern.slice(end) });
  };

  const canRename = (checked: RenamePreview) =>
    target !== null &&
    checked.findError === null &&
    checked.problemCount === 0 &&
    checked.ready > 0;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!target) return;
    // Enter may arrive before the preview has caught up with the last key.
    const checked = previewedRules === rules ? preview : previewRenames(selection, rules, target);
    if (!canRename(checked)) return;
    onClose();
    void runBulkRename({ origin, folder, renames: renamesOf(checked), held, target, rules });
  };

  const tableRow = (index: number): RenameTableRow => {
    const row = preview.rows[shownRows ? shownRows[index] : index];
    if (row.status === "problem") {
      return { ...row, tone: "bad", status: row.message ?? "Cannot be renamed" };
    }
    return row.status === "unchanged"
      ? { ...row, tone: "dim", status: "Unchanged" }
      : { ...row, tone: "plain", status: "Ready" };
  };

  const templateActive = templateIsActive(rules.template);
  const counterPlace: CounterPlace = rules.counter.enabled ? rules.counter.placement : "off";
  const problems = problemSummary(preview);

  return (
    <Dialog
      title={`Rename ${entries.length} items`}
      onClose={onClose}
      width={1120}
      className="bulk-rename-dialog"
    >
      <form className="rename-form" onSubmit={submit}>
        <div className="rename-rules">
          <p className="field-hint">Applied in this order, each to the result of the last.</p>

          <RuleSection
            step={1}
            title="Find and replace"
            enabled={rules.find.enabled}
            onToggle={(enabled) => changeFind({ enabled })}
          >
            <div className="rename-find">
              <input
                data-autofocus
                className={`mono ${preview.findError ? "has-error" : ""}`}
                aria-label="Find"
                aria-invalid={preview.findError !== null}
                placeholder={rules.find.mode === "regex" ? "Regular expression" : "Find"}
                spellCheck={false}
                value={rules.find.pattern}
                onChange={(event) => changeFind({ pattern: event.target.value })}
              />
              {history.length > 0 && (
                <select
                  aria-label="Recent patterns"
                  value=""
                  onChange={(event) => {
                    const entry = history.find(
                      (known) => findHistoryLabel(known) === event.target.value,
                    );
                    if (entry) changeFind({ ...entry, enabled: true });
                  }}
                >
                  <option value="" disabled>
                    Recent
                  </option>
                  {history.map((entry) => {
                    const label = findHistoryLabel(entry);
                    return (
                      <option key={label} value={label}>
                        {label}
                      </option>
                    );
                  })}
                </select>
              )}
            </div>
            {preview.findError && (
              <p className="form-error" role="alert">
                {preview.findError}
              </p>
            )}
            <input
              className="mono"
              aria-label="Replace with"
              placeholder={
                rules.find.mode === "regex"
                  ? "Replace with, $1 or $<name> for groups"
                  : "Replace with"
              }
              spellCheck={false}
              value={rules.find.replacement}
              onChange={(event) => changeFind({ replacement: event.target.value })}
            />
            <div className="rename-options">
              <Choice
                label="Pattern is"
                options={FIND_MODES}
                labels={FIND_MODE_LABELS}
                value={rules.find.mode}
                onChange={(mode) => changeFind({ mode })}
              />
              <label className="check">
                <input
                  type="checkbox"
                  checked={rules.find.caseSensitive}
                  onChange={(event) => changeFind({ caseSensitive: event.target.checked })}
                />
                Match case
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={rules.find.allMatches}
                  onChange={(event) => changeFind({ allMatches: event.target.checked })}
                />
                Every match
              </label>
            </div>
            <div className="rename-options">
              <span className="rename-option-label">Look in</span>
              <Choice
                label="Look in"
                options={FIND_SCOPES}
                labels={FIND_SCOPE_LABELS}
                value={rules.find.scope}
                onChange={(scope) => changeFind({ scope })}
              />
            </div>
          </RuleSection>

          <RuleSection
            step={2}
            title="Template"
            enabled={rules.template.enabled}
            onToggle={(enabled) => changeTemplate({ enabled })}
          >
            <input
              ref={templateRef}
              className="mono"
              aria-label="Template for the new name"
              placeholder="{name}-{n}.{ext}"
              spellCheck={false}
              value={rules.template.pattern}
              onChange={(event) => changeTemplate({ pattern: event.target.value })}
            />
            <div className="rename-tokens" role="group" aria-label="Insert a token">
              {TEMPLATE_TOKENS.map(({ token, meaning }) => (
                <button
                  key={token}
                  type="button"
                  className="rename-token mono"
                  title={meaning}
                  onClick={() => insertToken(token)}
                >
                  {token}
                </button>
              ))}
            </div>
            <div className="form-row">
              <label className="field grow">
                <span>Date as</span>
                <select
                  value={rules.template.dateStyle}
                  onChange={(event) =>
                    changeTemplate({ dateStyle: event.target.value as DateStyle })
                  }
                >
                  {DATE_STYLES.map((style) => (
                    <option key={style}>{style}</option>
                  ))}
                </select>
              </label>
              <label className="field grow">
                <span>Time as</span>
                <select
                  value={rules.template.timeStyle}
                  onChange={(event) =>
                    changeTemplate({ timeStyle: event.target.value as TimeStyle })
                  }
                >
                  {TIME_STYLES.map((style) => (
                    <option key={style}>{style}</option>
                  ))}
                </select>
              </label>
            </div>
            <p className="field-hint">
              The date and time are when the item was last modified. Without {"{ext}"}, items keep
              their extension.
            </p>
          </RuleSection>

          <RuleSection step={3} title="Counter">
            <div className="form-row">
              <WholeNumberField
                label="Start"
                value={rules.counter.start}
                min={-COUNTER_LIMIT}
                max={COUNTER_LIMIT}
                onChange={(start) => changeCounter({ start })}
              />
              <WholeNumberField
                label="Step"
                value={rules.counter.step}
                min={-COUNTER_LIMIT}
                max={COUNTER_LIMIT}
                onChange={(step) => changeCounter({ step })}
              />
              <WholeNumberField
                label="Digits"
                value={rules.counter.digits}
                min={COUNTER_DIGIT_LIMITS.min}
                max={COUNTER_DIGIT_LIMITS.max}
                onChange={(digits) => changeCounter({ digits })}
              />
              <label className="field grow">
                <span>Counts in</span>
                <select
                  value={rules.counter.order}
                  onChange={(event) =>
                    changeCounter({ order: event.target.value as CounterRule["order"] })
                  }
                >
                  <option value="list">List order</option>
                  <option value="name">Name order</option>
                </select>
              </label>
            </div>
            {templateActive ? (
              <p className="field-hint">The template places the counter where {"{n}"} is.</p>
            ) : (
              <div className="form-row">
                <label className="field grow">
                  <span>Add to each name</span>
                  <select
                    value={counterPlace}
                    onChange={(event) => {
                      const place = event.target.value as CounterPlace;
                      changeCounter(
                        place === "off" ? { enabled: false } : { enabled: true, placement: place },
                      );
                    }}
                  >
                    {Object.entries(COUNTER_PLACE_LABELS).map(([place, label]) => (
                      <option key={place} value={place}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field rename-short-field">
                  <span>Separator</span>
                  <input
                    className="mono"
                    maxLength={8}
                    spellCheck={false}
                    disabled={!rules.counter.enabled}
                    value={rules.counter.separator}
                    onChange={(event) => changeCounter({ separator: event.target.value })}
                  />
                </label>
              </div>
            )}
          </RuleSection>

          <RuleSection
            step={4}
            title="Case"
            control={
              <select
                aria-label="Case"
                value={rules.letterCase}
                onChange={(event) =>
                  setRules((current) => ({
                    ...current,
                    letterCase: event.target.value as LetterCase,
                  }))
                }
              >
                {LETTER_CASES.map((letterCase) => (
                  <option key={letterCase} value={letterCase}>
                    {LETTER_CASE_LABELS[letterCase]}
                  </option>
                ))}
              </select>
            }
          />

          <RuleSection
            step={5}
            title="Extension"
            control={
              <select
                aria-label="Extension"
                value={rules.extension.change}
                onChange={(event) =>
                  changeExtension({ change: event.target.value as ExtensionChange })
                }
              >
                {EXTENSION_CHANGES.map((change) => (
                  <option key={change} value={change}>
                    {EXTENSION_LABELS[change]}
                  </option>
                ))}
              </select>
            }
          >
            {rules.extension.change === "replace" && (
              <input
                className="mono"
                aria-label="New extension"
                placeholder="New extension, such as jpg"
                spellCheck={false}
                value={rules.extension.replacement}
                onChange={(event) => changeExtension({ replacement: event.target.value })}
              />
            )}
          </RuleSection>

          <RuleSection
            step={6}
            title="Trim and tidy"
            enabled={rules.tidy.enabled}
            onToggle={(enabled) => changeTidy({ enabled })}
          >
            <label className="check">
              <input
                type="checkbox"
                checked={rules.tidy.trim}
                onChange={(event) => changeTidy({ trim: event.target.checked })}
              />
              Trim spaces at the ends
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={rules.tidy.collapseSpaces}
                onChange={(event) => changeTidy({ collapseSpaces: event.target.checked })}
              />
              Make repeated spaces one
            </label>
            <div className="rename-options">
              <label className="check">
                <input
                  type="checkbox"
                  checked={rules.tidy.replaceSpaces}
                  onChange={(event) => changeTidy({ replaceSpaces: event.target.checked })}
                />
                Replace spaces with
              </label>
              <input
                className="mono rename-short-field"
                aria-label="Character that replaces spaces"
                maxLength={8}
                spellCheck={false}
                disabled={!rules.tidy.replaceSpaces}
                value={rules.tidy.spaceReplacement}
                onChange={(event) => changeTidy({ spaceReplacement: event.target.value })}
              />
            </div>
          </RuleSection>
        </div>

        <div className="rename-preview">
          <div className="rename-preview-bar">
            <div className="toggle-group" role="group" aria-label="Rows shown">
              {(Object.keys(FILTER_LABELS) as PreviewFilter[]).map((value) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={shownFilter === value}
                  className={`toggle ${shownFilter === value ? "is-on" : ""} ${
                    value === "problems" && preview.problemCount > 0 ? "is-bad" : ""
                  }`}
                  disabled={value === "problems" && preview.problemCount === 0}
                  onClick={() => setFilter(value)}
                >
                  {FILTER_LABELS[value]} {filterCounts[value]}
                </button>
              ))}
            </div>
            <span className="rename-folder" title={folder}>
              <bdi>{folder}</bdi>
            </span>
          </div>
          <RenameTable
            label="Old and new names"
            count={shownRows ? shownRows.length : preview.rows.length}
            row={tableRow}
            empty="No names change yet."
          />
        </div>

        <div className="rename-footer">
          {problems ? (
            <p className="form-error" role="status">
              {problems}
            </p>
          ) : (
            <p className="rename-summary" role="status">
              {preview.ready === 0
                ? "No names change yet."
                : `${preview.ready} of ${pluralize(preview.rows.length, "item")} will be renamed${
                    preview.unchanged > 0 ? `, ${preview.unchanged} unchanged` : ""
                  }.`}
            </p>
          )}
          <div className="dialog-actions">
            <button
              type="button"
              className="button push-left"
              disabled={rules === DEFAULT_RULES}
              onClick={() => setRules(DEFAULT_RULES)}
            >
              Reset rules
            </button>
            <button type="button" className="button" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="button button-primary" disabled={!canRename(preview)}>
              {canRename(preview) ? `Rename ${pluralize(preview.ready, "item")}` : "Rename"}
            </button>
          </div>
        </div>
      </form>
    </Dialog>
  );
}

interface RuleSectionProps {
  /** Where the rule comes in the order the rules apply. */
  step: number;
  title: string;
  /** Given with `onToggle` for a rule with a switch; its fields show while it is on. */
  enabled?: boolean;
  onToggle?: (enabled: boolean) => void;
  /** Shown in the heading in place of a switch. */
  control?: ReactNode;
  children?: ReactNode;
}

function RuleSection({ step, title, enabled, onToggle, control, children }: RuleSectionProps) {
  const open = enabled !== false;
  return (
    <section className={`rename-rule ${open ? "" : "is-off"}`}>
      <header>
        <span className="rename-step" aria-hidden="true">
          {step}
        </span>
        <h3>{title}</h3>
        {control}
        {onToggle && (
          <button
            type="button"
            role="switch"
            aria-checked={open}
            aria-label={title}
            className={`switch ${open ? "is-on" : ""}`}
            onClick={() => onToggle(!open)}
          >
            <span className="switch-thumb" />
          </button>
        )}
      </header>
      {open && children}
    </section>
  );
}

interface ChoiceProps<Option extends string> {
  label: string;
  options: readonly Option[];
  labels: Record<Option, string>;
  value: Option;
  onChange: (value: Option) => void;
}

/** A few options side by side, one of them picked. */
function Choice<Option extends string>({
  label,
  options,
  labels,
  value,
  onChange,
}: ChoiceProps<Option>) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option}
          type="button"
          aria-pressed={value === option}
          className={value === option ? "is-selected" : ""}
          onClick={() => onChange(option)}
        >
          {labels[option]}
        </button>
      ))}
    </div>
  );
}

interface WholeNumberFieldProps {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
}

/** A whole number that may be negative. What is typed stays as typed until it is a number. */
function WholeNumberField({ label, value, min, max, onChange }: WholeNumberFieldProps) {
  const [typed, setTyped] = useState<string | null>(null);
  return (
    <label className="field rename-number-field">
      <span>{label}</span>
      <input
        type="number"
        inputMode="numeric"
        step={1}
        min={min}
        max={max}
        value={typed ?? String(value)}
        onChange={(event) => {
          const text = event.target.value;
          setTyped(text);
          const number = Number(text);
          if (text.trim() !== "" && Number.isInteger(number)) {
            onChange(Math.min(max, Math.max(min, number)));
          }
        }}
        onBlur={() => setTyped(null)}
      />
    </label>
  );
}
