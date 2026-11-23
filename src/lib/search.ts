// What a pane's search looks for, how its filters read, and how its results relate to the folder
// it started in.

import type { PathStyle } from "./fileSource";
import { isWithin, normalizedPath, samePath } from "./path";
import { isDirLike } from "./sort";
import type {
  FileEntry,
  FileLocation,
  SearchKind,
  SearchPatternKind,
  SearchRecheck,
  SearchReport,
  SearchRequest,
} from "./types";

/** As many results as a search brings before it stops; the backend allows more. */
export const SEARCH_LIMIT = 10_000;
/** Searches remembered for each connection. */
export const RECENT_SEARCHES = 10;
/** Results are looked at again in full only while they are in at most this many folders. */
export const RECHECK_ALL_FOLDERS = 100;
/** The most paths one look at the results again may ask about. */
export const MAX_RECHECKED_PATHS = 5_000;

const KIB = 1024;
const MIB = 1024 * KIB;
const GIB = 1024 * MIB;
const TIB = 1024 * GIB;

export type SizePreset = "any" | "empty" | "small" | "medium" | "large" | "huge" | "custom";

export const SIZE_PRESETS: readonly {
  id: Exclude<SizePreset, "custom">;
  label: string;
  min: number | null;
  max: number | null;
}[] = [
  { id: "any", label: "Any size", min: null, max: null },
  { id: "empty", label: "Empty", min: null, max: 0 },
  { id: "small", label: "Smaller than 1 MB", min: null, max: MIB - 1 },
  { id: "medium", label: "1 MB to 100 MB", min: MIB, max: 100 * MIB },
  { id: "large", label: "Larger than 100 MB", min: 100 * MIB + 1, max: null },
  { id: "huge", label: "Larger than 1 GB", min: GIB + 1, max: null },
];

export interface SizeFilter {
  preset: SizePreset;
  /** What was typed for a custom range, as in `500 KB` or `2G`; empty leaves that end open. */
  atLeast: string;
  atMost: string;
}

export type DatePreset =
  "any" | "today" | "yesterday" | "week" | "month" | "year" | "older" | "custom";

export const DATE_PRESETS: readonly { id: Exclude<DatePreset, "custom">; label: string }[] = [
  { id: "any", label: "Any time" },
  { id: "today", label: "Today" },
  { id: "yesterday", label: "Yesterday" },
  { id: "week", label: "This week" },
  { id: "month", label: "This month" },
  { id: "year", label: "This year" },
  { id: "older", label: "Before this year" },
];

export interface DateFilter {
  preset: DatePreset;
  /** Days as `YYYY-MM-DD` in local time for a custom range; empty leaves that end open. */
  from: string;
  to: string;
}

export const DEPTH_PRESETS: readonly { depth: number | null; label: string }[] = [
  { depth: null, label: "All folders inside" },
  { depth: 1, label: "This folder only" },
  { depth: 2, label: "One level down" },
  { depth: 3, label: "Two levels down" },
  { depth: 5, label: "Four levels down" },
];

export const KIND_LABELS: Record<SearchKind, string> = {
  any: "Files and folders",
  files: "Files only",
  folders: "Folders only",
};

export interface SearchForm {
  pattern: string;
  patternKind: SearchPatternKind;
  caseSensitive: boolean;
  kind: SearchKind;
  size: SizeFilter;
  date: DateFilter;
  /** 1 looks only at what is directly in the folder; null goes all the way down. */
  depth: number | null;
  /** Whether hidden folders are searched too. */
  hidden: boolean;
  followLinks: boolean;
}

export function defaultSearchForm(hidden = false): SearchForm {
  return {
    pattern: "",
    patternKind: "text",
    caseSensitive: false,
    kind: "any",
    size: { preset: "any", atLeast: "", atMost: "" },
    date: { preset: "any", from: "", to: "" },
    depth: null,
    hidden,
    followLinks: false,
  };
}

const SIZE_UNITS: Record<string, number> = {
  "": 1,
  b: 1,
  k: KIB,
  kb: KIB,
  kib: KIB,
  m: MIB,
  mb: MIB,
  mib: MIB,
  g: GIB,
  gb: GIB,
  gib: GIB,
  t: TIB,
  tb: TIB,
  tib: TIB,
};

