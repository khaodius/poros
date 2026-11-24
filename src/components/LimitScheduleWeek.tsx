import { useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { daySpans, describeSpan, hourSpans, type Limits, type Span } from "../lib/limitSchedule";
import { WEEKDAYS, weekdayName } from "../lib/schedule";
import type { LimitMode, LimitSchedule } from "../lib/types";
import { useT, t } from "../i18n";

const HOURS_PER_DAY = 24;
const HOURS = Array.from({ length: HOURS_PER_DAY }, (_, hour) => hour);
const LABELLED_HOURS = [0, 3, 6, 9, 12, 15, 18, 21];
const LEGEND: { mode: LimitMode; label: string }[] = [
  {
    mode: "unlimited",
    get label() {
      return t("limits.unlimited");
    },
  },
  {
    mode: "limited",
    get label() {
      return t("limits.limited");
    },
  },
  {
    mode: "paused",
    get label() {
      return t("limits.paused");
    },
  },
];

interface Cell {
  day: number;
  hour: number;
}

interface LimitScheduleWeekProps {
  schedule: LimitSchedule;
  ordinary: Limits;
  /** The current time, in milliseconds. */
  now: number;
  /** Shows the rule that sets a cell's limits. */
  onPickRule: (index: number) => void;
}

/**
 * The week as seven rows of twenty-four hours, each hour split into an upload lane above and a
 * download lane below, filled by the limit in force. Arrow keys move between hours and days.
 */
export function LimitScheduleWeek({ schedule, ordinary, now, onPickRule }: LimitScheduleWeekProps) {
  const t = useT();
  // Switched off, it shows what the rules would do, so they can be set up first.
  const week = useMemo(
    () => WEEKDAYS.map((day) => daySpans({ ...schedule, enabled: true }, ordinary, day)),
    [schedule, ordinary],
  );
  const moment = new Date(now);
  const today = (moment.getDay() + 6) % 7;
  const minuteNow = moment.getHours() * 60 + moment.getMinutes();
  const [active, setActive] = useState<Cell>({ day: today, hour: Math.floor(minuteNow / 60) });
  const [pointed, setPointed] = useState<Cell | null>(null);
  const [focused, setFocused] = useState(false);
  const cells = useRef<(HTMLDivElement | null)[]>([]);

  /** The stretches of the day that reach into the hour, whole, as the tooltip names them. */
  const stretchesOf = ({ day, hour }: Cell): Span[] =>
    week[day].filter((span) => span.start < (hour + 1) * 60 && span.end > hour * 60);
  const describeCell = (cell: Cell, separator: string) =>
    stretchesOf(cell)
      .map((span) => describeSpan(schedule, cell.day, span))
      .join(separator);
  const ruleOf = (cell: Cell) =>
    stretchesOf(cell).find((span) => span.inForce.rule !== null)?.inForce.rule ?? null;

  const shown = pointed ?? (focused ? active : null);
  const current = week[today].find((span) => span.start <= minuteNow && span.end > minuteNow);
  const detail = shown
    ? describeCell(shown, t("common.clauseSeparator"))
    : current
      ? t("limits.nowSpan", { span: describeSpan(schedule, today, current) })
      : "";

  const moveTo = (cell: Cell) => {
    setActive(cell);
    cells.current[cell.day * HOURS_PER_DAY + cell.hour]?.focus();
  };

  const handleKey = (event: KeyboardEvent) => {
    const { day, hour } = active;
    const last = WEEKDAYS.length - 1;
    const targets: Record<string, Cell> = {
      ArrowLeft: { day, hour: Math.max(0, hour - 1) },
      ArrowRight: { day, hour: Math.min(HOURS_PER_DAY - 1, hour + 1) },
      ArrowUp: { day: Math.max(0, day - 1), hour },
      ArrowDown: { day: Math.min(last, day + 1), hour },
      Home: event.ctrlKey ? { day: 0, hour: 0 } : { day, hour: 0 },
      End: event.ctrlKey
        ? { day: last, hour: HOURS_PER_DAY - 1 }
        : { day, hour: HOURS_PER_DAY - 1 },
    };
    if (event.key in targets) {
      event.preventDefault();
      moveTo(targets[event.key]);
    } else if (event.key === "Enter" || event.key === " ") {
      const rule = ruleOf(active);
      if (rule !== null) {
        event.preventDefault();
        onPickRule(rule);
      }
    }
  };

  const handleClick = (event: MouseEvent, cell: Cell) => {
    setActive(cell);
    // The stretch under the pointer, as an hour can hold the end of one rule and another.
    const picked = (event.target as HTMLElement).closest<HTMLElement>("[data-rule]")?.dataset.rule;
    const rule = picked === undefined ? ruleOf(cell) : Number(picked);
    if (rule !== null) onPickRule(rule);
  };

  return (
    <div className={`limit-week ${schedule.enabled ? "" : "is-preview"}`}>
      <div className="limit-week-hours" aria-hidden>
        {LABELLED_HOURS.map((hour) => (
          <span
            key={hour}
            className={hour % 6 === 0 ? "" : "is-minor"}
            style={{ gridColumn: `${hour + 2} / span 3` }}
          >
            {String(hour).padStart(2, "0")}
          </span>
        ))}
      </div>
      <div
        className="limit-week-grid"
        role="grid"
        aria-label={t("limits.speedLimitsThroughTheWeek")}
        aria-describedby="limit-week-legend"
        onKeyDown={handleKey}
        onFocus={() => setFocused(true)}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget)) setFocused(false);
        }}
        onPointerLeave={() => setPointed(null)}
      >
        {week.map((spans, day) => (
          <div key={day} role="row" className={`limit-week-day ${day === today ? "is-today" : ""}`}>
            <span role="rowheader" className="limit-week-day-name">
              {weekdayName(day)}
            </span>
            {HOURS.map((hour) => {
              const cell = { day, hour };
              const parts = hourSpans(spans, hour);
              const isActive = active.day === day && active.hour === hour;
              const description = describeCell(cell, "\n");
              return (
                <div
                  key={hour}
                  ref={(element) => {
                    cells.current[day * HOURS_PER_DAY + hour] = element;
                  }}
                  role="gridcell"
                  tabIndex={isActive ? 0 : -1}
                  aria-label={description}
                  title={description}
                  className="limit-week-cell"
                  onPointerEnter={() => setPointed(cell)}
                  onClick={(event) => handleClick(event, cell)}
                >
                  {(["upload", "download"] as const).map((direction) => (
                    <span key={direction} className="limit-week-lane">
                      {parts.map((part) => (
                        <span
                          key={part.start}
                          className={`limit-week-segment is-${part.inForce[direction].mode}`}
                          style={{ flexGrow: part.end - part.start }}
                          data-rule={part.inForce.rule ?? undefined}
                        />
                      ))}
                    </span>
                  ))}
                  {day === today && hour === Math.floor(minuteNow / 60) && (
                    <span
                      className="limit-week-now"
                      style={{ left: `${((minuteNow % 60) / 60) * 100}%` }}
                      aria-hidden
                    />
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
      <p className="limit-week-detail" aria-live="polite">
        {detail}
      </p>
      <div className="limit-week-legend" id="limit-week-legend">
        {LEGEND.map(({ mode, label }) => (
          <span key={mode} className="limit-week-key">
            <span className={`limit-week-swatch limit-week-segment is-${mode}`} aria-hidden />
            {label}
          </span>
        ))}
        <span className="limit-week-key">{t("limits.eachHourUploadsAboveDownloads")}</span>
        <span className="limit-week-key">
          <span className="limit-week-swatch-now" aria-hidden />
          {t("limits.now")}
        </span>
      </div>
    </div>
  );
}
