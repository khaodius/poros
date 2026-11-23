// How the folders two panes show side by side differ, name by name, decided as a two-way
// synchronization decides it (`plan` in src-tauri/src/sync/plan.rs, read the way
// src-tauri/src/sync/folder_compare.rs reads it), so a row's color agrees with what Synchronize
// folders would do. Both suites run the cases in
// src-tauri/tests/fixtures/folder_compare_cases.json, so the two cannot drift apart unnoticed.

import type { PathStyle } from "./fileSource";
import { pluralize } from "./format";
import { normalizedPath } from "./path";
import type { FolderCompareSettings } from "./settings";
import { isDirLike } from "./sort";
import type { CompareMode, FileEntry, FolderSummary, Protocol } from "./types";

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
/** The furthest apart two time zones are: UTC+14 and UTC-12. */
const MAX_ZONE_OFFSET_HOURS = 26;
/** As `MAX_TIME_TOLERANCE_SECS` in src-tauri/src/sync/options.rs. */
const MAX_TIME_TOLERANCE_SECS = 86400;

/**
 * What a comparison needs of a listed entry. A link counts as what it leads to, as a
 * synchronization that follows links sees it.
 */
export interface CompareEntry {
  name: string;
  /** `other` is what a synchronization passes over: special files and links that lead nowhere. */
  shape: "folder" | "file" | "other";
  size: number;
  /** Seconds since the Unix epoch. */
  modified: number | null;
  hidden: boolean;
}

export function compareEntry(entry: FileEntry): CompareEntry {
  const file = entry.kind === "file" || (entry.kind === "symlink" && entry.linkTarget === "file");
  return {
    name: entry.name,
    shape: isDirLike(entry) ? "folder" : file ? "file" : "other",
    size: entry.size,
    modified: entry.modified,
    hidden: entry.hidden,
  };
}

export function compareEntries(entries: readonly FileEntry[]): CompareEntry[] {
  return entries.map(compareEntry);
}

/** Whether two listings hold the same things, so a comparison of one stands for the other. */
export function sameEntries(first: readonly CompareEntry[], second: readonly CompareEntry[]) {
  if (first === second) return true;
  if (first.length !== second.length) return false;
  return first.every((entry, index) => {
    const other = second[index];
    return (
      entry.name === other.name &&
      entry.shape === other.shape &&
      entry.size === other.size &&
      entry.modified === other.modified &&
      entry.hidden === other.hidden
    );
  });
}

export interface CompareRules {
  /** Files of one size are the same whatever their times. */
  sizeOnly: boolean;
  /** Times this close are the same (`--modify-window`). */
  toleranceSecs: number;
  /** Times a whole number of hours apart are the same. */
  ignoreHourOffsets: boolean;
  includeHidden: boolean;
  /** Names that differ only in case are one name. */
  ignoreCase: boolean;
  /** The side's listing gives times to the minute, and to the day for older files. */
  leftCoarse: boolean;
  rightCoarse: boolean;
}

/** The comparison's rules from the bar's options and the defaults in Settings > Sync. */
export function compareRules(
  options: FolderCompareSettings,
  syncDefaults: { compare: CompareMode; timeToleranceSecs: number },
  sides: { ignoreCase: boolean; leftCoarse: boolean; rightCoarse: boolean },
): CompareRules {
  const tolerance = Number.isFinite(syncDefaults.timeToleranceSecs)
    ? Math.round(syncDefaults.timeToleranceSecs)
    : 0;
  return {
    sizeOnly: sizeOnlyIn(options, syncDefaults.compare),
    toleranceSecs: Math.min(MAX_TIME_TOLERANCE_SECS, Math.max(0, tolerance)),
    ignoreHourOffsets: options.ignoreHourOffsets,
    includeHidden: options.includeHidden,
    ...sides,
  };
}

/** Whether size alone decides: as set in the bar, or as Settings > Sync has it until then. */
export function sizeOnlyIn(options: FolderCompareSettings, syncCompare: CompareMode): boolean {
  return options.sizeRule === "sync" ? syncCompare === "sizeOnly" : options.sizeRule === "sizeOnly";
}

/** Whether names that differ only in case are one name where the files are. */
export function ignoresCase(pathStyle: PathStyle, protocol: Protocol | undefined): boolean {
  return pathStyle === "windows" || protocol === "oneDrive" || protocol === "box";
}

/**
 * How far the real time may be from one a coarse listing gives: a day for a time at midnight,
 * which is how a listing with only a date reads, and a minute otherwise.
 */