/**
 * Bytes, or a number with a unit in powers of 1024 as sizes show everywhere in the app: `500 KB`,
 * `1.5M`, `2 GB`. Null for empty text, undefined for text that is not a size.
 */
export function parseSizeText(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const match = /^(\d+(?:[.,]\d+)?)\s*([a-zA-Z]*)$/.exec(trimmed);
  if (!match) return undefined;
  const factor = SIZE_UNITS[match[2].toLowerCase()];
  if (factor === undefined) return undefined;
  const bytes = Math.round(Number(match[1].replace(",", ".")) * factor);
  return Number.isSafeInteger(bytes) ? bytes : undefined;
}

export type Range = { start: number | null; end: number | null } | { error: string };

/** The smallest and largest size let through, in bytes, both included. */
export function sizeRange(filter: SizeFilter): Range {
  if (filter.preset !== "custom") {
    const preset = SIZE_PRESETS.find((candidate) => candidate.id === filter.preset);
    return { start: preset?.min ?? null, end: preset?.max ?? null };
  }
  const start = parseSizeText(filter.atLeast);
  const end = parseSizeText(filter.atMost);
  if (start === undefined) return { error: `${filter.atLeast.trim()} is not a size` };
  if (end === undefined) return { error: `${filter.atMost.trim()} is not a size` };
  if (start !== null && end !== null && start > end) {
    return { error: "The smallest size is larger than the largest" };
  }
  return { start, end };
}

/** Midnight at the start of a local `YYYY-MM-DD` day, in seconds; undefined when not a day. */
function startOfDay(day: string): number | undefined {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day.trim());
  if (!match) return undefined;
  const [year, month, date] = [Number(match[1]), Number(match[2]) - 1, Number(match[3])];
  const moment = new Date(year, month, date);
  if (moment.getFullYear() !== year || moment.getMonth() !== month || moment.getDate() !== date) {
    return undefined;
  }
  return Math.floor(moment.getTime() / 1000);
}

/** The first and last second let through, both included, in seconds since the epoch. */
export function dateRange(filter: DateFilter, now: Date, weekStartsOn: number): Range {
  const [year, month, day] = [now.getFullYear(), now.getMonth(), now.getDate()];
  const seconds = (moment: Date) => Math.floor(moment.getTime() / 1000);
  const dayStart = (offset: number) => seconds(new Date(year, month, day + offset));
  const daysIntoWeek = (now.getDay() - weekStartsOn + 7) % 7;
  switch (filter.preset) {
    case "any":
      return { start: null, end: null };
    case "today":
      return { start: dayStart(0), end: null };
    case "yesterday":
      return { start: dayStart(-1), end: dayStart(0) - 1 };
    case "week":
      return { start: dayStart(-daysIntoWeek), end: null };
    case "month":
      return { start: seconds(new Date(year, month, 1)), end: null };
    case "year":
      return { start: seconds(new Date(year, 0, 1)), end: null };
    case "older":
      return { start: null, end: seconds(new Date(year, 0, 1)) - 1 };
    case "custom": {
      const from = filter.from.trim();
      const to = filter.to.trim();
      const start = from === "" ? null : startOfDay(from);
      const lastDay = to === "" ? null : startOfDay(to);
      if (start === undefined) return { error: `${from} is not a date` };
      if (lastDay === undefined) return { error: `${to} is not a date` };
      // The whole of the last day counts: up to the second before the next one starts.
      const end = lastDay === null ? null : endOfDay(to);
      if (start !== null && end !== null && start > end) {
        return { error: "The first day is after the last" };
      }
      return { start, end };
    }
  }
}

function endOfDay(day: string): number {
  const [year, month, date] = day.trim().split("-").map(Number);
  return Math.floor(new Date(year, month - 1, date + 1).getTime() / 1000) - 1;
}

export interface SearchPlace {
  searchId: string;
  location: FileLocation;
  folder: string;
  now: Date;
  weekStartsOn: number;
}

