// Renaming many items of one folder at once: the rules that make each new name, what is wrong
// with a name before anything runs, and an order of renames that never needs a name still in use.

import type { PathStyle } from "./fileSource";
import type { PlannedRename, Protocol, RenameGroup, RenameSummary } from "./types";
import { t } from "../i18n";

export type FindMode = "text" | "regex";
/** The part of a name find and replace works on. */
export type FindScope = "name" | "extension" | "both";
export type LetterCase = "unchanged" | "lower" | "upper" | "title" | "sentence";
export type ExtensionChange = "keep" | "lower" | "upper" | "replace" | "remove";
/** The order items are counted in: as the list shows them, or by name. */
export type CounterOrder = "list" | "name";
export type CounterPlacement = "prefix" | "suffix";

export const FIND_MODES: FindMode[] = ["text", "regex"];
export const FIND_SCOPES: FindScope[] = ["name", "extension", "both"];
export const LETTER_CASES: LetterCase[] = ["unchanged", "lower", "upper", "title", "sentence"];
export const EXTENSION_CHANGES: ExtensionChange[] = ["keep", "lower", "upper", "replace", "remove"];
export const DATE_STYLES = [
  "YYYY-MM-DD",
  "YYYYMMDD",
  "DD-MM-YYYY",
  "MM-DD-YYYY",
  "YYMMDD",
] as const;
export const TIME_STYLES = ["HH-MM-SS", "HHMMSS", "HH-MM", "HHMM"] as const;
export type DateStyle = (typeof DATE_STYLES)[number];
export type TimeStyle = (typeof TIME_STYLES)[number];

export interface FindRule {
  enabled: boolean;
  mode: FindMode;
  pattern: string;
  replacement: string;
  caseSensitive: boolean;
  /** Replaces every match; off replaces the first one only. */
  allMatches: boolean;
  scope: FindScope;
}

export interface TemplateRule {
  enabled: boolean;
  /** The new name, with tokens such as `{name}` and `{n}` filled in for each item. */
  pattern: string;
  dateStyle: DateStyle;
  timeStyle: TimeStyle;
}

export interface CounterRule {
  /** Adds the counter before or after each name. A template places it with `{n}` instead. */
  enabled: boolean;
  start: number;
  step: number;
  /** Counters are padded with zeros to this many digits. */
  digits: number;
  order: CounterOrder;
  placement: CounterPlacement;
  /** Stands between the counter and the name. */
  separator: string;
}

export interface ExtensionRule {
  change: ExtensionChange;
  /** The new extension for `replace`, with or without its dot. */
  replacement: string;
}

export interface TidyRule {
  enabled: boolean;
  trim: boolean;
  collapseSpaces: boolean;
  replaceSpaces: boolean;
  /** What each space becomes; nothing removes them. */
  spaceReplacement: string;
}

/** The rules, in the order they apply. */
export interface RenameRules {
  find: FindRule;
  template: TemplateRule;
  counter: CounterRule;
  letterCase: LetterCase;
  extension: ExtensionRule;
  tidy: TidyRule;
}

export const COUNTER_DIGIT_LIMITS = { min: 1, max: 12 };

export const DEFAULT_RULES: RenameRules = {
  find: {
    enabled: true,
    mode: "text",
    pattern: "",
    replacement: "",
    caseSensitive: false,
    allMatches: true,
    scope: "name",
  },
  template: { enabled: false, pattern: "", dateStyle: "YYYY-MM-DD", timeStyle: "HH-MM-SS" },
  counter: {
    enabled: false,
    start: 1,
    step: 1,
    digits: 2,
    order: "list",
    placement: "suffix",
    separator: "_",
  },
  letterCase: "unchanged",
  extension: { change: "keep", replacement: "" },
  tidy: {
    enabled: false,
    trim: true,
    collapseSpaces: true,
    replaceSpaces: false,
    spaceReplacement: "_",
  },
};

export interface TemplateToken {
  token: string;
  meaning: string;
}

export const TEMPLATE_TOKENS: TemplateToken[] = [
  {
    token: "{name}",
    get meaning() {
      return t("rename.nameWithoutTheExtension");
    },
  },
  {
    token: "{ext}",
    get meaning() {
      return t("rename.extension");
    },
  },
  {
    token: "{n}",
    get meaning() {
      return t("rename.counter");
    },
  },
  {
    token: "{date}",
    get meaning() {
      return t("rename.dateModified");
    },
  },
  {
    token: "{time}",
    get meaning() {
      return t("rename.timeModified");
    },
  },
  {
    token: "{parent}",
    get meaning() {
      return t("rename.nameOfTheFolder");
    },
  },
];

/** What a rename works from: an item as the folder lists it. */
export interface RenameItem {
  name: string;
  /** Folders have no extension: their whole name is the name. */
  isFolder: boolean;
  /** Seconds since the Unix epoch. */
  modified: number | null;
}

export interface NameParts {
  stem: string;
  /** Without its dot; null when the name has none. */
  extension: string | null;
}

/**
 * `photo.final.jpg` is `photo.final` and `jpg`. Folders, dotfiles such as `.bashrc` and names
 * that end in a dot have no extension.
 */
