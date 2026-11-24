// The speed limit schedule as far as the settings page and status bar need it. The backend
// applies it (src-tauri/src/transfer/limit_schedule.rs); both are tested against
// src-tauri/tests/fixtures/limit-schedule-cases.json so they agree on what is in force.

import { weekdayName } from "./schedule";
import type {
  Direction,
  JobSnapshot,
  LimitMode,
  LimitRule,
  LimitSchedule,
  LimitStatus,
  RateLimit,
  TransferStats,
} from "./types";

export const MAX_RULES = 50;
/** The most a rule limits to, as the ordinary limits in the settings take. */
export const MAX_LIMIT_KIB = 10 * 1024 * 1024;
const MAX_NAME_CHARS = 60;
const MAX_ID_CHARS = 64;
export const MINUTES_PER_DAY = 24 * 60;
const DAYS_PER_WEEK = 7;
const LIMIT_MODES: LimitMode[] = ["unlimited", "limited", "paused"];

export const UNLIMITED: RateLimit = { mode: "unlimited", kib: 0 };
export const PAUSED: RateLimit = { mode: "paused", kib: 0 };
export const DEFAULT_LIMIT_SCHEDULE: LimitSchedule = { enabled: false, rules: [] };

/** Zero is unlimited, as in the ordinary limits. */
export function limited(kib: number): RateLimit {
  return kib === 0 ? UNLIMITED : { mode: "limited", kib };
}

export interface Limits {
  upload: RateLimit;
  download: RateLimit;
}

/** What applies at one time of the week. */
export interface InForce extends Limits {
  /** The rule that applies, by its place in the list; `null` leaves the ordinary limits. */
  rule: number | null;
}

/** The two limits of the transfer settings, in KiB per second with zero for unlimited. */
export function ordinaryLimits(uploadKib: number, downloadKib: number): Limits {
  return { upload: limited(uploadKib), download: limited(downloadKib) };
}

/** As it applies: only a limit carries a number. */
function asApplied(limit: RateLimit): RateLimit {
  switch (limit.mode) {
    case "limited":
      return limited(limit.kib);
    case "unlimited":
      return UNLIMITED;
    case "paused":
      return PAUSED;
  }
}

function sameLimit(first: RateLimit, second: RateLimit): boolean {
  return first.mode === second.mode && first.kib === second.kib;
}

export function sameInForce(first: InForce, second: InForce): boolean {
  return (
    first.rule === second.rule &&
    sameLimit(first.upload, second.upload) &&
    sameLimit(first.download, second.download)
  );
}

/**
 * Whether the rule covers `minute` of `weekday` (Monday is 0). The part of a rule after midnight
 * belongs to the day it started on.
 */
function covers(rule: LimitRule, weekday: number, minute: number): boolean {
  const startsToday = rule.days.includes(weekday);
  if (rule.startMinute < rule.endMinute) {
    return startsToday && minute >= rule.startMinute && minute < rule.endMinute;
  }
  const startedYesterday = rule.days.includes((weekday + DAYS_PER_WEEK - 1) % DAYS_PER_WEEK);
  return (
    (startsToday && minute >= rule.startMinute) || (startedYesterday && minute < rule.endMinute)
  );
}

/** What applies at `minute` of `weekday`. Off, or with no rule covering it, the ordinary limits. */
export function limitsAt(
  schedule: LimitSchedule,
  ordinary: Limits,
  weekday: number,
  minute: number,
): InForce {
  let rule: number | null = null;
  if (schedule.enabled) {
    for (let index = schedule.rules.length - 1; index >= 0; index--) {
      if (covers(schedule.rules[index], weekday, minute)) {
        rule = index;
        break;
      }
    }
  }
  if (rule === null) return { rule, ...ordinary };
  const { upload, download } = schedule.rules[rule];
  return { rule, upload: asApplied(upload), download: asApplied(download) };
}

/** A stretch of one day with the same limits, from `start` to before `end` in minutes. */
export interface Span {
  start: number;
  end: number;
  inForce: InForce;
}

/** One day of the week as stretches of the same limits, for the week view. */
export function daySpans(schedule: LimitSchedule, ordinary: Limits, weekday: number): Span[] {
  // Within a day, what applies changes only where some rule starts or ends.
  const boundaries = new Set([0]);
  for (const rule of schedule.rules) {
    boundaries.add(rule.startMinute);
    boundaries.add(rule.endMinute);
  }
  const starts = [...boundaries].sort((first, second) => first - second);
  const spans: Span[] = [];
  starts.forEach((start, position) => {
    const end = starts[position + 1] ?? MINUTES_PER_DAY;
    const inForce = limitsAt(schedule, ordinary, weekday, start);
    const previous = spans[spans.length - 1];
    if (previous && sameInForce(previous.inForce, inForce)) {
      previous.end = end;
    } else {
      spans.push({ start, end, inForce });
    }
  });
  return spans;
}