/** The request the backend gets for `form`, or why it cannot be made. */
export function buildSearchRequest(
  form: SearchForm,
  { searchId, location, folder, now, weekStartsOn }: SearchPlace,
): SearchRequest | { error: string } {
  const sizes = sizeRange(form.size);
  if ("error" in sizes) return sizes;
  const dates = dateRange(form.date, now, weekStartsOn);
  if ("error" in dates) return dates;
  return {
    searchId,
    location,
    folder,
    pattern: form.pattern,
    patternKind: form.patternKind,
    caseSensitive: form.caseSensitive,
    kind: form.kind,
    minSize: sizes.start,
    maxSize: sizes.end,
    modifiedAfter: dates.start,
    modifiedBefore: dates.end,
    maxDepth: form.depth,
    hidden: form.hidden,
    followLinks: form.followLinks,
    limit: SEARCH_LIMIT,
  };
}

/** What the size button says: the preset, or the custom range in short. */
export function describeSize(filter: SizeFilter): string {
  if (filter.preset !== "custom") {
    return SIZE_PRESETS.find((preset) => preset.id === filter.preset)?.label ?? "Any size";
  }
  const atLeast = filter.atLeast.trim();
  const atMost = filter.atMost.trim();
  if (atLeast && atMost) return `${atLeast} to ${atMost}`;
  if (atLeast) return `At least ${atLeast}`;
  if (atMost) return `At most ${atMost}`;
  return "Any size";
}

export function describeDate(filter: DateFilter): string {
  if (filter.preset !== "custom") {
    return DATE_PRESETS.find((preset) => preset.id === filter.preset)?.label ?? "Any time";
  }
  const from = filter.from.trim();
  const to = filter.to.trim();
  if (from && to) return from === to ? `On ${from}` : `${from} to ${to}`;
  if (from) return `Since ${from}`;
  if (to) return `Until ${to}`;
  return "Any time";
}

export function describeDepth(depth: number | null): string {
  const preset = DEPTH_PRESETS.find((candidate) => candidate.depth === depth);
  if (preset) return preset.label;
  return depth === null ? "All folders inside" : `${depth - 1} levels down`;
}

/** One line for a remembered search: what it looked for, then each filter that narrows it. */
export function describeSearch(form: SearchForm): string {
  const parts = [form.pattern === "" ? "Everything" : form.pattern];
  if (form.patternKind === "regex") parts.push("regular expression");
  if (form.caseSensitive) parts.push("match case");
  if (form.kind !== "any") parts.push(KIND_LABELS[form.kind].toLowerCase());
  if (sizeNarrows(form.size)) parts.push(describeSize(form.size).toLowerCase());
  if (dateNarrows(form.date)) parts.push(describeDate(form.date).toLowerCase());
  if (form.depth !== null) parts.push(describeDepth(form.depth).toLowerCase());
  if (form.hidden) parts.push("hidden folders");
  if (form.followLinks) parts.push("following links");
  return parts.join(", ");
}

export function sizeNarrows(filter: SizeFilter): boolean {
  return describeSize(filter) !== "Any size";
}

export function dateNarrows(filter: DateFilter): boolean {
  return describeDate(filter) !== "Any time";
}

function sameSearch(first: SearchForm, second: SearchForm): boolean {
  return JSON.stringify(first) === JSON.stringify(second);
}

/** The recent searches with `form` first, each once, at most `limit` of them. */
export function rememberSearch(
  recent: readonly SearchForm[],
  form: SearchForm,
  limit = RECENT_SEARCHES,
): SearchForm[] {
  return [form, ...recent.filter((earlier) => !sameSearch(earlier, form))].slice(0, limit);
}

/** The folder an entry is in, from its path: `/srv/a/b.txt` -> `/srv/a`, `C:\b.txt` -> `C:\`. */
export function folderOf(entry: Pick<FileEntry, "path" | "name">, style: PathStyle): string {
  const { path, name } = entry;
  const separators = style === "windows" ? /[\\/]+$/ : /\/+$/;
  const folder = path.endsWith(name) ? path.slice(0, path.length - name.length) : path;
  const trimmed = folder.replace(separators, "");
  if (trimmed === "") return style === "windows" ? folder : "/";
  // A drive keeps its separator, as the folder at its top is named.
  return style === "windows" && /^[a-zA-Z]:$/.test(trimmed) ? `${trimmed}\\` : trimmed;
}

