import {
  useDeferredValue,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { TriangleAlert } from "lucide-react";
import type { Staleness } from "../hooks/usePane";
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
import { entriesStillListed } from "../lib/deletion";
import type { FileOrigin } from "../lib/fileOrigin";
import { isDirLike } from "../lib/sort";
import type { DirListing, FileEntry } from "../lib/types";
import { knownTraits, nameTargetOf, runBulkRename, useBulkRenameStore } from "../state/bulkRename";
import { useSettingsStore } from "../state/settingsStore";
import { Dialog } from "./Dialog";
import { RenameTable, type RenameTableRow } from "./RenameTable";
import { t, useT } from "../i18n";

const FIND_MODE_LABELS: Record<FindMode, string> = {
  get text() {
    return t("rename.text");
  },
  get regex() {
    return t("rename.regex");
  },
};
const FIND_SCOPE_LABELS: Record<FindScope, string> = {
  get name() {
    return t("common.name");
  },
  get extension() {
    return t("rename.extension");
  },
  get both() {
    return t("rename.both");
  },
};
const LETTER_CASE_LABELS: Record<LetterCase, string> = {
  get unchanged() {
    return t("rename.unchanged");
  },
  get lower() {
    return t("rename.lowercase");
  },
  get upper() {
    return t("rename.uppercase");
  },
  get title() {
    return t("rename.titleCase");
  },
  get sentence() {
    return t("rename.sentenceCase");
  },
};
const EXTENSION_LABELS: Record<ExtensionChange, string> = {
  get keep() {
    return t("common.keep");
  },
  get lower() {
    return t("rename.lowercase");
  },
  get upper() {
    return t("rename.uppercase");
  },
  get replace() {
    return t("rename.replaceWith");
  },
  get remove() {
    return t("common.remove");
  },
};
const COUNTER_LIMIT = 999_999_999;
const NO_ENTRIES: readonly FileEntry[] = [];

type CounterPlace = "off" | CounterRule["placement"];
const COUNTER_PLACE_LABELS: Record<CounterPlace, string> = {
  get off() {
    return t("common.no");
  },
  get prefix() {
    return t("rename.beforeTheName");
  },
  get suffix() {
    return t("rename.afterTheName");
  },
};

type PreviewFilter = "all" | "changed" | "problems";
const FILTER_LABELS: Record<PreviewFilter, string> = {
  get all() {
    return t("rename.all");
  },
  get changed() {
    return t("rename.changed");
  },
  get problems() {
    return t("rename.problems");
  },
};

interface BulkRenameDialogProps {
  origin: FileOrigin;
  /** The folder the items are in. */
  folder: string;
  /** The items picked to rename, in the order the list shows them, as the listing had them then. */
  chosen: readonly FileEntry[];
  /** The folder's listing on show now, which a fresh one may replace while the dialog is open. */
  listing: DirListing | null;
  /** Whether that listing is a remembered one the server has yet to confirm, or could not. */
  stale: Staleness;
  /** Asks the server for the folder again, after it could not be asked. */
  onRetry: () => void;
  onClose: () => void;
}

/**
 * Rules on the left, every item's old and new name on the right, and nothing runs until Rename.
 * Names are checked against what the folder lists now: over a remembered listing Rename waits
 * for the server's, and an item gone from it by then is left out.
 */
export function BulkRenameDialog({
  origin,
  folder,
  chosen,
  listing,
  stale,
  onRetry,
  onClose,
}: BulkRenameDialogProps) {
  const t = useT();
  const [rules, setRules] = useState(() => useBulkRenameStore.getState().rules);
  const [filter, setFilter] = useState<PreviewFilter>("all");
  const history = useSettingsStore((state) => state.settings.interface.renameHistory);
  const templateRef = useRef<HTMLInputElement>(null);
  /** Where the cursor goes in the template once a token chip has put its token in. */
  const caretAfterToken = useRef<number | null>(null);

  const listed = listing?.entries ?? NO_ENTRIES;
  // As the folder lists them now, with the modified times {date} and {time} take.
  const entries = useMemo(() => entriesStillListed(chosen, listed), [chosen, listed]);
  const gone = goneNote(chosen.length - entries.length, entries.length);

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

  // Whether case and Unicode form matter in the folder may take asking it, so the preview starts
  // from what its kind says and Rename waits for the answer.
  const [target, setTarget] = useState<NameTarget | null>(null);
  const assumedTarget = useMemo<NameTarget>(() => {
    const traits = knownTraits(origin, folder);
    return {
      ...traits,
      caseInsensitive: traits.caseInsensitive ?? false,
      normalizes: traits.normalizes ?? false,
    };
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
  const preview = useMemo(() => {
    // The problems it finds are written in the interface's language, so they follow it.
    void t;
    return previewRenames(selection, previewedRules, target ?? assumedTarget);
  }, [selection, previewedRules, target, assumedTarget, t]);
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
    stale === null &&
    target !== null &&
    checked.findError === null &&
    checked.problemCount === 0 &&
    checked.ready > 0;
  const apply = () => {
    if (!target || stale !== null) return;
    // Enter may arrive before the preview has caught up with the last key.
    const checked = previewedRules === rules ? preview : previewRenames(selection, rules, target);
    if (!canRename(checked)) return;
    onClose();
    void runBulkRename({ origin, folder, renames: renamesOf(checked), held, target, rules });
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    apply();
  };
  /** Enter applies from anywhere in the dialog, the lists and the preview included, but a button. */
  const applyOnEnter = (event: KeyboardEvent<HTMLFormElement>) => {
    if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
    if (event.target instanceof HTMLButtonElement) return;
    event.preventDefault();
    apply();
  };

  const tableRow = (index: number): RenameTableRow => {
    const row = preview.rows[shownRows ? shownRows[index] : index];
    if (row.status === "problem") {
      return { ...row, tone: "bad", status: row.message ?? t("rename.cannotBeRenamed") };
    }
    return row.status === "unchanged"
      ? { ...row, tone: "dim", status: t("rename.nameUnchanged") }
      : { ...row, tone: "plain", status: t("rename.ready") };
  };

  const templateActive = templateIsActive(rules.template);
  const counterPlace: CounterPlace = rules.counter.enabled ? rules.counter.placement : "off";
  const problems = problemSummary(preview);
  const summary =
    entries.length === 0 && gone
      ? gone
      : [renamedSummary(preview), gone].filter(Boolean).join(t("common.sentenceSeparator"));

  return (
    <Dialog
      title={
        entries.length === 0
          ? t("rename.nothingToRename")
          : t("rename.renameItems", { count: entries.length })
      }
      onClose={onClose}
      width={1120}
      className="bulk-rename-dialog"
    >
      <form className="rename-form" onSubmit={submit} onKeyDown={applyOnEnter}>
        <div className="rename-rules">
          <p className="field-hint">{t("rename.appliedInThisOrderEach")}</p>

          <RuleSection
            step={1}
            title={t("rename.findAndReplace")}
            enabled={rules.find.enabled}
            onToggle={(enabled) => changeFind({ enabled })}
          >
            <div className="rename-find">
              <input
                data-autofocus
                className={`mono ${preview.findError ? "has-error" : ""}`}
                aria-label={t("rename.find")}
                aria-invalid={preview.findError !== null}
                placeholder={
                  rules.find.mode === "regex" ? t("rename.regularExpression") : t("rename.find")
                }
                spellCheck={false}
                value={rules.find.pattern}
                onChange={(event) => changeFind({ pattern: event.target.value })}
              />
              {history.length > 0 && (
                <select
                  aria-label={t("rename.recentPatterns")}
                  value=""
                  onChange={(event) => {
                    const entry = history[Number(event.target.value)];
                    if (entry) changeFind({ ...entry, enabled: true });
                  }}
                >
                  <option value="" disabled>
                    {t("rename.recent")}
                  </option>
                  {history.map((entry, index) => (
                    <option key={index} value={index}>
                      {findHistoryLabel(entry)}
                    </option>
                  ))}
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
              aria-label={t("rename.replaceWith")}
              placeholder={
                rules.find.mode === "regex"
                  ? t("rename.replaceWith1OrName")
                  : t("rename.replaceWith")
              }
              spellCheck={false}
              value={rules.find.replacement}
              onChange={(event) => changeFind({ replacement: event.target.value })}
            />
            <div className="rename-options">
              <Choice
                label={t("rename.patternIs")}
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
                {t("rename.matchCase")}
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={rules.find.allMatches}
                  onChange={(event) => changeFind({ allMatches: event.target.checked })}
                />
                {t("rename.everyMatch")}
              </label>
            </div>
            <div className="rename-options">
              <span className="rename-option-label">{t("rename.lookIn")}</span>
              <Choice
                label={t("rename.lookIn")}
                options={FIND_SCOPES}
                labels={FIND_SCOPE_LABELS}
                value={rules.find.scope}
                onChange={(scope) => changeFind({ scope })}
              />
            </div>
          </RuleSection>

          <RuleSection
            step={2}
            title={t("rename.template")}
            enabled={rules.template.enabled}
            onToggle={(enabled) => changeTemplate({ enabled })}
          >
            <input
              ref={templateRef}
              className="mono"
              aria-label={t("rename.templateForTheNewName")}
              placeholder="{name}-{n}.{ext}"
              spellCheck={false}
              value={rules.template.pattern}
              onChange={(event) => changeTemplate({ pattern: event.target.value })}
            />
            <div className="rename-tokens" role="group" aria-label={t("rename.insertAToken")}>
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
                <span>{t("rename.dateAs")}</span>
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
                <span>{t("rename.timeAs")}</span>
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
            <p className="field-hint">{t("rename.theDateAndTimeAre", { ext: "{ext}" })}</p>
          </RuleSection>

          <RuleSection step={3} title={t("rename.counter")}>
            <div className="form-row">
              <WholeNumberField
                label={t("common.start")}
                value={rules.counter.start}
                min={-COUNTER_LIMIT}
                max={COUNTER_LIMIT}
                onChange={(start) => changeCounter({ start })}
              />
              <WholeNumberField
                label={t("rename.step")}
                value={rules.counter.step}
                min={-COUNTER_LIMIT}
                max={COUNTER_LIMIT}
                onChange={(step) => changeCounter({ step })}
              />
              <WholeNumberField
                label={t("rename.digits")}
                value={rules.counter.digits}
                min={COUNTER_DIGIT_LIMITS.min}
                max={COUNTER_DIGIT_LIMITS.max}
                onChange={(digits) => changeCounter({ digits })}
              />
              <label className="field grow">
                <span>{t("rename.countsIn")}</span>
                <select
                  value={rules.counter.order}
                  onChange={(event) =>
                    changeCounter({ order: event.target.value as CounterRule["order"] })
                  }
                >
                  <option value="list">{t("rename.listOrder")}</option>
                  <option value="name">{t("rename.nameOrder")}</option>
                </select>
              </label>
            </div>
            {templateActive ? (
              <p className="field-hint">{t("rename.theTemplatePlacesTheCounter", { n: "{n}" })}</p>
            ) : (
              <div className="form-row">
                <label className="field grow">
                  <span>{t("rename.addToEachName")}</span>
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
                  <span>{t("rename.separator")}</span>
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
            title={t("rename.case")}
            control={
              <select
                aria-label={t("rename.case")}
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
            title={t("rename.extension")}
            control={
              <select
                aria-label={t("rename.extension")}
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
                aria-label={t("rename.newExtension")}
                placeholder={t("rename.newExtensionSuchAsJpg")}
                spellCheck={false}
                value={rules.extension.replacement}
                onChange={(event) => changeExtension({ replacement: event.target.value })}
              />
            )}
          </RuleSection>

          <RuleSection
            step={6}
            title={t("rename.trimAndTidy")}
            enabled={rules.tidy.enabled}
            onToggle={(enabled) => changeTidy({ enabled })}
          >
            <label className="check">
              <input
                type="checkbox"
                checked={rules.tidy.trim}
                onChange={(event) => changeTidy({ trim: event.target.checked })}
              />
              {t("rename.trimSpacesAtTheEnds")}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={rules.tidy.collapseSpaces}
                onChange={(event) => changeTidy({ collapseSpaces: event.target.checked })}
              />
              {t("rename.makeRepeatedSpacesOne")}
            </label>
            <div className="rename-options">
              <label className="check">
                <input
                  type="checkbox"
                  checked={rules.tidy.replaceSpaces}
                  onChange={(event) => changeTidy({ replaceSpaces: event.target.checked })}
                />
                {t("rename.replaceSpacesWith")}
              </label>
              <input
                className="mono rename-short-field"
                aria-label={t("rename.characterThatReplacesSpaces")}
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
            <div className="toggle-group" role="group" aria-label={t("rename.rowsShown")}>
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
            label={t("rename.oldAndNewNames")}
            count={shownRows ? shownRows.length : preview.rows.length}
            row={tableRow}
            empty={t("rename.noNamesChangeYet")}
          />
        </div>

        <div className="rename-footer">
          {stale === "outdated" && (
            <p className="pane-notice pane-notice-warning" role="status">
              <TriangleAlert size={13} />
              <span>{t("rename.couldNotRefreshThisFolder")}</span>
              <button type="button" className="link-button" onClick={onRetry}>
                {t("common.tryAgain")}
              </button>
            </p>
          )}
          {problems ? (
            <>
              <p className="form-error" role="status">
                {problems}
              </p>
              {gone && <p className="rename-summary">{gone}</p>}
            </>
          ) : (
            <p className="rename-summary" role="status">
              {summary}
            </p>
          )}
          <div className="dialog-actions">
            <button
              type="button"
              className="button push-left"
              disabled={rules === DEFAULT_RULES}
              onClick={() => setRules(DEFAULT_RULES)}
            >
              {t("rename.resetRules")}
            </button>
            <button type="button" className="button" onClick={onClose}>
              {t("common.cancel")}
            </button>
            <button type="submit" className="button button-primary" disabled={!canRename(preview)}>
              {stale === "refreshing"
                ? t("rename.checkingTheFolder")
                : canRename(preview)
                  ? t("rename.renameItems", { count: preview.ready })
                  : t("common.rename")}
            </button>
          </div>
        </div>
      </form>
    </Dialog>
  );
}

/** How many items get a new name, or that none does yet. */
function renamedSummary(preview: RenamePreview): string {
  if (preview.ready === 0) return t("rename.noNamesChangeYet");
  if (preview.unchanged === 0) {
    return t("rename.ofItemsWillBeRenamed", { ready: preview.ready, count: preview.rows.length });
  }
  return t("rename.ofItemsWillBeRenamedSomeUnchanged", {
    ready: preview.ready,
    count: preview.rows.length,
    unchanged: preview.unchanged,
  });
}

/** Says which picked items the folder no longer lists, or null when it lists them all. */
function goneNote(gone: number, left: number): string | null {
  if (gone === 0) return null;
  if (left === 0) return t("rename.theseItemsAreNoLongerInTheFolder");
  return t("rename.selectedItemsLeftOut", { count: gone });
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
