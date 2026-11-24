import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Plus,
  Trash2,
} from "lucide-react";
import { useMinute } from "../hooks/useMinute";
import {
  defaultRule,
  describeHours,
  describeLimit,
  MAX_LIMIT_KIB,
  MAX_RULES,
  moveRule,
  ordinaryLimits,
  ruleLabel,
} from "../lib/limitSchedule";
import {
  describeDays,
  minuteOfDayFrom,
  timeInputValue,
  WEEKDAYS,
  weekdayName,
} from "../lib/schedule";
import type { TransferSettings } from "../lib/settings";
import type { LimitMode, LimitRule, LimitSchedule, RateLimit } from "../lib/types";
import { LIMIT_SCHEDULE_GROUP_ID, takeScheduleReveal } from "../state/scheduleSettingsReveal";
import { saveSettingsSection } from "../state/settingsStore";
import { LimitScheduleWeek } from "./LimitScheduleWeek";
import { CommitInput, SettingGroup, SwitchSetting } from "./settingsFields";
import { Stepper } from "./Stepper";
import { useT, t } from "../i18n";

const MODE_OPTIONS: { value: LimitMode; label: string }[] = [
  {
    value: "limited",
    get label() {
      return t("limits.limitTo");
    },
  },
  {
    value: "unlimited",
    get label() {
      return t("limits.unlimited");
    },
  },
  {
    value: "paused",
    get label() {
      return t("common.pause");
    },
  },
];
/** What a limit starts at when switched to a number it never had. */
const NEW_LIMIT_KIB = 500;

type RulePart = "summary" | "name" | "up" | "down";
type FocusTarget = { ruleId: string; part: RulePart } | "add";

const PART_SELECTORS: Record<RulePart, string> = {
  summary: ".limit-rule-summary",
  name: ".limit-rule-name",
  up: '[data-part="up"]',
  down: '[data-part="down"]',
};

function scheduleGroup(): HTMLElement | null {
  return document.getElementById(LIMIT_SCHEDULE_GROUP_ID);
}

function ruleElement(ruleId: string): HTMLElement | null {
  return (
    scheduleGroup()?.querySelector<HTMLElement>(`[data-rule-id="${CSS.escape(ruleId)}"]`) ?? null
  );
}

