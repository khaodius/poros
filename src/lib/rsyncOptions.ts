// rsync's options for folder synchronization: their defaults, how stored ones are read back, and
// how the editor groups them, in the order rsync's manual describes them. The backend carries
// them out and reads typed rsync arguments (src-tauri/src/sync/options.rs).

import type {
  AppendMode,
  CompareMode,
  DeleteTiming,
  FilterFiles,
  LinkMode,
  SyncDirection,
  SyncOptions,
} from "./types";

export const TIME_TOLERANCE_LIMITS = { min: 0, max: 24 * 60 * 60 };
/** rsync's largest block for protocol 30 and later. */
export const MAX_BLOCK_SIZE = 128 * 1024;

export const DEFAULT_SYNC_OPTIONS: SyncOptions = {
  compare: "sizeAndTime",
  timeToleranceSecs: 2,
  skipNewerOnTarget: false,
  ignoreExisting: false,
  deleteExtraneous: false,
  excludes: [],
  existingOnly: false,
  minSize: null,
  maxSize: null,
  recursive: true,
  dirs: false,
  mkpath: false,
  pruneEmptyDirs: false,
  wholeFile: false,
  inplace: false,
  append: "off",
  blockSize: null,
  removeSourceFiles: false,
  fsync: false,
  rsyncPath: "",
  deleteTiming: "during",
  deleteExcluded: false,
  maxDelete: null,
  force: false,
  ignoreErrors: false,
  filterRules: [],
  includes: [],
  includeFrom: [],
  excludeFrom: [],
  filesFrom: "",
  from0: false,
  filterFiles: "off",
  cvsExclude: false,
  times: true,
  omitDirTimes: false,
  perms: false,
  executability: false,
  chmod: "",
  owner: false,
  group: false,
  chown: "",
  numericIds: false,
  links: "follow",
  safeLinks: false,
  copyUnsafeLinks: false,
  copyDirlinks: false,
  keepDirlinks: false,
  bwlimitKib: 0,
  compress: false,
  backup: false,
  backupDir: "",
  backupSuffix: "",
  dryRun: false,
  itemize: false,
  verbose: false,
  stats: false,
  logFile: "",
};

const CHOICES: { [K in keyof SyncOptions]?: readonly string[] } = {
  compare: ["sizeAndTime", "sizeOnly", "checksum", "always"] satisfies CompareMode[],
  append: ["off", "append", "verify"] satisfies AppendMode[],
  deleteTiming: ["before", "during", "delay", "after"] satisfies DeleteTiming[],
  filterFiles: ["off", "read", "readAndExclude"] satisfies FilterFiles[],
  links: ["follow", "preserve", "skip"] satisfies LinkMode[],
};

/** Options without a value of their own unless set. */
const OPTIONAL_NUMBERS = new Set<keyof SyncOptions>([
  "minSize",
  "maxSize",
  "blockSize",
  "maxDelete",
]);

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function finite(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? Math.round(value) : null;
}

/** Stored options with each value of the right kind kept and the rest at their defaults. */
export function sanitizeSyncOptions(value: unknown): SyncOptions {
  const stored = asRecord(value);
  const options: Record<string, unknown> = { ...DEFAULT_SYNC_OPTIONS };
  for (const [key, fallback] of Object.entries(DEFAULT_SYNC_OPTIONS)) {
    const given = stored[key];
    const name = key as keyof SyncOptions;
    const choices = CHOICES[name];
    if (choices) {
      if (typeof given === "string" && choices.includes(given)) options[key] = given;
    } else if (OPTIONAL_NUMBERS.has(name)) {
      const number = finite(given);
      options[key] = number !== null && number >= 0 ? number : null;
    } else if (Array.isArray(fallback)) {
      if (Array.isArray(given)) {
        options[key] = given.filter((item): item is string => typeof item === "string");
      }
    } else if (typeof fallback === "number") {
      const number = finite(given);
      if (number !== null) options[key] = Math.max(0, number);
    } else if (typeof given === typeof fallback) {
      options[key] = given;
    }
  }
  const result = options as unknown as SyncOptions;
  result.timeToleranceSecs = Math.min(result.timeToleranceSecs, TIME_TOLERANCE_LIMITS.max);
  return result;
}

