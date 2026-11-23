// Pure helpers for the scheduled tasks dialog; tasks are kept and run by
// src-tauri/src/automation. Weekdays count from Monday as 0, as in the backend.

import { formatDate, pluralize } from "./format";
import { sanitizeSyncOptions } from "./rsyncOptions";
import type { SyncSettings } from "./settings";
import type {
  Direction,
  ExistsAction,
  ScheduledTask,
  SyncOptions,
  TaskAction,
  TaskItem,
  Trigger,
} from "./types";

export type TransferTaskAction = Extract<TaskAction, { type: "transfer" }>;

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
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

const TRANSFER_VERBS: Record<Direction, string> = {
  upload: "Upload",
  download: "Download",
  relay: "Copy",
};

/** Offered when the task has no name of its own. */
export function suggestedName(action: TaskAction): string {
  if (action.type === "command") {
    const command = action.command.trim();
    return command ? `Run ${command.split(/\s+/)[0]}` : "";
  }
  if (action.type === "transfer") {
    const verb = TRANSFER_VERBS[action.direction];
    if (action.items.length === 0) return "";
    return action.items.length === 1
      ? `${verb} ${folderName(action.items[0].path.trim())}`
      : `${verb} ${pluralize(action.items.length, "item")}`;
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
  direction: SyncSettings["direction"];
  options: SyncSettings["options"];
  arguments: string;
}

export function syncAction({
  connectionId,
  localPath,
  remotePath,
  direction,
  options,
  arguments: args,
}: SyncTaskSource): TaskAction {
  return {
    ...options,
    type: "sync",
    connectionId,
    localPath,
    remotePath,
    direction,
    arguments: args,
  };
}

/** The rsync options of a synchronization task, without what else it holds. */
export function taskOptions(action: Extract<TaskAction, { type: "sync" }>): SyncOptions {
  return sanitizeSyncOptions(action);
}

/** What a transfer task may do with a file already there; it cannot ask. */
export const TASK_EXISTS_ACTIONS: { value: ExistsAction; label: string }[] = [
  { value: "overwrite", label: "Overwrite" },
  { value: "overwriteIfNewer", label: "Overwrite if the source is newer" },
  { value: "overwriteIfDifferent", label: "Overwrite if size or date differ" },
  { value: "resume", label: "Resume" },
  { value: "rename", label: "Keep both" },
  { value: "skip", label: "Skip" },
];

/** A transfer task with nothing in it yet. */
export function transferAction(
  connectionId: string,
  direction: Direction = "upload",
  change: Partial<TransferTaskAction> = {},
): TransferTaskAction {
  return {
    type: "transfer",
    connectionId,
    sourceConnectionId: null,
    direction,
    items: [],
    destination: "",
    exists: "overwrite",
    subfolder: "",
    ...change,
  };
}

/** Where the items are read: this computer for an upload, a server otherwise. */
function itemsAreLocal(direction: Direction): boolean {
  return direction === "upload";
}

/** Where the destination is: this computer for a download, a server otherwise. */
function destinationIsLocal(direction: Direction): boolean {
  return direction === "download";
}

/** The action after its direction changes, keeping what still means the same. */
export function withDirection(
  action: TransferTaskAction,
  direction: Direction,
): TransferTaskAction {
  if (action.direction === direction) return action;
  const keepItems = itemsAreLocal(action.direction) === itemsAreLocal(direction);
  const keepDestination = destinationIsLocal(action.direction) === destinationIsLocal(direction);
  let { connectionId, sourceConnectionId } = action;
  // The server the items are on becomes the one copied from, and back.
  if (direction === "relay" && action.direction === "download") {
    sourceConnectionId = connectionId;
    connectionId = "";
  } else if (action.direction === "relay" && direction === "download") {
    connectionId = sourceConnectionId ?? "";
  }
  return {
    ...action,
    direction,
    connectionId,
    sourceConnectionId: direction === "relay" ? sourceConnectionId : null,
    items: keepItems ? action.items : [],
    destination: keepDestination ? action.destination : "",
  };
}

/** The items with `added` at the end, each path once. */
export function withTaskItems(items: TaskItem[], added: TaskItem[]): TaskItem[] {
  const next = [...items];
  for (const item of added) {
    const path = item.path.trim();
    if (path && !next.some((existing) => existing.path === path)) next.push({ ...item, path });
  }
  return next;
}

/** What a folder pattern may hold between braces. Mirrored in `automation/schedule.rs`. */
export const PATTERN_PLACEHOLDERS = [
  "date",
  "time",
  "year",
  "month",
  "day",
  "hour",
  "minute",
  "weekday",
  "week",
] as const;
const MAX_PATTERN_CHARS = 200;
/** End a path part, or are refused by Windows and most cloud drives. */
const FORBIDDEN_IN_FOLDER_NAMES = /[\\:*?"<>|]/;

/** Control characters, as Rust's `char::is_control` has them. */
function hasControlCharacters(text: string): boolean {
  return [...text].some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code < 0x20 || (code >= 0x7f && code <= 0x9f);
  });
}