/** The speed limit schedule in Settings > Transfers: a switch, the rules and the week they make. */
export function LimitScheduleSettings({ transfers }: { transfers: TransferSettings }) {
  const t = useT();
  const schedule = transfers.limitSchedule;
  const { rules } = schedule;
  const now = useMinute();
  const ordinary = useMemo(
    () => ordinaryLimits(transfers.uploadLimitKib, transfers.downloadLimitKib),
    [transfers.uploadLimitKib, transfers.downloadLimitKib],
  );
  const pendingFocus = useRef<FocusTarget | null>(null);
  // The rules whose editor is open; the others show as one summary each.
  const [openRules, setOpenRules] = useState<ReadonlySet<string>>(new Set());
  const setOpen = (id: string, open: boolean) =>
    setOpenRules((current) => {
      const next = new Set(current);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });

  // The settings dialog shows itself after its contents are in place, so the block is brought
  // into view a frame later, once there is something to scroll.
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      if (!takeScheduleReveal()) return;
      const group = scheduleGroup();
      group?.scrollIntoView({ block: "start" });
      group?.querySelector<HTMLElement>('[role="switch"]')?.focus({ preventScroll: true });
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  // Moving, opening or adding a rule redraws the list; the focus follows the rule it was on.
  useLayoutEffect(() => {
    const target = pendingFocus.current;
    if (target === null) return;
    pendingFocus.current = null;
    if (target === "add") {
      scheduleGroup()?.querySelector<HTMLElement>(".limit-rule-add")?.focus();
      return;
    }
    const item = ruleElement(target.ruleId);
    const wanted = item?.querySelector<HTMLButtonElement>(PART_SELECTORS[target.part]);
    const other = target.part === "up" ? "down" : "up";
    const reachable =
      wanted && !wanted.disabled ? wanted : item?.querySelector<HTMLElement>(PART_SELECTORS[other]);
    reachable?.focus();
  });

  const save = (next: LimitSchedule) =>
    void saveSettingsSection("transfers", { limitSchedule: next });
  const saveRules = (next: LimitRule[]) => save({ ...schedule, rules: next });
  const update = (id: string, change: Partial<LimitRule>) =>
    saveRules(rules.map((rule) => (rule.id === id ? { ...rule, ...change } : rule)));

  const add = () => {
    const rule = defaultRule(crypto.randomUUID());
    pendingFocus.current = { ruleId: rule.id, part: "name" };
    setOpen(rule.id, true);
    // A first rule is meant to apply, so it switches the schedule on.
    save({ enabled: schedule.enabled || rules.length === 0, rules: [...rules, rule] });
  };

  const pickRule = (index: number) => {
    const rule = rules[index];
    if (!rule) return;
    pendingFocus.current = { ruleId: rule.id, part: "name" };
    setOpen(rule.id, true);
  };

  return (
    <SettingGroup title={t("limits.speedLimitSchedule")} id={LIMIT_SCHEDULE_GROUP_ID}>
      <SwitchSetting
        label={t("limits.followTheSchedule")}
        hint={t("limits.followTheScheduleHint")}
        checked={schedule.enabled}
        onChange={(enabled) => save({ ...schedule, enabled })}
      />
      <p className="setting-note">{t("limits.whereRulesOverlapTheOne")}</p>
      {rules.length > 0 && (
        <ol className="limit-rules">
          {rules.map((rule, index) => (
            <RuleEditor
              key={rule.id}
              rule={rule}
              index={index}
              count={rules.length}
              open={openRules.has(rule.id)}
              onToggle={() => setOpen(rule.id, !openRules.has(rule.id))}
              onChange={(change) => update(rule.id, change)}
              onMove={(offset, part) => {
                pendingFocus.current = { ruleId: rule.id, part };
                saveRules(moveRule(rules, index, offset));
              }}
              onDelete={() => {
                pendingFocus.current = "add";
                saveRules(rules.filter((existing) => existing.id !== rule.id));
              }}
            />
          ))}
        </ol>
      )}
      <button
        type="button"
        className="button button-small limit-rule-add"
        disabled={rules.length >= MAX_RULES}
        onClick={add}
      >
        <Plus size={13} />
        {t("limits.addRule")}
      </button>
      <LimitScheduleWeek schedule={schedule} ordinary={ordinary} now={now} onPickRule={pickRule} />
      {!schedule.enabled && rules.length > 0 && (
        <p className="setting-note">{t("limits.theScheduleIsOffThe")}</p>
      )}
    </SettingGroup>
  );
}

interface RuleEditorProps {
  rule: LimitRule;
  index: number;
  count: number;
  open: boolean;
  onToggle: () => void;
  onChange: (change: Partial<LimitRule>) => void;
  onMove: (offset: number, part: RulePart) => void;
  onDelete: () => void;
}