/**
 * Options saved before the full set existed: the compare mode, three switches, the time
 * window, and excludes as one pattern per line.
 */
export function legacySyncOptions(stored: Record<string, unknown>): SyncOptions {
  const excludes = stored.excludes;
  return sanitizeSyncOptions({
    ...stored,
    excludes: typeof excludes === "string" ? linesOf(excludes) : excludes,
  });
}

/** The non-blank lines of a box with one item per line. */
export function linesOf(text: string): string[] {
  return text.split(/\r?\n/).filter((line) => line.trim() !== "");
}

const SIZE_UNITS = ["b", "k", "m", "g", "t", "p"];

/**
 * A size as rsync reads it for --min-size and --max-size: `K`, `M`, `G` count in 1024s (`KiB`
 * too), `KB` and the like in 1000s, and a bare number is in bytes. Blank is no limit;
 * `undefined` is a mistake.
 */
export function parseSize(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  const match = /^(\d+(?:\.\d+)?)\s*([bkmgtp]?)(i?b?)$/i.exec(trimmed);
  if (!match) return undefined;
  const [, number, unit, suffix] = match;
  const power = SIZE_UNITS.indexOf((unit || "b").toLowerCase());
  const lower = suffix.toLowerCase();
  if (lower === "i" || (unit.toLowerCase() === "b" && lower !== "")) return undefined;
  const base = lower === "b" ? 1000 : 1024;
  return Math.round(Number(number) * base ** power);
}

/** Bytes in the largest unit that keeps them whole, as `parseSize` reads them back. */
export function formatSizeArgument(bytes: number | null): string {
  if (bytes === null) return "";
  for (let power = SIZE_UNITS.length - 1; power > 0; power--) {
    const unit = 1024 ** power;
    if (bytes >= unit && bytes % unit === 0) {
      return `${bytes / unit}${SIZE_UNITS[power].toUpperCase()}`;
    }
  }
  return `${bytes}B`;
}

type KeysOf<V> = {
  [K in keyof SyncOptions]: SyncOptions[K] extends V ? K : never;
}[keyof SyncOptions];

export type OptionField =
  | { kind: "switch"; key: KeysOf<boolean>; label: string; flag: string; hint?: string }
  | {
      kind: "select";
      key: keyof typeof CHOICES & KeysOf<string>;
      label: string;
      flag?: string;
      choices: { value: string; label: string }[];
      hint?: string;
    }
  | {
      kind: "number";
      key: KeysOf<number>;
      label: string;
      flag: string;
      unit?: string;
      max: number;
      hint?: string;
    }
  | {
      kind: "optionalNumber";
      key: "blockSize" | "maxDelete";
      label: string;
      flag: string;
      unit?: string;
      max: number;
      hint?: string;
    }
  | { kind: "size"; key: "minSize" | "maxSize"; label: string; flag: string; hint?: string }
  | {
      kind: "text";
      key: KeysOf<string>;
      label: string;
      flag: string;
      placeholder?: string;
      hint?: string;
    }
  | {
      kind: "list";
      key: KeysOf<string[]>;
      label: string;
      flag: string;
      placeholder?: string;
      hint?: string;
    };

export interface OptionGroup {
  id: string;
  title: string;
  fields: OptionField[];
}

export const COMPARE_CHOICES: { value: CompareMode; label: string }[] = [
  { value: "sizeAndTime", label: "Size and modification time" },
  { value: "sizeOnly", label: "Size only (--size-only)" },
  { value: "checksum", label: "Contents (-c, reads files of equal size)" },
  { value: "always", label: "Copy every file (-I)" },
];

