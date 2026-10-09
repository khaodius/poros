// Pure helpers for the scheduled tasks dialog; tasks are kept and run by
// src-tauri/src/automation. Weekdays count from Monday as 0, as in the backend.

import { formatDate, pluralize } from "./format";
import type { SyncSettings } from "./settings";
import { excludePatterns } from "./sync";
import type { ScheduledTask, TaskAction, Trigger } from "./types";

export const WEEKDAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];
const WORKDAYS = [0, 1, 2, 3, 4];
const WEEKEND = [5, 6];

const MINUTES_PER_HOUR = 60;
const MINUTES_PER_DAY = 24 * MINUTES_PER_HOUR;
/** Longest repeat interval the backend accepts: four weeks. */
export const MAX_INTERVAL_MINUTES = 28 * MINUTES_PER_DAY;

export type IntervalUnit = "minutes" | "hours" | "days";
export const INTERVAL_UNITS: { value: IntervalUnit; label: string; minutes: number }[] = [
  { value: "minutes", label: "minutes", minutes: 1 },
  { value: "hours", label: "hours", minutes: MINUTES_PER_HOUR },
  { value: "days", label: "days", minutes: MINUTES_PER_DAY },
];

/** The largest unit that divides the interval evenly, for showing it in the editor. */
export function splitInterval(minutes: number): { amount: number; unit: IntervalUnit } {
  for (const unit of [...INTERVAL_UNITS].reverse()) {
    if (minutes >= unit.minutes && minutes % unit.minutes === 0) {
      return { amount: minutes / unit.minutes, unit: unit.value };
    }
  }
  return { amount: Math.max(1, minutes), unit: "minutes" };
}

export function intervalMinutes(amount: number, unit: IntervalUnit): number {
  const perUnit = INTERVAL_UNITS.find((option) => option.value === unit)?.minutes ?? 1;
  return amount * perUnit;
}

/** Monday is 0. */
export function weekdayOf(date: Date): number {
  return (date.getDay() + 6) % 7;
}

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

/** `HH:MM`, as a time input shows it. */
export function timeInputValue(minuteOfDay: number): string {
  return `${pad(Math.floor(minuteOfDay / MINUTES_PER_HOUR))}:${pad(minuteOfDay % MINUTES_PER_HOUR)}`;
}

export function minuteOfDayFrom(value: string): number | null {
  const match = /^(\d{2}):(\d{2})/.exec(value);
  if (!match) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  return hours < 24 && minutes < 60 ? hours * MINUTES_PER_HOUR + minutes : null;
}