/**
 * Where an entry is, relative to the folder a search started in: `logs/old` for an entry in
 * `<root>/logs/old`, `.` for one in the folder itself.
 */
export function relativeFolder(root: string, entry: FileEntry, style: PathStyle): string {
  const folder = folderOf(entry, style);
  if (samePath(folder, root, style)) return ".";
  if (!isWithin(folder, root, style)) return folder;
  // Measured on the root as written, not as compared: lower case can be longer, as İ is.
  const outer = root.replace(style === "windows" ? /[\\/]+$/ : /\/+$/, "");
  return folder.slice(outer.length + 1);
}

/** The folder every entry is in, when they share one. */
export function sharedFolder(entries: readonly FileEntry[], style: PathStyle): string | null {
  const [first, ...rest] = entries;
  if (!first) return null;
  const folder = folderOf(first, style);
  return rest.every((entry) => samePath(folderOf(entry, style), folder, style)) ? folder : null;
}

/**
 * The paths to look at again when the folder a search started in changed: every result while
 * they are in few folders, else those directly in it, plus `extra` either way.
 */
export function recheckTargets(
  entries: readonly FileEntry[],
  root: string,
  style: PathStyle,
  extra: Iterable<string> = [],
): string[] {
  const folders = new Set(entries.map((entry) => normalizedPath(folderOf(entry, style), style)));
  const wanted =
    folders.size <= RECHECK_ALL_FOLDERS
      ? entries
      : entries.filter((entry) => samePath(folderOf(entry, style), root, style));
  const paths = new Set([...extra, ...wanted.map((entry) => entry.path)]);
  return [...paths].slice(0, MAX_RECHECKED_PATHS);
}

/**
 * The paths to look at again after `changed` results were renamed, moved or deleted: theirs,
 * the `added` paths they go by now, and every result inside a changed folder, whose path went
 * with it.
 */
export function recheckAfterChange(
  results: readonly FileEntry[],
  changed: readonly FileEntry[],
  added: readonly string[],
  style: PathStyle,
): string[] {
  const folders = changed.filter(isDirLike).map((entry) => entry.path);
  const inside = results
    .filter((result) =>
      folders.some(
        (folder) => isWithin(result.path, folder, style) && !samePath(result.path, folder, style),
      ),
    )
    .map((result) => result.path);
  const paths = new Set([...changed.map((entry) => entry.path), ...added, ...inside]);
  return [...paths].slice(0, MAX_RECHECKED_PATHS);
}

/**
 * Whether a pane's list shows hidden items: as the eye toggle says for a folder, and always
 * for search results, which hold only what the search chose to find.
 */
export function showsHiddenItems(showHidden: boolean, showingResults: boolean): boolean {
  return showHidden || showingResults;
}

/**
 * Two reports of a running search as one, to show at once: what each found and could not read,
 * progress as of the later, and the start folder's name from whichever gave it.
 */
export function mergeReports(earlier: SearchReport | null, later: SearchReport): SearchReport {
  if (!earlier) return later;
  return {
    ...later,
    entries: [...earlier.entries, ...later.entries],
    unreadable: [...earlier.unreadable, ...later.unreadable],
    start: later.start ?? earlier.start,
  };
}

/**
 * The results after a look at `asked` again: items still there as they are now, items gone
 * left out, and asked items not shown before, such as one renamed, added at the end. Items the
 * look could not tell about stay as they were.
 */
export function applyRecheck(
  entries: readonly FileEntry[],
  recheck: SearchRecheck,
  asked: readonly string[],
): FileEntry[] {
  const found = new Map(recheck.entries.map((entry) => [entry.path, entry]));
  const missing = new Set(recheck.missing);
  const result: FileEntry[] = [];
  const shown = new Set<string>();
  for (const entry of entries) {
    if (missing.has(entry.path)) continue;
    result.push(found.get(entry.path) ?? entry);
    shown.add(entry.path);
  }
  for (const path of asked) {
    const entry = found.get(path);
    if (entry && !shown.has(path)) {
      result.push(entry);
      shown.add(path);
    }
  }
  return result;
}