/** The parts of `spans` within one hour, cut to it. */
export function hourSpans(spans: Span[], hour: number): Span[] {
  const from = hour * 60;
  const to = from + 60;
  return spans
    .filter((span) => span.start < to && span.end > from)
    .map((span) => ({ ...span, start: Math.max(span.start, from), end: Math.min(span.end, to) }));
}

export function defaultRule(id: string): LimitRule {
  return {
    id,
    name: "",
    days: [0, 1, 2, 3, 4],
    startMinute: 9 * 60,
    endMinute: 18 * 60,
    upload: limited(500),
    download: limited(500),
  };
}

/** The rules with the one at `index` moved `offset` places, as far as the list allows. */
export function moveRule(rules: LimitRule[], index: number, offset: number): LimitRule[] {
  const target = Math.min(rules.length - 1, Math.max(0, index + offset));
  if (target === index || !rules[index]) return rules;
  const moved = [...rules];
  const [rule] = moved.splice(index, 1);
  moved.splice(target, 0, rule);
  return moved;
}

/** The rule's name, or its place in the list when it has none. */
export function ruleLabel(name: string, index: number): string {
  return name || `Rule ${index + 1}`;
}

/** `500 KiB/s`, `unlimited` or `paused`. */
export function describeLimit(limit: RateLimit): string {
  switch (limit.mode) {
    case "limited":
      return `${limit.kib} KiB/s`;
    case "unlimited":
      return "unlimited";
    case "paused":
      return "paused";
  }
}

/** `uploads 200 KiB/s, downloads paused`, or both at once when they agree. */
export function describeLimits(limits: Limits): string {
  if (sameLimit(asApplied(limits.upload), asApplied(limits.download))) {
    return `uploads and downloads ${describeLimit(limits.upload)}`;
  }
  return `uploads ${describeLimit(limits.upload)}, downloads ${describeLimit(limits.download)}`;
}

/** `09:00`, and `24:00` for the end of a day. */
export function minuteLabel(minute: number): string {
  const hours = Math.floor(minute / 60);
  const minutes = minute % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

/** When a rule runs, in words, for the list of rules. */
export function describeHours(rule: Pick<LimitRule, "startMinute" | "endMinute">): string {
  const start = minuteLabel(rule.startMinute);
  if (rule.startMinute === rule.endMinute) {
    return rule.startMinute === 0 ? "All day" : `24 hours from ${start}`;
  }
  const end = rule.endMinute === 0 ? "midnight" : minuteLabel(rule.endMinute);
  return rule.endMinute < rule.startMinute && rule.endMinute !== 0
    ? `${start} to ${end} the next day`
    : `${start} to ${end}`;
}

/** What one stretch of the week view stands for, for its tooltip. */
export function describeSpan(schedule: LimitSchedule, weekday: number, span: Span): string {
  const when = `${weekdayName(weekday)} ${minuteLabel(span.start)} to ${minuteLabel(span.end)}`;
  const { rule } = span.inForce;
  const source =
    rule === null ? "No rule, the ordinary limits" : ruleLabel(schedule.rules[rule].name, rule);
  return `${when}: ${source}, ${describeLimits(span.inForce)}`;
}

/** A time as a clock shows it, `18:00`, with the weekday when it is not today. */
export function clockTime(epochSeconds: number, now: Date): string {
  const moment = new Date(epochSeconds * 1000);
  const time = minuteLabel(moment.getHours() * 60 + moment.getMinutes());
  return moment.toDateString() === now.toDateString()
    ? time
    : `${weekdayName((moment.getDay() + 6) % 7)} ${time}`;
}

/** Whether the schedule holds transfers in this direction now. */
export function holds(limits: LimitStatus, direction: Direction): boolean {
  switch (direction) {
    case "upload":
      return limits.upload.mode === "paused";
    case "download":
      return limits.download.mode === "paused";
    case "relay":
      return limits.upload.mode === "paused" || limits.download.mode === "paused";
  }
}

/**
 * For a queued job the schedule holds: what it waits for, and until when if that is known.
 * `null` while the whole queue is paused, as nothing continues then until the queue is started;
 * a scheduled run's job is the exception while the queue only waits for Start, since it goes
 * without one.
 */
export function scheduleWait(
  job: Pick<JobSnapshot, "state" | "direction" | "unattended">,
  stats: Pick<TransferStats, "limits" | "queuePaused" | "heldForStart">,
  now: Date,
): string | null {
  const { limits } = stats;
  const stopped = stats.queuePaused && !(stats.heldForStart && job.unattended === true);
  if (stopped || job.state !== "queued" || !holds(limits, job.direction)) return null;
  const end = limits.pauseEnds[job.direction];
  return end === null
    ? "Waiting for the speed limit schedule"
    : `Waiting for the speed limit schedule until ${clockTime(end, now)}`;
}

/** The schedule set the limits in force, rather than the ordinary limits. */
export function scheduleSetsLimits(limits: LimitStatus): boolean {
  return limits.rule !== null && !limits.ignored;
}

/**
 * The limits the status bar names: those a rule slows or pauses, or both directions when it
 * lifts every limit.
 */
export function shownLimits(
  limits: LimitStatus,
): { direction: "upload" | "download"; mode: LimitMode; text: string }[] {
  const directions = (["upload", "download"] as const).map((direction) => ({
    direction,
    limit: limits[direction],
  }));
  const restricted = directions.filter(({ limit }) => limit.mode !== "unlimited");
  return (restricted.length > 0 ? restricted : directions).map(({ direction, limit }) => ({
    direction,
    mode: limit.mode,
    text: describeLimit(limit),
  }));
}

/** The status bar's tooltip for the schedule. */
export function scheduleTooltip(limits: LimitStatus, now: Date): string {
  const until = limits.until === null ? "" : ` until ${clockTime(limits.until, now)}`;
  const rule = limits.rule ? ruleLabel(limits.rule.name, limits.rule.index) : "";
  if (limits.ignored) {
    return (
      `Speed limit schedule ignored${until}: "${rule}" is set aside and the ordinary limits ` +
      `apply, ${describeLimits(limits)}.`
    );
  }
  return `Speed limit schedule: "${rule}" applies${until}, ${describeLimits(limits)}.`;
}

type Stored = Record<string, unknown>;

function asObject(value: unknown): Stored {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Stored) : {};
}