export const OPTION_GROUPS: OptionGroup[] = [
  {
    id: "compare",
    title: "Which files are copied",
    fields: [
      {
        kind: "select",
        key: "compare",
        label: "Compare files by",
        choices: COMPARE_CHOICES,
      },
      {
        kind: "number",
        key: "timeToleranceSecs",
        label: "Times this close count as equal",
        flag: "--modify-window",
        unit: "s",
        max: TIME_TOLERANCE_LIMITS.max,
        hint: "Some file systems, such as FAT, keep times to 2 seconds.",
      },
      {
        kind: "switch",
        key: "skipNewerOnTarget",
        label: "Skip files that are newer on the target",
        flag: "-u",
      },
      {
        kind: "switch",
        key: "ignoreExisting",
        label: "Skip files that already exist on the target",
        flag: "--ignore-existing",
      },
      {
        kind: "switch",
        key: "existingOnly",
        label: "Only update files the target has; create nothing new",
        flag: "--existing",
      },
      { kind: "size", key: "minSize", label: "Skip files smaller than", flag: "--min-size" },
      {
        kind: "size",
        key: "maxSize",
        label: "Skip files larger than",
        flag: "--max-size",
        hint: "Sizes such as 500K, 10M or 2G; a bare number is in bytes.",
      },
    ],
  },
  {
    id: "transfer",
    title: "Transfer behaviour",
    fields: [
      { kind: "switch", key: "recursive", label: "Go into folders", flag: "-r" },
      {
        kind: "switch",
        key: "dirs",
        label: "Without going into folders, still create the top ones",
        flag: "-d",
      },
      {
        kind: "switch",
        key: "mkpath",
        label: "Create the whole path to the target folder",
        flag: "--mkpath",
        hint: "Without it, only the target folder itself is created when it is missing.",
      },
      {
        kind: "switch",
        key: "pruneEmptyDirs",
        label: "Leave out folders that would end up empty",
        flag: "-m",
      },
      {
        kind: "switch",
        key: "wholeFile",
        label: "Copy whole files, without rsync's delta transfer",
        flag: "-W",
      },
      {
        kind: "switch",
        key: "inplace",
        label: "Write into the existing file instead of a temporary one",
        flag: "--inplace",
      },
      {
        kind: "select",
        key: "append",
        label: "Shorter files on the target",
        choices: [
          { value: "off", label: "Copy them again as usual" },
          { value: "append", label: "Send only the rest (--append)" },
          { value: "verify", label: "Send the rest, then check the whole file (--append-verify)" },
        ],
      },
      {
        kind: "optionalNumber",
        key: "blockSize",
        label: "Delta block size",
        flag: "-B",
        unit: "bytes",
        max: MAX_BLOCK_SIZE,
        hint: "Blank picks one from each file's size.",
      },
      {
        kind: "switch",
        key: "removeSourceFiles",
        label: "Remove source files once they are on the target",
        flag: "--remove-source-files",
        hint: "Folders stay. Files already the same on both sides are removed too.",
      },
      { kind: "switch", key: "fsync", label: "Flush each file to disk", flag: "--fsync" },
      {
        kind: "text",
        key: "rsyncPath",
        label: "rsync on the server",
        flag: "--rsync-path",
        placeholder: "rsync",
        hint: "Blank uses the site's setting, then the transfer settings.",
      },
    ],
  },
  {
    id: "delete",
    title: "Deletion",
    fields: [
      {
        kind: "switch",
        key: "deleteExtraneous",
        label: "Delete files the source does not have",
        flag: "--delete",
      },
      {
        kind: "select",
        key: "deleteTiming",
        label: "When to delete",
        choices: [
          { value: "before", label: "Before copying (--delete-before)" },
          { value: "during", label: "While copying (--delete-during)" },
          { value: "delay", label: "Found while copying, deleted after (--delete-delay)" },
          { value: "after", label: "After copying (--delete-after)" },
        ],
      },
      {
        kind: "switch",
        key: "deleteExcluded",
        label: "Delete excluded files from the target too",
        flag: "--delete-excluded",
      },
      {
        kind: "optionalNumber",
        key: "maxDelete",
        label: "Delete at most",
        flag: "--max-delete",
        unit: "items",
        max: 1_000_000_000,
        hint: "Blank is no limit; 0 deletes nothing and reports what would go.",
      },
      {
        kind: "switch",
        key: "force",
        label: "Replace folders that are not empty with files",
        flag: "--force",
      },
      {
        kind: "switch",
        key: "ignoreErrors",
        label: "Delete even when part of the source could not be read",
        flag: "--ignore-errors",
      },
    ],
  },
  {
    id: "filter",
    title: "Filter rules",
    fields: [
      {
        kind: "list",
        key: "excludes",
        label: "Exclude",
        flag: "--exclude",
        placeholder: "*.tmp\nnode_modules/\n/build",
        hint: "One pattern per line, as in rsync: * and ? match within a name, ** across folders, a trailing / matches only folders and a leading / anchors to the top folder.",
      },
      {
        kind: "list",
        key: "includes",
        label: "Include",
        flag: "--include",
        placeholder: "*/\n*.html",
        hint: "Checked before the excludes, so these are kept even when an exclude matches.",
      },
      {
        kind: "list",
        key: "filterRules",
        label: "Filter rules",
        flag: "--filter",
        placeholder: "- *.log\nP /uploads/\ndir-merge .rsync-filter",
        hint: "rsync's filter syntax, one rule per line, checked before includes and excludes.",
      },
      {
        kind: "list",
        key: "excludeFrom",
        label: "Exclude patterns from files",
        flag: "--exclude-from",
        placeholder: "excludes.txt",
        hint: "Files on this computer, one per line; relative paths start at the local folder.",
      },
      {
        kind: "list",
        key: "includeFrom",
        label: "Include patterns from files",
        flag: "--include-from",
      },
      {
        kind: "text",
        key: "filesFrom",
        label: "Copy only the files listed in",
        flag: "--files-from",
        placeholder: "files.txt",
      },
      {
        kind: "switch",
        key: "from0",
        label: "Names in those files end with a null character",
        flag: "-0",
      },
      {
        kind: "select",
        key: "filterFiles",
        label: "Rules in .rsync-filter files",
        choices: [
          { value: "off", label: "Ignore them" },
          { value: "read", label: "Read them in each folder (-F)" },
          { value: "readAndExclude", label: "Read them and leave them out (-FF)" },
        ],
      },
      {
        kind: "switch",
        key: "cvsExclude",
        label: "Leave out files CVS would ignore",
        flag: "-C",
      },
    ],
  },
  {
    id: "attributes",
    title: "Times, permissions and ownership",
    fields: [
      { kind: "switch", key: "times", label: "Keep modification times", flag: "-t" },
      {
        kind: "switch",
        key: "omitDirTimes",
        label: "Leave the times of folders alone",
        flag: "-O",
      },
      { kind: "switch", key: "perms", label: "Keep permissions", flag: "-p" },
      {
        kind: "switch",
        key: "executability",
        label: "Without permissions, keep whether files can run",
        flag: "-E",
      },
      {
        kind: "text",
        key: "chmod",
        label: "Change permissions",
        flag: "--chmod",
        placeholder: "D755,F644",
        hint: "As rsync takes them: D for folders, F for files, then octal or chmod's u+x form.",
      },
      { kind: "switch", key: "owner", label: "Keep the owner", flag: "-o" },
      { kind: "switch", key: "group", label: "Keep the group", flag: "-g" },
      {
        kind: "text",
        key: "chown",
        label: "Set the owner and group",
        flag: "--chown",
        placeholder: "www-data:www-data",
      },
      {
        kind: "switch",
        key: "numericIds",
        label: "Match owners and groups by number, not name",
        flag: "--numeric-ids",
      },
    ],
  },
  {
    id: "links",
    title: "Symbolic links",
    fields: [
      {
        kind: "select",
        key: "links",
        label: "Links",
        choices: [
          { value: "follow", label: "Copy what they point to (-L)" },
          { value: "preserve", label: "Copy them as links (-l)" },
          { value: "skip", label: "Leave them out" },
        ],
      },
      {
        kind: "switch",
        key: "safeLinks",
        label: "Leave out links that point outside the folder",
        flag: "--safe-links",
      },
      {
        kind: "switch",
        key: "copyUnsafeLinks",
        label: "Copy what links outside the folder point to",
        flag: "--copy-unsafe-links",
      },
      {
        kind: "switch",
        key: "copyDirlinks",
        label: "Copy links to folders as folders",
        flag: "-k",
      },
      {
        kind: "switch",
        key: "keepDirlinks",
        label: "Treat links to folders on the target as folders",
        flag: "-K",
      },
    ],
  },
  {
    id: "bandwidth",
    title: "Bandwidth and compression",
    fields: [
      {
        kind: "number",
        key: "bwlimitKib",
        label: "Speed limit",
        flag: "--bwlimit",
        unit: "KiB/s",
        max: 100 * 1024 * 1024,
        hint: "0 is unlimited. Shared by every file of the synchronization.",
      },
      {
        kind: "switch",
        key: "compress",
        label: "Compress data on the way",
        flag: "-z",
        hint: "Over a connection of its own with SSH compression.",
      },
    ],
  },
  {
    id: "backup",
    title: "Backups",
    fields: [
      {
        kind: "switch",
        key: "backup",
        label: "Keep files that are replaced or deleted",
        flag: "-b",
      },
      {
        kind: "text",
        key: "backupDir",
        label: "Backup folder",
        flag: "--backup-dir",
        placeholder: "Beside each file",
        hint: "On the target's side; a relative path starts at the target folder.",
      },
      {
        kind: "text",
        key: "backupSuffix",
        label: "Added to backup names",
        flag: "--suffix",
        placeholder: "~ without a backup folder",
      },
    ],
  },
  {
    id: "output",
    title: "Dry run and reporting",
    fields: [
      {
        kind: "switch",
        key: "dryRun",
        label: "Dry run: show what would happen and change nothing",
        flag: "-n",
      },
      {
        kind: "switch",
        key: "itemize",
        label: "Log each change in rsync's itemized form",
        flag: "-i",
      },
      { kind: "switch", key: "verbose", label: "Log each file copied", flag: "-v" },
      { kind: "switch", key: "stats", label: "Log statistics at the end", flag: "--stats" },
      {
        kind: "text",
        key: "logFile",
        label: "Also write to the log file",
        flag: "--log-file",
        placeholder: "/home/me/sync.log",
      },
    ],
  },
];