function slack(time: number, coarse: boolean): number {
  if (!coarse) return 0;
  return ((time % DAY) + DAY) % DAY === 0 ? DAY : MINUTE;
}

/** Whether two files' times count as the same moment. */
export function sameMoment(left: number, right: number, rules: CompareRules): boolean {
  const tolerance = Math.max(
    rules.toleranceSecs,
    slack(left, rules.leftCoarse),
    slack(right, rules.rightCoarse),
  );
  const difference = left - right;
  if (Math.abs(difference) <= tolerance) return true;
  if (!rules.ignoreHourOffsets) return false;
  const hours = Math.floor((difference + HOUR / 2) / HOUR);
  return (
    hours !== 0 &&
    Math.abs(hours) <= MAX_ZONE_OFFSET_HOURS &&
    Math.abs(difference - hours * HOUR) <= tolerance
  );
}

/** What a two-way synchronization makes of one name, with the left side in the local place. */
export type Outcome =
  | "identical"
  | "bothFolders"
  | "onlyLeft"
  | "onlyRight"
  | "newerLeft"
  | "newerRight"
  | "different"
  /** One side has something a synchronization passes over, so it decides nothing here. */
  | "passedOver";

type Compared = Pick<CompareEntry, "shape" | "size" | "modified">;

function bothFiles(left: Compared, right: Compared, rules: CompareRules): Outcome {
  const sameSize = left.size === right.size;
  if (rules.sizeOnly && sameSize) return "identical";
  const timed = left.modified !== null && right.modified !== null;
  const close = timed
    ? sameMoment(left.modified as number, right.modified as number, rules)
    : left.modified === right.modified;
  if (close) return sameSize ? "identical" : "different";
  // Neither is newer when one of the two has no time.
  if (!timed) return "different";
  return (left.modified as number) > (right.modified as number) ? "newerLeft" : "newerRight";
}

/** Decides one name; `undefined` is a side that has nothing of that name. */
export function decide(
  left: Compared | undefined,
  right: Compared | undefined,
  rules: CompareRules,
): Outcome {
  if (left?.shape === "other" || right?.shape === "other") return "passedOver";
  if (!left) return right ? "onlyRight" : "passedOver";
  if (!right) return "onlyLeft";
  if (left.shape === "folder" && right.shape === "folder") return "bothFolders";
  if (left.shape === "folder" || right.shape === "folder") return "different";
  return bothFiles(left, right, rules);
}

/** The five states rows are colored by, as the legend lists them. */
export const LEGEND_STATES = ["newer", "older", "onlyHere", "different", "identical"] as const;
export type LegendState = (typeof LEGEND_STATES)[number];

/**
 * How an entry compares with the one of its name in the other pane, seen from its own pane.
 * `both` is a folder the two sides have, about whose contents nothing is claimed; `passedOver`
 * is something not compared.
 */
export type CompareState = LegendState | "both" | "passedOver";

const SEEN_FROM: Record<Outcome, { left: CompareState; right: CompareState }> = {
  identical: { left: "identical", right: "identical" },
  bothFolders: { left: "both", right: "both" },
  onlyLeft: { left: "onlyHere", right: "passedOver" },
  onlyRight: { left: "passedOver", right: "onlyHere" },
  newerLeft: { left: "newer", right: "older" },
  newerRight: { left: "older", right: "newer" },
  different: { left: "different", right: "different" },
  passedOver: { left: "passedOver", right: "passedOver" },
};

export interface ListingComparison {
  /** The state of each compared entry of a side, by its name as listed there. */
  left: ReadonlyMap<string, CompareState>;
  right: ReadonlyMap<string, CompareState>;
}

interface NameIndex {
  exact: Map<string, CompareEntry>;
  /** By lower-case name, while case is ignored. */
  folded: Map<string, CompareEntry> | null;
  /** Lower-case names two entries share, while case is ignored. */
  collided: Set<string>;
}

function indexByName(entries: readonly CompareEntry[], rules: CompareRules): NameIndex {
  const exact = new Map<string, CompareEntry>();
  const folded = rules.ignoreCase ? new Map<string, CompareEntry>() : null;
  const collided = new Set<string>();
  for (const entry of entries) {
    if (entry.hidden && !rules.includeHidden) continue;
    exact.set(entry.name, entry);
    if (!folded) continue;
    const key = entry.name.toLowerCase();
    if (folded.has(key)) collided.add(key);
    else folded.set(key, entry);
  }
  return { exact, folded, collided };
}