function RuleEditor({
  rule,
  index,
  count,
  open,
  onToggle,
  onChange,
  onMove,
  onDelete,
}: RuleEditorProps) {
  const t = useT();
  const label = ruleLabel(rule.name, index);
  const editorId = `limit-rule-editor-${index}`;
  const toggleDay = (day: number) =>
    onChange({
      days: rule.days.includes(day)
        ? rule.days.filter((existing) => existing !== day)
        : [...rule.days, day].sort((first, second) => first - second),
    });
  const setTime = (field: "startMinute" | "endMinute", value: string) => {
    const minute = minuteOfDayFrom(value);
    if (minute !== null) onChange({ [field]: minute });
  };

  return (
    <li className={`limit-rule ${open ? "is-open" : ""}`} data-rule-id={rule.id} aria-label={label}>
      <span className="limit-rule-order" aria-hidden>
        {index + 1}
      </span>
      <div className="limit-rule-body">
        <div className="limit-rule-head">
          <button
            type="button"
            className="limit-rule-summary"
            aria-expanded={open}
            aria-controls={open ? editorId : undefined}
            title={open ? t("limits.closeTheRule") : t("limits.editTheRule")}
            onClick={onToggle}
          >
            <span className="limit-rule-title">
              <ChevronRight size={14} className="limit-rule-chevron" aria-hidden />
              <span className={rule.name ? "" : "is-unnamed"}>{label}</span>
            </span>
            <span className="limit-rule-facts">
              <span className={rule.days.length === 0 ? "is-never" : ""}>
                {rule.days.length === 0 ? t("limits.noDaysSoItNever") : describeDays(rule.days)}
              </span>
              <span>{describeHours(rule)}</span>
              <LimitFact direction="upload" limit={rule.upload} />
              <LimitFact direction="download" limit={rule.download} />
            </span>
          </button>
          <span className="limit-rule-actions">
            <button
              type="button"
              className="icon-button"
              data-part="up"
              title={t("limits.moveUp")}
              aria-label={t("limits.moveUp2", { label })}
              disabled={index === 0}
              onClick={() => onMove(-1, "up")}
            >
              <ChevronUp size={15} />
            </button>
            <button
              type="button"
              className="icon-button"
              data-part="down"
              title={t("limits.moveDownSoItWins")}
              aria-label={t("limits.moveDown", { label })}
              disabled={index === count - 1}
              onClick={() => onMove(1, "down")}
            >
              <ChevronDown size={15} />
            </button>
            <button
              type="button"
              className="icon-button"
              title={t("limits.deleteRule")}
              aria-label={t("limits.delete", { label })}
              onClick={onDelete}
            >
              <Trash2 size={14} />
            </button>
          </span>
        </div>
        {open && (
          <div className="limit-rule-editor" id={editorId}>
            <div className="limit-rule-line">
              <CommitInput
                className="limit-rule-name"
                label={t("limits.nameOf", { label })}
                value={rule.name}
                placeholder={t("limits.nameSuchAsOfficeHours")}
                onCommit={(name) => onChange({ name })}
              />
            </div>
            <div className="limit-rule-line">
              <div className="toggle-group" role="group" aria-label={t("limits.daysOfTheWeek")}>
                {WEEKDAYS.map((day) => {
                  const name = weekdayName(day);
                  const chosen = rule.days.includes(day);
                  return (
                    <button
                      key={name}
                      type="button"
                      className={`toggle ${chosen ? "is-on" : ""}`}
                      aria-pressed={chosen}
                      onClick={() => toggleDay(day)}
                    >
                      {name}
                    </button>
                  );
                })}
              </div>
              <span className="limit-rule-hours">
                <input
                  type="time"
                  aria-label={t("limits.startTime")}
                  value={timeInputValue(rule.startMinute)}
                  onChange={(event) => setTime("startMinute", event.target.value)}
                />
                <span>{t("limits.to")}</span>
                <input
                  type="time"
                  aria-label={t("limits.endTime")}
                  value={timeInputValue(rule.endMinute)}
                  onChange={(event) => setTime("endMinute", event.target.value)}
                />
              </span>
            </div>
            <div className="limit-rule-line">
              <LimitEditor
                direction="Upload"
                limit={rule.upload}
                onChange={(upload) => onChange({ upload })}
              />
              <LimitEditor
                direction="Download"
                limit={rule.download}
                onChange={(download) => onChange({ download })}
              />
            </div>
          </div>
        )}
      </div>
    </li>
  );
}

/** One limit of a rule in its summary, with the arrow the status bar uses. */
function LimitFact({ direction, limit }: { direction: "upload" | "download"; limit: RateLimit }) {
  const t = useT();
  const Icon = direction === "upload" ? ArrowUp : ArrowDown;
  return (
    <span className={`limit-rule-fact is-${limit.mode}`}>
      <Icon
        size={12}
        aria-label={direction === "upload" ? t("common.upload") : t("common.download")}
      />
      {describeLimit(limit)}
    </span>
  );
}

interface LimitEditorProps {
  direction: "Upload" | "Download";
  limit: RateLimit;
  onChange: (limit: RateLimit) => void;
}

function LimitEditor({ direction, limit, onChange }: LimitEditorProps) {
  const Icon = direction === "Upload" ? ArrowUp : ArrowDown;
  const label = `${direction} limit`;
  return (
    <span className="limit-rule-limit">
      <Icon size={13} className="limit-rule-direction" aria-hidden />
      <span className="limit-rule-limit-label">{direction}</span>
      <select
        aria-label={label}
        value={limit.mode}
        onChange={(event) => {
          const mode = event.target.value as LimitMode;
          onChange({
            mode,
            kib: mode === "limited" && limit.kib === 0 ? NEW_LIMIT_KIB : limit.kib,
          });
        }}
      >
        {MODE_OPTIONS.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      {limit.mode === "limited" && (
        <Stepper
          value={limit.kib}
          min={1}
          max={MAX_LIMIT_KIB}
          label={label}
          unit="KiB/s"
          onChange={(kib) => onChange({ mode: "limited", kib })}
        />
      )}
    </span>
  );
}