/** Options a two-way synchronization cannot use. */
export function unavailable(key: keyof SyncOptions, direction: SyncDirection): boolean {
  if (direction !== "both") return false;
  return (
    key === "deleteExtraneous" ||
    key === "deleteTiming" ||
    key === "deleteExcluded" ||
    key === "maxDelete" ||
    key === "skipNewerOnTarget" ||
    key === "removeSourceFiles"
  );
}

/** How many of a group's options differ from rsync's defaults. */
export function changedCount(group: OptionGroup, options: SyncOptions): number {
  return group.fields.filter(
    (field) =>
      JSON.stringify(options[field.key]) !== JSON.stringify(DEFAULT_SYNC_OPTIONS[field.key]),
  ).length;
}

/** The option a field sets, changed and with the rule two-way synchronizations need. */
export function withOption<K extends keyof SyncOptions>(
  options: SyncOptions,
  key: K,
  value: SyncOptions[K],
  direction: SyncDirection,
): SyncOptions {
  const next = { ...options, [key]: value };
  // Both ways, every file is compared by time to find the newer one.
  if (direction === "both" && next.compare === "always") next.compare = "sizeAndTime";
  return next;
}

/** How the command line reads, with the folders in the order rsync takes them. */
export function commandPreview(
  args: string,
  direction: SyncDirection,
  localPath: string,
  remote: string,
): string {
  const quote = (path: string) =>
    /[\s'"\\$`]/.test(path) ? `'${path.replace(/'/g, "'\\''")}'` : path;
  const local = quote(withSlash(localPath || "LOCAL"));
  const server = quote(withSlash(remote || "SERVER:PATH"));
  const [source, target] = direction === "download" ? [server, local] : [local, server];
  return ["rsync", args.trim(), source, target].filter(Boolean).join(" ");
}

/** The trailing separator that has rsync copy a folder's contents rather than the folder. */
function withSlash(path: string): string {
  if (/[\\/]$/.test(path)) return path;
  return /\\/.test(path) && !path.includes("/") ? `${path}\\` : `${path}/`;
}