export function splitName(name: string, isFolder: boolean): NameParts {
  const dot = name.lastIndexOf(".");
  if (isFolder || dot <= 0 || dot === name.length - 1) return { stem: name, extension: null };
  return { stem: name.slice(0, dot), extension: name.slice(dot + 1) };
}

export function joinName({ stem, extension }: NameParts): string {
  return extension ? `${stem}.${extension}` : stem;
}

interface CompiledFind {
  expression: RegExp;
  replacement: string | (() => string);
}

export interface FindCheck {
  find: CompiledFind | null;
  /** Why the pattern is not a regular expression, in the engine's own words. */
  error: string | null;
}

const REGEXP_SPECIALS = /[.*+?^${}()|[\]\\]/g;
const ERROR_PREFIX = "Invalid regular expression: ";

/**
 * The find rule ready to run, or what is wrong with its pattern. Regular expressions always
 * match by code point, so a match never ends in the middle of an emoji.
 */
export function compileFind(rule: FindRule): FindCheck {
  if (!rule.enabled || rule.pattern === "") return { find: null, error: null };
  const flags = `${rule.allMatches ? "g" : ""}${rule.caseSensitive ? "" : "i"}u`;
  if (rule.mode === "text") {
    const literal = rule.pattern.replace(REGEXP_SPECIALS, "\\$&");
    const { replacement } = rule;
    return {
      find: { expression: new RegExp(literal, flags), replacement: () => replacement },
      error: null,
    };
  }
  try {
    return {
      find: { expression: new RegExp(rule.pattern, flags), replacement: rule.replacement },
      error: null,
    };
  } catch (caught) {
    return { find: null, error: regexpProblem(caught, rule.pattern, flags) };
  }
}

/** The engine's message without the lead-in and the pattern, which the field already shows. */
function regexpProblem(caught: unknown, pattern: string, flags: string): string {
  let message = caught instanceof Error ? caught.message : String(caught);
  if (message.startsWith(ERROR_PREFIX)) message = message.slice(ERROR_PREFIX.length);
  const quoted = `/${pattern}/${flags}: `;
  if (message.startsWith(quoted)) message = message.slice(quoted.length);
  return message.charAt(0).toUpperCase() + message.slice(1);
}

function findAndReplace(text: string, find: CompiledFind): string {
  // The two overloads of `replace` take a string or a function, not their union.
  return typeof find.replacement === "string"
    ? text.replace(find.expression, find.replacement)
    : text.replace(find.expression, find.replacement);
}