/** The entry of that name, in its own spelling before another that differs only in case. */
function named(index: NameIndex, name: string): CompareEntry | undefined {
  return index.exact.get(name) ?? index.folded?.get(name.toLowerCase());
}

/**
 * Compares two listings by name: one pass over each, with the other side found in a map, so a
 * folder of a hundred thousand entries is compared once when it is listed and never while
 * scrolling. Hidden entries are left out unless they count. While case is ignored, a name one
 * side has in two spellings is compared on neither side, since which of them the other side's
 * entry stands for is unknown; the comparison of subfolders passes it over the same way.
 */
export function compareListings(
  left: readonly CompareEntry[],
  right: readonly CompareEntry[],
  rules: CompareRules,
): ListingComparison {
  const leftIndex = indexByName(left, rules);
  const rightIndex = indexByName(right, rules);
  const ambiguous = (name: string) => {
    if (!rules.ignoreCase) return false;
    const key = name.toLowerCase();
    return leftIndex.collided.has(key) || rightIndex.collided.has(key);
  };
  const leftStates = new Map<string, CompareState>();
  const rightStates = new Map<string, CompareState>();
  for (const entry of leftIndex.exact.values()) {
    const outcome = ambiguous(entry.name)
      ? "passedOver"
      : decide(entry, named(rightIndex, entry.name), rules);
    leftStates.set(entry.name, SEEN_FROM[outcome].left);
  }
  for (const entry of rightIndex.exact.values()) {
    const outcome = ambiguous(entry.name)
      ? "passedOver"
      : decide(named(leftIndex, entry.name), entry, rules);
    rightStates.set(entry.name, SEEN_FROM[outcome].right);
  }
  return { left: leftStates, right: rightStates };
}

/** What a row shows of the comparison: its tint and glyph, and the words under the pointer. */
export interface RowCompare {
  state: MarkedState;
  title: string;
}

export const STATE_LABELS: Record<LegendState, string> = {
  newer: "Newer here",
  older: "Older here",
  onlyHere: "Only here",
  different: "Different",
  identical: "Identical",
};

/**
 * The states a row shows; what is passed over gets no mark. `differsInside` is a folder in which
 * the comparison of subfolders found differences: it is in none of the legend's states, so
 * selecting by state never takes a whole folder for what is inside it.
 */
export type MarkedState = Exclude<CompareState, "passedOver"> | "differsInside";

/** One mark per state, shared by every row in it, so rows are not drawn again for nothing. */
const STATE_MARKS: Record<Exclude<CompareState, "passedOver">, RowCompare> = {
  newer: { state: "newer", title: "Newer here" },
  older: { state: "older", title: "Older here" },
  onlyHere: { state: "onlyHere", title: "Only here" },
  different: { state: "different", title: "Different, and neither is newer" },
  identical: { state: "identical", title: "Identical" },
  both: { state: "both", title: "On both sides. The contents are not compared." },
};

/** The folders both sides have below two compared folders, and what was found in each. */
export interface DeepComparison {
  /** The folders compared, as the two panes showed them. */
  leftRoot: string;
  rightRoot: string;
  /** Paths are spelled in lower case. */
  ignoreCase: boolean;
  /** By path below the roots, with `/` between names. */
  folders: ReadonlyMap<string, FolderSummary>;
  /** More folders were compared than are kept; the deepest are missing. */
  truncated: boolean;
}

/** `path` below `root`, with `/` between names: "" for the root itself, null outside it. */
export function pathBelow(path: string, root: string, style: PathStyle): string | null {
  const full = normalizedPath(path, style);
  const base = normalizedPath(root, style);
  if (full === base) return "";
  const separator = style === "windows" ? "\\" : "/";
  const prefix = base.endsWith(separator) ? base : base + separator;
  if (!full.startsWith(prefix)) return null;
  return full.slice(prefix.length).split(separator).join("/");
}

/**
 * Where in a deep comparison two panes are when they show the same folder below its roots: the
 * path to find their folders by. Null when they do not.
 */
export function deepPrefix(
  deep: DeepComparison,
  left: { path: string; pathStyle: PathStyle },
  right: { path: string; pathStyle: PathStyle },
): string | null {
  const leftBelow = pathBelow(left.path, deep.leftRoot, left.pathStyle);
  const rightBelow = pathBelow(right.path, deep.rightRoot, right.pathStyle);
  if (leftBelow === null || rightBelow === null) return null;
  const fold = (path: string) => (deep.ignoreCase ? path.toLowerCase() : path);
  return fold(leftBelow) === fold(rightBelow) ? fold(leftBelow) : null;
}