/** The ISO 8601 week of a local date: weeks start on Monday, week 1 holds January 4. */
export function isoWeek(date: Date): number {
  const thursday = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  thursday.setDate(thursday.getDate() - weekdayOf(thursday) + 3);
  const firstThursday = new Date(thursday.getFullYear(), 0, 4);
  firstThursday.setDate(firstThursday.getDate() - weekdayOf(firstThursday) + 3);
  const days = Math.round((thursday.getTime() - firstThursday.getTime()) / 86_400_000);
  return 1 + days / 7;
}

function placeholderValue(name: string, now: Date): string | null {
  switch (name) {
    case "date":
      return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
    case "time":
      return `${pad(now.getHours())}${pad(now.getMinutes())}`;
    case "year":
      return String(now.getFullYear()).padStart(4, "0");
    case "month":
      return pad(now.getMonth() + 1);
    case "day":
      return pad(now.getDate());
    case "hour":
      return pad(now.getHours());
    case "minute":
      return pad(now.getMinutes());
    case "weekday":
      return WEEKDAY_NAMES[weekdayOf(now)];
    case "week":
      return pad(isoWeek(now));
    default:
      return null;
  }
}

function plainFolderName(part: string): boolean {
  return (
    part !== "" &&
    part !== "." &&
    part !== ".." &&
    part.trim() === part &&
    !part.endsWith(".") &&
    !FORBIDDEN_IN_FOLDER_NAMES.test(part) &&
    !hasControlCharacters(part)
  );
}

/**
 * The folder a pattern such as `backup-{date}` names for a run at `now`, or what is wrong with
 * it. Mirrors `expand_folder_pattern` in `automation/schedule.rs`, which has the last word.
 */
export function expandFolderPattern(
  pattern: string,
  now: Date,
): { folder: string } | { error: string } {
  const trimmed = pattern.trim().replace(/^\/+|\/+$/g, "");
  if ([...trimmed].length > MAX_PATTERN_CHARS) {
    return { error: `The folder pattern is longer than ${MAX_PATTERN_CHARS} characters` };
  }
  let expanded = "";
  let rest = trimmed;
  for (let open = rest.search(/[{}]/); open >= 0; open = rest.search(/[{}]/)) {
    expanded += rest.slice(0, open);
    const after = rest.slice(open + 1);
    const close = after.search(/[{}]/);
    if (rest[open] !== "{" || close < 0 || after[close] !== "}") {
      return {
        error: "The folder pattern has a brace without its partner; write placeholders as {date}",
      };
    }
    const name = after.slice(0, close);
    const value = placeholderValue(name, now);
    if (value === null) {
      const known = PATTERN_PLACEHOLDERS.map((placeholder) => `{${placeholder}}`).join(", ");
      return {
        error: `The folder pattern has {${name}}, which is not a placeholder; use ${known}`,
      };
    }
    expanded += value;
    rest = after.slice(close + 1);
  }
  expanded += rest;
  const bad =
    expanded === "" ? undefined : expanded.split("/").find((part) => !plainFolderName(part));
  if (bad !== undefined) {
    return { error: `The folder pattern names "${bad}", which cannot be a folder name` };
  }
  return { folder: expanded };
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