function wholeNumber(value: unknown, max: number): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(max, Math.max(0, Math.floor(value)))
    : 0;
}

function sanitizeLimit(value: unknown): RateLimit {
  const stored = asObject(value);
  const mode = LIMIT_MODES.includes(stored.mode as LimitMode)
    ? (stored.mode as LimitMode)
    : "unlimited";
  const kib = wholeNumber(stored.kib, MAX_LIMIT_KIB);
  return mode === "limited" && kib === 0 ? { mode: "unlimited", kib } : { mode, kib };
}

/** The first line, without control characters, trimmed and cut to `limit` characters. */
function oneLine(value: unknown, limit: number): string {
  const text = typeof value === "string" ? value : "";
  const printable = [...(text.split("\n")[0] ?? "")]
    .filter((character) => {
      const code = character.codePointAt(0) ?? 0;
      return code > 0x1f && (code < 0x7f || code > 0x9f);
    })
    .join("");
  return [...printable.trim()].slice(0, limit).join("").trimEnd();
}

/**
 * Stored rules made safe to use, as the backend does: known days, times within a day, limits in
 * range, one-line names and an id of its own for each.
 */
export function sanitizeLimitSchedule(stored: unknown): LimitSchedule {
  const source = asObject(stored);
  const rules = Array.isArray(source.rules) ? source.rules.slice(0, MAX_RULES) : [];
  const seen = new Set<string>();
  return {
    enabled: typeof source.enabled === "boolean" ? source.enabled : false,
    rules: rules.map((value, index) => {
      const rule = asObject(value);
      const days = Array.isArray(rule.days) ? rule.days : [];
      let id =
        typeof rule.id === "string"
          ? [...rule.id.trim()].slice(0, MAX_ID_CHARS).join("").trimEnd()
          : "";
      if (!id) id = `rule-${index + 1}`;
      const base = [...id];
      for (let number = index + 1; seen.has(id); number += 1) {
        const suffix = `-${number}`;
        id = base.slice(0, MAX_ID_CHARS - suffix.length).join("") + suffix;
      }
      seen.add(id);
      return {
        id,
        name: oneLine(rule.name, MAX_NAME_CHARS),
        days: [
          ...new Set(
            days.filter(
              (day): day is number => Number.isInteger(day) && day >= 0 && day < DAYS_PER_WEEK,
            ),
          ),
        ].sort((first, second) => first - second),
        startMinute: wholeNumber(rule.startMinute, MINUTES_PER_DAY - 1),
        endMinute: wholeNumber(rule.endMinute, MINUTES_PER_DAY - 1),
        upload: sanitizeLimit(rule.upload),
        download: sanitizeLimit(rule.download),
      };
    }),
  };
}