function differences(summary: FolderSummary): number {
  return (
    summary.newerLeft +
    summary.newerRight +
    summary.onlyLeft +
    summary.onlyRight +
    summary.different
  );
}

/**
 * What the compare bar says while a pane shows a remembered listing that could not be refreshed:
 * the rows are compared as shown, and compared again once the folder is listed.
 */
export function outdatedNote(here: boolean, there: boolean, partner: string): string | undefined {
  const after = "so the marks may be out of date until it is listed again";
  if (here && there) {
    return "Neither pane's listing could be refreshed, so the marks may be out of date until both are listed again";
  }
  if (here) return `This pane's listing could not be refreshed, ${after}`;
  if (there) return `The listing in ${partner} could not be refreshed, ${after}`;
  return undefined;
}

/** What the compare bar says while search results are on show in place of a compared folder. */
export function pausedNote(here: boolean, there: boolean, partner: string): string | undefined {
  const after = "The marks come back when the results are closed";
  if (here && there) return `Comparing is paused while both panes show search results. ${after}`;
  if (here) return `Comparing is paused while this pane shows search results. ${after}`;
  if (there) return `Comparing is paused while ${partner} shows search results. ${after}`;
  return undefined;
}

/** What a folder's summary says in words, seen from one side: "2 newer here, 1 only there". */
export function describeSummary(summary: FolderSummary, side: "left" | "right"): string {
  const here = side === "left";
  const parts: [number, string][] = [
    [here ? summary.newerLeft : summary.newerRight, "newer here"],
    [here ? summary.newerRight : summary.newerLeft, "older here"],
    [here ? summary.onlyLeft : summary.onlyRight, "only here"],
    [here ? summary.onlyRight : summary.onlyLeft, "only in the other pane"],
    [summary.different, "different"],
    [summary.identical, "identical"],
    [summary.notCompared, "not compared"],
  ];
  return parts
    .filter(([count]) => count > 0)
    .map(([count, label]) => `${count.toLocaleString()} ${label}`)
    .join(", ");
}

function folderMark(summary: FolderSummary, side: "left" | "right"): RowCompare {
  const found = describeSummary(summary, side);
  if (differences(summary) > 0)
    return { state: "differsInside", title: `Differs inside: ${found}` };
  if (summary.notCompared > 0) {
    return { state: "both", title: `No differences found inside, but not all of it: ${found}` };
  }
  return {
    state: "identical",
    title:
      summary.identical > 0
        ? `Identical inside: ${pluralize(summary.identical, "file")}`
        : "Identical inside: no files on either side",
  };
}

/**
 * The mark of each compared row of one side. Folders both sides have take theirs from the deep
 * comparison, where it covers the folder on show (`prefix` is where in it that folder is).
 */
export function rowMarks(
  states: ReadonlyMap<string, CompareState>,
  side: "left" | "right",
  deep?: { comparison: DeepComparison; prefix: string } | null,
): Map<string, RowCompare> {
  const marks = new Map<string, RowCompare>();
  for (const [name, state] of states) {
    if (state === "passedOver") continue;
    let mark = STATE_MARKS[state];
    if (state === "both" && deep) {
      const spelled = deep.comparison.ignoreCase ? name.toLowerCase() : name;
      const summary = deep.comparison.folders.get(
        deep.prefix ? `${deep.prefix}/${spelled}` : spelled,
      );
      if (summary) mark = folderMark(summary, side);
    }
    marks.set(name, mark);
  }
  return marks;
}

export type LegendCounts = Record<LegendState, number>;

const LEGEND_STATE_SET: ReadonlySet<MarkedState> = new Set(LEGEND_STATES);

function isLegendState(state: MarkedState): state is LegendState {
  return LEGEND_STATE_SET.has(state);
}

/** How many of `entries` are in each state, for the legend over the rows a pane can show. */
export function legendCounts(
  entries: readonly { name: string }[],
  marks: ReadonlyMap<string, RowCompare>,
): LegendCounts {
  const counts: LegendCounts = { newer: 0, older: 0, onlyHere: 0, different: 0, identical: 0 };
  for (const entry of entries) {
    const state = marks.get(entry.name)?.state;
    if (state && isLegendState(state)) counts[state] += 1;
  }
  return counts;
}

/** The names in `marks` that are in one of `states`. */
export function namesIn(
  marks: ReadonlyMap<string, RowCompare>,
  states: readonly MarkedState[],
): Set<string> {
  const names = new Set<string>();
  for (const [name, mark] of marks) if (states.includes(mark.state)) names.add(name);
  return names;
}