function twoDigits(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/** The local date of `epochSeconds`, in a form that is safe in a file name. */
export function formatDateToken(epochSeconds: number, style: DateStyle): string {
  const date = new Date(epochSeconds * 1000);
  const year = String(date.getFullYear()).padStart(4, "0");
  const month = twoDigits(date.getMonth() + 1);
  const day = twoDigits(date.getDate());
  switch (style) {
    case "YYYY-MM-DD":
      return `${year}-${month}-${day}`;
    case "YYYYMMDD":
      return `${year}${month}${day}`;
    case "DD-MM-YYYY":
      return `${day}-${month}-${year}`;
    case "MM-DD-YYYY":
      return `${month}-${day}-${year}`;
    case "YYMMDD":
      return `${year.slice(-2)}${month}${day}`;
  }
}

/** The local time of `epochSeconds`, without the colons file names cannot have everywhere. */
export function formatTimeToken(epochSeconds: number, style: TimeStyle): string {
  const date = new Date(epochSeconds * 1000);
  const hours = twoDigits(date.getHours());
  const minutes = twoDigits(date.getMinutes());
  const seconds = twoDigits(date.getSeconds());
  switch (style) {
    case "HH-MM-SS":
      return `${hours}-${minutes}-${seconds}`;
    case "HHMMSS":
      return `${hours}${minutes}${seconds}`;
    case "HH-MM":
      return `${hours}-${minutes}`;
    case "HHMM":
      return `${hours}${minutes}`;
  }
}

/** `7` padded to three digits is `007`; the sign of a negative counter stays in front. */
export function formatCounter(value: number, digits: number): string {
  const magnitude = String(Math.abs(value)).padStart(digits, "0");
  return value < 0 ? `-${magnitude}` : magnitude;
}

const nameCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
const nameOrders = new WeakMap<readonly RenameItem[], Uint32Array>();

/**
 * Where each item stands when the items are put in name order, the way a list sorted by name
 * shows them. Kept per list of items, as sorting many thousands of names takes a moment.
 */
function nameOrder(items: readonly RenameItem[]): Uint32Array {
  const known = nameOrders.get(items);
  if (known) return known;
  const sorted = items
    .map((item, index) => ({ name: item.name, index }))
    .sort(
      (left, right) =>
        nameCollator.compare(left.name, right.name) ||
        (left.name < right.name ? -1 : left.name > right.name ? 1 : left.index - right.index),
    );
  const positions = new Uint32Array(items.length);
  sorted.forEach(({ index }, position) => {
    positions[index] = position;
  });
  nameOrders.set(items, positions);
  return positions;
}

function wholeNumber(value: number, fallback: number): number {
  return Number.isFinite(value) ? Math.trunc(value) : fallback;
}

/** The counter each item gets, as text, in the order of `items`. */
export function counterValues(items: readonly RenameItem[], rule: CounterRule): string[] {
  const start = wholeNumber(rule.start, DEFAULT_RULES.counter.start);
  const step = wholeNumber(rule.step, DEFAULT_RULES.counter.step);
  const digits = Math.min(
    COUNTER_DIGIT_LIMITS.max,
    Math.max(COUNTER_DIGIT_LIMITS.min, wholeNumber(rule.digits, DEFAULT_RULES.counter.digits)),
  );
  const positions = rule.order === "name" ? nameOrder(items) : null;
  return items.map((_, index) =>
    formatCounter(start + (positions ? positions[index] : index) * step, digits),
  );
}

const WORD = /[\p{L}\p{N}][\p{L}\p{M}\p{N}'’]*/gu;
const FIRST_LETTER = /\p{L}/u;

function upperFirst(word: string): string {
  const first = String.fromCodePoint(word.codePointAt(0) ?? 0);
  return first.toUpperCase() + word.slice(first.length);
}

export function applyLetterCase(text: string, letterCase: LetterCase): string {
  switch (letterCase) {
    case "unchanged":
      return text;
    case "lower":
      return text.toLowerCase();
    case "upper":
      return text.toUpperCase();
    case "title":
      return text.toLowerCase().replace(WORD, upperFirst);
    case "sentence":
      return text.toLowerCase().replace(FIRST_LETTER, (letter) => letter.toUpperCase());
  }
}

function tidy(text: string, rule: TidyRule): string {
  let tidied = text;
  if (rule.trim) tidied = tidied.trim();
  if (rule.collapseSpaces) tidied = tidied.replace(/\s+/g, " ");
  if (rule.replaceSpaces) tidied = tidied.replace(/\s/g, rule.spaceReplacement);
  return tidied;
}

const TOKEN = /(\.?)\{(name|ext|n|date|time|parent)\}/g;

export function templateIsActive(rule: TemplateRule): boolean {
  return rule.enabled && rule.pattern !== "";
}

function templateUses(rule: TemplateRule, token: string): boolean {
  return templateIsActive(rule) && rule.pattern.includes(`{${token}}`);
}

interface TokenValues {
  name: string;
  ext: string;
  n: string;
  date: string;
  time: string;
  parent: string;
}

/**
 * The template with its tokens filled in. A dot written before `{ext}` goes with the extension:
 * an item that has none gets neither. Anything else in braces stays as typed.
 */
function fillTemplate(pattern: string, values: TokenValues): string {
  return pattern.replace(TOKEN, (_, dot: string, token: keyof TokenValues) => {
    if (token === "ext" && values.ext === "") return "";
    return dot + values[token];
  });
}

function cleanExtension(text: string): string {
  return text.trim().replace(/^\.+/, "");
}

function changeExtension(parts: NameParts, rule: ExtensionRule): NameParts {
  switch (rule.change) {
    case "keep":
      return parts;
    case "lower":
      return { ...parts, extension: parts.extension?.toLowerCase() ?? null };
    case "upper":
      return { ...parts, extension: parts.extension?.toUpperCase() ?? null };
    case "remove":
      return { ...parts, extension: null };
    case "replace": {
      const extension = cleanExtension(rule.replacement);
      return extension === "" ? parts : { ...parts, extension };
    }
  }
}

/** What is being renamed, and the folder around it. */
export interface RenameSelection {
  /** The items to rename, in the order the list shows them. */
  items: readonly RenameItem[];
  /** The name of the folder the items are in, for `{parent}`. */
  parent: string;
  /** The names in that folder of everything that is not being renamed. */
  others: readonly string[];
}

export interface NewNames {
  names: string[];
  /** The items a `{date}` or `{time}` token has no time for. */
  undated: ReadonlySet<number>;
  findError: string | null;
}

/**
 * The name each item gets. The rules apply in a fixed order: find and replace, the template,
 * the counter, letter case, the extension, then trimming and tidying.
 */
export function newNames(selection: RenameSelection, rules: RenameRules): NewNames {
  const { items, parent } = selection;
  const { find, error } = compileFind(rules.find);
  const scope = rules.find.scope;
  const useTemplate = templateIsActive(rules.template);
  const templateSetsExtension = templateUses(rules.template, "ext");
  const needsTime = templateUses(rules.template, "date") || templateUses(rules.template, "time");
  const counts =
    (useTemplate && templateUses(rules.template, "n")) || (!useTemplate && rules.counter.enabled);
  const counters = counts ? counterValues(items, rules.counter) : null;
  const undated = new Set<number>();

  const names = items.map((item, index) => {
    let parts = splitName(item.name, item.isFolder);
    if (find) {
      if (scope === "both") {
        parts = splitName(findAndReplace(item.name, find), item.isFolder);
      } else if (scope === "name") {
        parts = { ...parts, stem: findAndReplace(parts.stem, find) };
      } else if (parts.extension !== null) {
        parts = { ...parts, extension: findAndReplace(parts.extension, find) };
      }
    }
    if (useTemplate) {
      if (needsTime && item.modified === null) undated.add(index);
      const filled = fillTemplate(rules.template.pattern, {
        name: parts.stem,
        ext: parts.extension ?? "",
        n: counters?.[index] ?? "",
        date:
          item.modified === null ? "" : formatDateToken(item.modified, rules.template.dateStyle),
        time:
          item.modified === null ? "" : formatTimeToken(item.modified, rules.template.timeStyle),
        parent,
      });
      parts = templateSetsExtension ? splitName(filled, item.isFolder) : { ...parts, stem: filled };
    } else if (counters) {
      const { placement, separator } = rules.counter;
      parts = {
        ...parts,
        stem:
          placement === "prefix"
            ? `${counters[index]}${separator}${parts.stem}`
            : `${parts.stem}${separator}${counters[index]}`,
      };
    }
    parts = { ...parts, stem: applyLetterCase(parts.stem, rules.letterCase) };
    if (!item.isFolder) parts = changeExtension(parts, rules.extension);
    if (rules.tidy.enabled) {
      parts = {
        stem: tidy(parts.stem, rules.tidy),
        extension: parts.extension === null ? null : tidy(parts.extension, rules.tidy),
      };
    }
    return joinName(parts);
  });
  return { names, undated, findError: error };
}

/** What a folder accepts as a name, and which names it holds to be the same. */
export interface NameTarget {
  /** Characters no name may hold there, besides control characters. */
  forbidden: string;
  /** Windows also refuses its device names, and names that end in a space or a dot. */
  windowsNames: boolean;
  /** Names that differ only in case are one name. */
  caseInsensitive: boolean;
  /** Names are stored in one Unicode form, so two spellings of one name are one name. */
  normalizes: boolean;
  maxLength: number;
  /** UTF-8 bytes, as most servers count, or UTF-16 units, as Windows does. */
  lengthUnit: "bytes" | "characters";
}

/**
 * A target as far as its kind tells; whether case and Unicode form matter may take a look at the
 * folder.
 */
export type TargetTraits = Omit<NameTarget, "caseInsensitive" | "normalizes"> & {
  /** Undefined where it depends on the file system behind the folder. */
  caseInsensitive: boolean | undefined;
  /** Undefined where it depends on the file system behind the folder, as on a Mac. */
  normalizes: boolean | undefined;
};

const WINDOWS_FORBIDDEN = '<>:"/\\|?*';
const PORTABLE_NAME_BYTES = 255;
/** S3 and Backblaze B2 limit the whole key, which is the path inside the bucket. */
const OBJECT_KEY_BYTES = 1024;
const B2_NAME_BYTES = 250;
const DRIVE_NAME_CHARACTERS = 32767;

function utf8Length(text: string): number {
  return new TextEncoder().encode(text).length;
}

/** The bytes left for a name in a bucket's folder: `/bucket/a/b` leaves the key `a/b/` less. */
function objectNameBytes(folder: string): number {
  const inBucket = folder.split("/").filter(Boolean).slice(1);
  const prefix = inBucket.length === 0 ? 0 : utf8Length(inBucket.join("/")) + 1;
  return Math.max(1, OBJECT_KEY_BYTES - prefix);
}

/**
 * What names a folder takes, from where it is: this computer, or a server of some protocol.
 * A server reached over SFTP, FTP or WebDAV, and a folder on this computer that is not on
 * Windows, do not say whether case matters, or whether composed and decomposed accents are one
 * name (as on a Mac); `caseProbe` and `formProbe` find that out from the folder itself.
 */
export function targetTraits(place: {
  pathStyle: PathStyle;
  protocol?: Protocol;
  /** The folder's path, which counts against the name on object storage. */
  folder: string;
}): TargetTraits {
  const posix = {
    forbidden: "/",
    windowsNames: false,
    normalizes: false,
    maxLength: PORTABLE_NAME_BYTES,
    lengthUnit: "bytes" as const,
  };
  const windows = {
    forbidden: WINDOWS_FORBIDDEN,
    windowsNames: true,
    normalizes: false,
    maxLength: PORTABLE_NAME_BYTES,
    lengthUnit: "characters" as const,
  };
  switch (place.protocol) {
    case undefined:
      return place.pathStyle === "windows"
        ? { ...windows, caseInsensitive: true }
        : { ...posix, caseInsensitive: undefined, normalizes: undefined };
    case "sftp":
    case "ftp":
    case "ftps":
    case "ftpsImplicit":
    case "webDav":
    case "webDavs":
      return { ...posix, caseInsensitive: undefined, normalizes: undefined };
    case "s3":
      return { ...posix, maxLength: objectNameBytes(place.folder), caseInsensitive: false };
    case "backblazeB2":
      return {
        ...posix,
        maxLength: Math.min(B2_NAME_BYTES, objectNameBytes(place.folder)),
        caseInsensitive: false,
      };
    // Drive looks names up without regard to case, so names that differ only in case count as
    // one name here, as on a case-insensitive disk.
    case "googleDrive":
      return {
        ...posix,
        maxLength: DRIVE_NAME_CHARACTERS,
        lengthUnit: "characters",
        caseInsensitive: true,
      };
    case "oneDrive":
      return { ...windows, caseInsensitive: true };
    case "box":
      return {
        ...posix,
        forbidden: "/\\",
        lengthUnit: "characters",
        caseInsensitive: true,
      };
    // A vault encrypts names, so case always matters, and stores them in one Unicode form.
    case "cryptomator":
      return { ...posix, normalizes: true, caseInsensitive: false };
  }
}

/** Names looked at for one with letters whose swapped spelling the folder does not list. */
const CASE_PROBE_CANDIDATES = 200;

/**
 * A name the folder holds and the same name with the case of its letters swapped, which the
 * folder does not list. If the folder answers for the second name too, case does not matter
 * there. Null when no listed name has letters to swap.
 */
export function caseProbe(names: readonly string[]): { listed: string; swapped: string } | null {
  const held = new Set(names);
  for (const name of names.slice(0, CASE_PROBE_CANDIDATES)) {
    const swapped = name.replace(/[a-zA-Z]/g, (letter) =>
      letter === letter.toLowerCase() ? letter.toUpperCase() : letter.toLowerCase(),
    );
    if (swapped !== name && !held.has(swapped)) return { listed: name, swapped };
  }
  return null;
}

/**
 * A name the folder holds and the same name in the other Unicode form, composed or decomposed,
 * which the folder does not list. If the folder answers for the second name too, it holds the
 * two forms to be one name. Null when no listed name has accents that can be written two ways.
 */
export function formProbe(names: readonly string[]): { listed: string; otherForm: string } | null {
  const held = new Set(names);
  for (const name of names) {
    const composed = name.normalize("NFC");
    const otherForm = composed === name ? name.normalize("NFD") : composed;
    if (otherForm !== name && !held.has(otherForm)) return { listed: name, otherForm };
  }
  return null;
}

/** One spelling for every name the target holds to be the same name. */
export function nameKey(
  name: string,
  target: Pick<NameTarget, "caseInsensitive" | "normalizes">,
): string {
  const normalized = target.normalizes ? name.normalize("NFC") : name;
  return target.caseInsensitive ? normalized.toLowerCase() : normalized;
}

const FIRST_PRINTABLE = 0x20;
const DELETE_CHARACTER = 0x7f;

/** The characters before the space, and DEL: none belongs in a name made here. */
function hasControlCharacter(name: string): boolean {
  for (let index = 0; index < name.length; index += 1) {
    const unit = name.charCodeAt(index);
    if (unit < FIRST_PRINTABLE || unit === DELETE_CHARACTER) return true;
  }
  return false;
}

const WINDOWS_DEVICE = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i;

export type ProblemKind =
  "sharedName" | "empty" | "invalid" | "tooLong" | "undated" | "duplicate" | "taken";
export const PROBLEM_KINDS: ProblemKind[] = [
  "sharedName",
  "empty",
  "invalid",
  "tooLong",
  "undated",
  "duplicate",
  "taken",
];

export interface NameProblem {
  problem: ProblemKind;
  message: string;
}

/** How long the target counts `name` to be, when that is over its limit. */
function excessLength(name: string, target: NameTarget): number | null {
  if (target.lengthUnit === "characters") {
    return name.length > target.maxLength ? name.length : null;
  }
  // No character takes more than three bytes for each UTF-16 unit, so short names need no count.
  if (name.length * 3 <= target.maxLength) return null;
  const bytes = utf8Length(name);
  return bytes > target.maxLength ? bytes : null;
}

/**
 * Why the target refuses `name`, or null when it takes it. Mirrors `validate_name` in
 * src-tauri/src/local.rs and src-tauri/src/protocol.rs, which the rename itself runs every name
 * through, and adds what only the target's file system would say.
 */
export function nameProblem(name: string, target: NameTarget): NameProblem | null {
  const invalid = (message: string): NameProblem => ({ problem: "invalid", message });
  if (name === "") return { problem: "empty", message: t("rename.theNameIsEmpty") };
  if (name === "." || name === "..") return invalid(t("rename.isNotAName", { name }));
  if (hasControlCharacter(name)) return invalid(t("rename.namesCannotContainControlCharacters"));
  for (const character of target.forbidden) {
    if (name.includes(character)) return invalid(t("rename.namesCannotContain", { character }));
  }
  if (target.windowsNames) {
    if (/[ .]$/.test(name)) return invalid(t("rename.namesCannotEndWithASpaceOrADot"));
    if (WINDOWS_DEVICE.test(name)) {
      return invalid(
        t("rename.isAReservedNameOnWindows", { name: name.split(".")[0].toUpperCase() }),
      );
    }
  }
  const length = excessLength(name, target);
  if (length !== null) {
    return {
      problem: "tooLong",
      message: t(
        target.lengthUnit === "bytes"
          ? "rename.theNameIsTooLongInBytes"
          : "rename.theNameIsTooLongInCharacters",
        { length, count: target.maxLength },
      ),
    };
  }
  return null;
}

export interface PreviewRow {
  /** The name the item has. */
  name: string;
  /** The name the rules give it. */
  newName: string;
  /** `ready` rows are renamed; `unchanged` rows keep their name and are skipped. */
  status: "unchanged" | "ready" | "problem";
  problem?: ProblemKind;
  /** What is wrong, for the row. */
  message?: string;
}

export interface RenamePreview {
  rows: PreviewRow[];
  /** Rows that will be renamed. */
  ready: number;
  unchanged: number;
  /** How many rows have each kind of problem. */
  problems: Partial<Record<ProblemKind, number>>;
  problemCount: number;
  findError: string | null;
}

/** How two names the target holds to be one name differ: not at all, in case, or in accents. */
function spellingOf(first: string, second: string): "same" | "case" | "accents" {
  if (first === second) return "same";
  return first.toLowerCase() === second.toLowerCase() ? "case" : "accents";
}

const KEEPS_NAME = {
  same: "rename.keepsThisName",
  case: "rename.keepsANameDifferingInCase",
  accents: "rename.keepsANameDifferingInAccents",
} as const;

const GETS_NAME = {
  same: "rename.getsTheSameName",
  case: "rename.getsANameDifferingInCase",
  accents: "rename.getsANameDifferingInAccents",
} as const;

/**
 * Every item with its new name and whether it can have it. A name is refused when the target
 * does not take it, when two items would end with it, or when an item that is not being renamed
 * has it. Items that keep their name hold on to it. Renames go by name, so an item that has the
 * same name as another selected item (which Google Drive and S3 allow) cannot be renamed with it.
 */
export function previewRenames(
  selection: RenameSelection,
  rules: RenameRules,
  target: NameTarget,
): RenamePreview {
  const { names, undated, findError } = newNames(selection, rules);
  const rows: PreviewRow[] = selection.items.map((item, index) => ({
    name: item.name,
    newName: names[index],
    status: names[index] === item.name ? "unchanged" : "ready",
  }));
  const flag = (row: PreviewRow, problem: ProblemKind, message: string) => {
    row.status = "problem";
    row.problem = problem;
    row.message = message;
  };

  const holders = new Map<string, number[]>();
  rows.forEach((row, index) => {
    const key = nameKey(row.newName, target);
    const sharing = holders.get(key);
    if (sharing) sharing.push(index);
    else holders.set(key, [index]);
  });
  const others = new Map(selection.others.map((name) => [nameKey(name, target), name]));
  const sources = new Map<string, number[]>();
  rows.forEach((row, index) => {
    const key = nameKey(row.name, target);
    const sharing = sources.get(key);
    if (sharing) sharing.push(index);
    else sources.set(key, [index]);
  });

  rows.forEach((row, index) => {
    if (row.status !== "ready") return;
    const twin = sources.get(nameKey(row.name, target))?.find((holder) => holder !== index);
    if (twin !== undefined) {
      const other = rows[twin].name;
      flag(
        row,
        "sharedName",
        other === row.name
          ? t("rename.anotherSelectedItemHasTheSameName")
          : t("rename.isSelectedTooAndCountsAsTheSameName", { name: other }),
      );
      return;
    }
    const refusal = nameProblem(row.newName, target);
    if (refusal) {
      flag(row, refusal.problem, refusal.message);
      return;
    }
    if (undated.has(index)) {
      flag(row, "undated", t("rename.noModifiedTimeFor", { date: "{date}", time: "{time}" }));
      return;
    }
    const key = nameKey(row.newName, target);
    const sharing = holders.get(key)?.find((holder) => holder !== index);
    if (sharing !== undefined) {
      const other = rows[sharing];
      const spelling = spellingOf(other.newName, row.newName);
      flag(
        row,
        "duplicate",
        t(other.status === "unchanged" ? KEEPS_NAME[spelling] : GETS_NAME[spelling], {
          name: other.name,
        }),
      );
      return;
    }
    const outside = others.get(key);
    if (outside !== undefined) {
      const spelling = spellingOf(outside, row.newName);
      flag(
        row,
        "taken",
        spelling === "same"
          ? t("rename.anItemThatIsNotSelectedHasThisName")
          : t(
              spelling === "case"
                ? "rename.notSelectedAndHasANameDifferingInCase"
                : "rename.notSelectedAndHasANameDifferingInAccents",
              { name: outside },
            ),
      );
    }
  });

  const problems: Partial<Record<ProblemKind, number>> = {};
  let ready = 0;
  let unchanged = 0;
  for (const row of rows) {
    if (row.status === "ready") ready += 1;
    else if (row.status === "unchanged") unchanged += 1;
    else if (row.problem) problems[row.problem] = (problems[row.problem] ?? 0) + 1;
  }
  return {
    rows,
    ready,
    unchanged,
    problems,
    problemCount: rows.length - ready - unchanged,
    findError,
  };
}

const PROBLEM_NOTES = {
  sharedName: "rename.problemSharedName",
  empty: "rename.problemEmpty",
  invalid: "rename.problemInvalid",
  tooLong: "rename.problemTooLong",
  undated: "rename.problemUndated",
  duplicate: "rename.problemDuplicate",
  taken: "rename.problemTaken",
} as const satisfies Record<ProblemKind, string>;

/** What stands in the way of renaming, in one line, or null when nothing does. */
export function problemSummary(preview: RenamePreview): string | null {
  if (preview.findError) {
    return t("rename.theRegularExpressionIsNotValid", { error: preview.findError });
  }
  const notes = PROBLEM_KINDS.flatMap((kind) => {
    const count = preview.problems[kind];
    return count ? [t(PROBLEM_NOTES[kind], { count })] : [];
  });
  return notes.length > 0
    ? t("rename.problemsSentence", { problems: notes.join(t("common.listSeparator")) })
    : null;
}

const STARTS_WITH_MARK = /^\p{M}/u;
const ZERO_WIDTH_JOINER = 0x200d;

/** Whether `text` can be cut before `index` without splitting a character from its marks. */
function isBoundary(text: string, index: number): boolean {
  if (index <= 0 || index >= text.length) return true;
  const unit = text.charCodeAt(index);
  const insidePair = unit >= 0xdc00 && unit <= 0xdfff;
  const joined = unit === ZERO_WIDTH_JOINER || text.charCodeAt(index - 1) === ZERO_WIDTH_JOINER;
  return !insidePair && !joined && !STARTS_WITH_MARK.test(text.slice(index, index + 2));
}

const DIGIT_ZERO = 0x30;
const DIGIT_NINE = 0x39;

/** Whether a cut before `index` would have a digit on both sides. */
function splitsNumber(text: string, index: number): boolean {
  const isDigit = (at: number) => {
    const unit = text.charCodeAt(at);
    return unit >= DIGIT_ZERO && unit <= DIGIT_NINE;
  };
  return index > 0 && index < text.length && isDigit(index - 1) && isDigit(index);
}

/**
 * The part of `after` that differs from `before`: what lies between the start and the end the
 * two names share, as `[start, end)`. Never cuts through an emoji or an accented letter, and
 * takes a number whole, so `09-30` after `0420` is not marked as `09-3`.
 */
export function changedRange(before: string, after: string): [number, number] {
  const shortest = Math.min(before.length, after.length);
  let start = 0;
  while (start < shortest && before[start] === after[start]) start += 1;
  while (start > 0 && !(isBoundary(after, start) && isBoundary(before, start))) start -= 1;
  let shared = 0;
  while (
    shared < shortest - start &&
    before[before.length - 1 - shared] === after[after.length - 1 - shared]
  ) {
    shared += 1;
  }
  while (
    shared > 0 &&
    !(isBoundary(after, after.length - shared) && isBoundary(before, before.length - shared))
  ) {
    shared -= 1;
  }
  let end = after.length - shared;
  while (splitsNumber(after, start)) start -= 1;
  while (splitsNumber(after, end)) end += 1;
  return [start, end];
}

export type { PlannedRename, RenameGroup };

/** The single renames a group amounts to, in the order they run. */
export function stepsOf(group: RenameGroup): PlannedRename[] {
  const [first, ...rest] = group.renames;
  if (group.temporary === undefined || first === undefined) return group.renames;
  return [
    { from: first.from, to: group.temporary },
    ...rest,
    { from: group.temporary, to: first.to },
  ];
}

/** The renames a preview asks for: its rows that change and have nothing wrong. */
export function renamesOf(preview: RenamePreview): PlannedRename[] {
  return preview.rows.flatMap((row) =>
    row.status === "ready" ? [{ from: row.name, to: row.newName }] : [],
  );
}

/**
 * Puts renames within one folder in an order that is always safe. A rename whose new name
 * another item still has waits for that item to move first, so a chain (a to b while b to c)
 * runs from its free end. A ring (a and b trading names, or a to b to c to a) is opened with a
 * temporary name, and so is a change of case alone where case does not matter, since the old
 * name would stand in the way of the new one. `taken` is every name the folder holds, which no
 * temporary name may be. No two renames may share a source or a target.
 */
export function planRenames(
  renames: readonly PlannedRename[],
  taken: Iterable<string>,
  target: Pick<NameTarget, "caseInsensitive" | "normalizes">,
  temporaryName: (attempt: number) => string,
): RenameGroup[] {
  const key = (name: string) => nameKey(name, target);
  const bySource = new Map<string, number>();
  const targets = new Set<string>();
  renames.forEach((rename, index) => {
    const source = key(rename.from);
    const destination = key(rename.to);
    if (bySource.has(source)) throw new Error(`${rename.from} is renamed twice`);
    if (targets.has(destination)) throw new Error(`Two items would be named ${rename.to}`);
    bySource.set(source, index);
    targets.add(destination);
  });

  const unavailable = new Set(targets);
  for (const name of taken) unavailable.add(key(name));
  for (const rename of renames) unavailable.add(key(rename.from));
  let attempt = 0;
  const freeTemporaryName = () => {
    for (;;) {
      attempt += 1;
      const name = temporaryName(attempt);
      const nameAsKey = key(name);
      if (unavailable.has(nameAsKey)) continue;
      unavailable.add(nameAsKey);
      return name;
    }
  };

  const groups: RenameGroup[] = [];
  const planned = new Array<boolean>(renames.length).fill(false);
  renames.forEach((_, start) => {
    if (planned[start]) return;
    // Each rename waits for the one whose item has the name it wants. Sources and targets are
    // unique, so following that from a rename either ends or comes back to where it started.
    const waiting: number[] = [];
    let current: number | undefined = start;
    let ring = false;
    while (current !== undefined && !planned[current]) {
      waiting.push(current);
      planned[current] = true;
      const blocker = bySource.get(key(renames[current].to));
      if (blocker === start) {
        ring = true;
        break;
      }
      current = blocker;
    }
    if (ring) {
      const [first, ...rest] = waiting;
      groups.push({
        temporary: freeTemporaryName(),
        renames: [renames[first], ...rest.reverse().map((index) => renames[index])],
      });
    } else {
      for (const index of waiting.reverse()) groups.push({ renames: [renames[index]] });
    }
  });
  return groups;
}

export type OutcomeStatus = "renamed" | "failed" | "stranded" | "left";

/** What became of one rename a run was asked for. */
export interface OutcomeRow extends PlannedRename {
  status: OutcomeStatus;
  /** Why it failed, or what name the item was left under. */
  message?: string;
}

/**
 * What became of each rename of a run that stopped early, in the order asked: it has its new
 * name, it is the one that failed, it is left under a temporary name, or the run never got to
 * it (or put it back).
 */
export function renameOutcomes(
  asked: readonly PlannedRename[],
  summary: RenameSummary,
): OutcomeRow[] {
  const renamed = new Set(summary.renamed.map((rename) => rename.from));
  const { failure, stranded } = summary;
  return asked.map((rename): OutcomeRow => {
    if (renamed.has(rename.from)) return { ...rename, status: "renamed" };
    if (stranded?.from === rename.from) {
      return {
        ...rename,
        status: "stranded",
        message: t("rename.nowNamed", { temporary: stranded.temporary, message: stranded.message }),
      };
    }
    if (failure?.from === rename.from) {
      return { ...rename, status: "failed", message: failure.message };
    }
    return { ...rename, status: "left" };
  });
}

/** `/srv/photos` is `photos`; a root, or a drive, has no name. */
export function folderName(folder: string): string {
  const parts = folder.split(/[\\/]/).filter(Boolean);
  const last = parts[parts.length - 1] ?? "";
  return parts.length === 1 && /^[A-Za-z]:$/.test(last) ? "" : last;
}

/** A find and replace the user ran, offered again later. */
export type FindHistoryEntry = Omit<FindRule, "enabled">;

export const FIND_HISTORY_LIMIT = 12;
const HISTORY_TEXT_LIMIT = 500;

function sameFind(first: FindHistoryEntry, second: FindHistoryEntry): boolean {
  return (
    first.mode === second.mode &&
    first.pattern === second.pattern &&
    first.replacement === second.replacement
  );
}

/** The history with `rule` first, once, and no longer than the limit. */
export function rememberFind(
  history: readonly FindHistoryEntry[],
  rule: FindRule,
): FindHistoryEntry[] {
  if (!rule.enabled || rule.pattern === "" || rule.pattern.length > HISTORY_TEXT_LIMIT) {
    return [...history];
  }
  const { mode, pattern, replacement, caseSensitive, allMatches, scope } = rule;
  const entry: FindHistoryEntry = { mode, pattern, replacement, caseSensitive, allMatches, scope };
  return [entry, ...history.filter((known) => !sameFind(known, entry))].slice(
    0,
    FIND_HISTORY_LIMIT,
  );
}

/** Keeps the stored entries that are whole, so settings edited by hand still load. */
export function sanitizeFindHistory(stored: unknown): FindHistoryEntry[] {
  if (!Array.isArray(stored)) return [];
  const kept: FindHistoryEntry[] = [];
  for (const value of stored) {
    if (!value || typeof value !== "object") continue;
    const { mode, pattern, replacement, caseSensitive, allMatches, scope } = value as Record<
      string,
      unknown
    >;
    if (typeof pattern !== "string" || pattern === "" || pattern.length > HISTORY_TEXT_LIMIT) {
      continue;
    }
    const entry: FindHistoryEntry = {
      mode: FIND_MODES.includes(mode as FindMode) ? (mode as FindMode) : "text",
      pattern,
      replacement: typeof replacement === "string" ? replacement.slice(0, HISTORY_TEXT_LIMIT) : "",
      caseSensitive: caseSensitive === true,
      allMatches: allMatches !== false,
      scope: FIND_SCOPES.includes(scope as FindScope) ? (scope as FindScope) : "name",
    };
    if (!kept.some((known) => sameFind(known, entry))) kept.push(entry);
    if (kept.length === FIND_HISTORY_LIMIT) break;
  }
  return kept;
}

/** `IMG_(\d+) to photo-$1` for the list of recent patterns. */
export function findHistoryLabel(entry: FindHistoryEntry): string {
  const replacement = entry.replacement === "" ? "nothing" : entry.replacement;
  return `${entry.pattern} to ${replacement}${entry.mode === "regex" ? " (regex)" : ""}`;
}