/** `YYYY-MM-DDTHH:MM` in local time, as a date and time input shows it. */
export function dateTimeInputValue(epochSeconds: number): string {
  const date = new Date(epochSeconds * 1000);
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** Seconds since the Unix epoch for a local `YYYY-MM-DDTHH:MM`. */
export function epochFromDateTimeInput(value: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(value);
  if (!match) return null;
  const [year, month, day, hours, minutes] = match.slice(1).map(Number);
  const date = new Date(year, month - 1, day, hours, minutes);
  return Number.isNaN(date.getTime()) ? null : Math.floor(date.getTime() / 1000);
}

function sameDays(weekdays: number[], expected: number[]): boolean {
  return weekdays.length === expected.length && expected.every((day) => weekdays.includes(day));
}

function describeDays(weekdays: number[]): string {
  if (sameDays(weekdays, EVERY_DAY)) return "Every day";
  if (sameDays(weekdays, WORKDAYS)) return "Weekdays";
  if (sameDays(weekdays, WEEKEND)) return "Weekends";
  return [...weekdays]
    .sort((first, second) => first - second)
    .map((day) => WEEKDAY_NAMES[day])
    .join(", ");
}

export function describeTrigger(trigger: Trigger): string {
  switch (trigger.type) {
    case "once":
      return `Once, ${formatDate(trigger.at)}`;
    case "every": {
      const { amount, unit } = splitInterval(trigger.minutes);
      const singular = unit.slice(0, -1);
      return amount === 1 ? `Every ${singular}` : `Every ${pluralize(amount, singular)}`;
    }
    case "daily":
      return `${describeDays(trigger.weekdays)} at ${timeInputValue(trigger.minuteOfDay)}`;
  }
}

/** The last part of a local or server path. */
function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Offered when the task has no name of its own. */
export function suggestedName(action: TaskAction): string {
  if (action.type === "command") {
    const command = action.command.trim();
    return command ? `Run ${command.split(/\s+/)[0]}` : "";
  }
  const folder = folderName(action.direction === "download" ? action.remotePath : action.localPath);
  if (!folder.trim()) return "";
  const verb =
    action.direction === "upload"
      ? "Upload"
      : action.direction === "download"
        ? "Download"
        : "Sync";
  return `${verb} ${folder}`;
}

export interface SyncTaskSource {
  connectionId: string;
  localPath: string;
  remotePath: string;
  options: SyncSettings;
}

export function syncAction({
  connectionId,
  localPath,
  remotePath,
  options,
}: SyncTaskSource): TaskAction {
  return {
    type: "sync",
    connectionId,
    localPath,
    remotePath,
    direction: options.direction,
    compare: options.compare,
    deleteExtraneous: options.direction !== "both" && options.deleteExtraneous,
    skipNewerOnTarget: options.direction !== "both" && options.skipNewerOnTarget,
    ignoreExisting: options.ignoreExisting,
    timeToleranceSecs: options.timeToleranceSecs,
    excludes: excludePatterns(options.excludes),
  };
}

/** A new task: every night at two, run again at the next start if Poros was closed. */
export function newTask(action: TaskAction): ScheduledTask {
  return {
    id: "",
    name: "",
    enabled: true,
    trigger: { type: "daily", minuteOfDay: 2 * MINUTES_PER_HOUR, weekdays: EVERY_DAY },
    action,
    runMissed: true,
  };
}

/** The trigger as another kind, keeping its time where it can, for switching between them. */
export function convertTrigger(trigger: Trigger, type: Trigger["type"], now: Date): Trigger {
  if (trigger.type === type) return trigger;
  const nowSeconds = Math.floor(now.getTime() / 1000);
  const next = new Date(now);
  if (trigger.type === "daily") {
    next.setHours(0, trigger.minuteOfDay, 0, 0);
    if (next <= now) next.setDate(next.getDate() + 1);
  } else {
    next.setHours(now.getHours() + 1, 0, 0, 0);
  }
  const given = trigger.type === "once" ? trigger.at : trigger.type === "every" ? trigger.start : 0;
  const moment = given > nowSeconds ? given : Math.floor(next.getTime() / 1000);
  switch (type) {
    case "once":
      return { type, at: moment };
    case "every":
      return {
        type,
        minutes: trigger.type === "daily" ? MINUTES_PER_DAY : MINUTES_PER_HOUR,
        start: moment,
      };
    case "daily": {
      const date = new Date(moment * 1000);
      return {
        type,
        minuteOfDay: date.getHours() * MINUTES_PER_HOUR + date.getMinutes(),
        weekdays: EVERY_DAY,
      };
    }
  }
}

/** The task without what the backend works out, for saving or comparing. */
export function editableTask(task: ScheduledTask): ScheduledTask {
  const { id, name, enabled, trigger, action, runMissed } = task;
  return { id, name, enabled, trigger, action, runMissed };
}

/** Objects with their keys in order, so equal values print the same. */
function canonical(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonical);
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, field]) => field !== undefined)
      .sort(([first], [second]) => first.localeCompare(second))
      .map(([key, field]) => [key, canonical(field)]),
  );
}

/** Whether two tasks would be saved the same. */
export function sameTask(first: ScheduledTask, second: ScheduledTask): boolean {
  return (
    JSON.stringify(canonical(editableTask(first))) ===
    JSON.stringify(canonical(editableTask(second)))
  );
}
