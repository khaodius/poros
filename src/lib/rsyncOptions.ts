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
import { t } from "../i18n";

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
      /** The unit after the number, which may agree with it. */
      unit?: (count: number) => string;
      max: number;
      hint?: string;
    }
  | {
      kind: "optionalNumber";
      key: "blockSize" | "maxDelete";
      label: string;
      flag: string;
      /** The unit after the number, which may agree with it. */
      unit?: (count: number) => string;
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
  {
    value: "sizeAndTime",
    get label() {
      return t("rsync.sizeAndModificationTime");
    },
  },
  {
    value: "sizeOnly",
    get label() {
      return t("rsync.sizeOnlySizeOnly");
    },
  },
  {
    value: "checksum",
    get label() {
      return t("rsync.contentsCReadsFilesOf");
    },
  },
  {
    value: "always",
    get label() {
      return t("rsync.copyEveryFileI");
    },
  },
];

export const OPTION_GROUPS: OptionGroup[] = [
  {
    id: "compare",
    get title() {
      return t("rsync.whichFilesAreCopied");
    },
    fields: [
      {
        kind: "select",
        key: "compare",
        get label() {
          return t("rsync.compareFilesBy");
        },
        choices: COMPARE_CHOICES,
      },
      {
        kind: "number",
        key: "timeToleranceSecs",
        get label() {
          return t("rsync.timesThisCloseCountAs");
        },
        flag: "--modify-window",
        unit: (count) => t("rsync.unitSeconds", { count }),
        max: TIME_TOLERANCE_LIMITS.max,
        get hint() {
          return t("rsync.someFileSystemsSuchAs");
        },
      },
      {
        kind: "switch",
        key: "skipNewerOnTarget",
        get label() {
          return t("rsync.skipFilesThatAreNewer");
        },
        flag: "-u",
      },
      {
        kind: "switch",
        key: "ignoreExisting",
        get label() {
          return t("rsync.skipFilesThatAlreadyExist");
        },
        flag: "--ignore-existing",
      },
      {
        kind: "switch",
        key: "existingOnly",
        get label() {
          return t("rsync.onlyUpdateFilesTheTarget");
        },
        flag: "--existing",
      },
      {
        kind: "size",
        key: "minSize",
        get label() {
          return t("rsync.skipFilesSmallerThan");
        },
        flag: "--min-size",
      },
      {
        kind: "size",
        key: "maxSize",
        get label() {
          return t("rsync.skipFilesLargerThan");
        },
        flag: "--max-size",
        get hint() {
          return t("rsync.sizesSuchAs500k10m");
        },
      },
    ],
  },
  {
    id: "transfer",
    get title() {
      return t("rsync.transferBehaviour");
    },
    fields: [
      {
        kind: "switch",
        key: "recursive",
        get label() {
          return t("rsync.goIntoFolders");
        },
        flag: "-r",
      },
      {
        kind: "switch",
        key: "dirs",
        get label() {
          return t("rsync.withoutGoingIntoFoldersStill");
        },
        flag: "-d",
      },
      {
        kind: "switch",
        key: "mkpath",
        get label() {
          return t("rsync.createTheWholePathTo");
        },
        flag: "--mkpath",
        get hint() {
          return t("rsync.withoutItOnlyTheTarget");
        },
      },
      {
        kind: "switch",
        key: "pruneEmptyDirs",
        get label() {
          return t("rsync.leaveOutFoldersThatWould");
        },
        flag: "-m",
      },
      {
        kind: "switch",
        key: "wholeFile",
        get label() {
          return t("rsync.copyWholeFilesWithoutRsyncs");
        },
        flag: "-W",
      },
      {
        kind: "switch",
        key: "inplace",
        get label() {
          return t("rsync.writeIntoTheExistingFile");
        },
        flag: "--inplace",
      },
      {
        kind: "select",
        key: "append",
        get label() {
          return t("rsync.shorterFilesOnTheTarget");
        },
        choices: [
          {
            value: "off",
            get label() {
              return t("rsync.copyThemAgainAsUsual");
            },
          },
          {
            value: "append",
            get label() {
              return t("rsync.sendOnlyTheRestAppend");
            },
          },
          {
            value: "verify",
            get label() {
              return t("rsync.sendTheRestThenCheck");
            },
          },
        ],
      },
      {
        kind: "optionalNumber",
        key: "blockSize",
        get label() {
          return t("rsync.deltaBlockSize");
        },
        flag: "-B",
        unit: (count) => t("rsync.unitBytes", { count }),
        max: MAX_BLOCK_SIZE,
        get hint() {
          return t("rsync.blankPicksOneFromEach");
        },
      },
      {
        kind: "switch",
        key: "removeSourceFiles",
        get label() {
          return t("rsync.removeSourceFilesOnceThey");
        },
        flag: "--remove-source-files",
        get hint() {
          return t("rsync.foldersStayFilesAlreadyThe");
        },
      },
      {
        kind: "switch",
        key: "fsync",
        get label() {
          return t("rsync.flushEachFileToDisk");
        },
        flag: "--fsync",
      },
      {
        kind: "text",
        key: "rsyncPath",
        get label() {
          return t("rsync.rsyncOnTheServer");
        },
        flag: "--rsync-path",
        placeholder: "rsync",
        get hint() {
          return t("rsync.blankUsesTheSitesSetting");
        },
      },
    ],
  },
  {
    id: "delete",
    get title() {
      return t("rsync.deletion");
    },
    fields: [
      {
        kind: "switch",
        key: "deleteExtraneous",
        get label() {
          return t("rsync.deleteFilesTheSourceDoes");
        },
        flag: "--delete",
      },
      {
        kind: "select",
        key: "deleteTiming",
        get label() {
          return t("rsync.whenToDelete");
        },
        choices: [
          {
            value: "before",
            get label() {
              return t("rsync.beforeCopyingDeleteBefore");
            },
          },
          {
            value: "during",
            get label() {
              return t("rsync.whileCopyingDeleteDuring");
            },
          },
          {
            value: "delay",
            get label() {
              return t("rsync.foundWhileCopyingDeletedAfter");
            },
          },
          {
            value: "after",
            get label() {
              return t("rsync.afterCopyingDeleteAfter");
            },
          },
        ],
      },
      {
        kind: "switch",
        key: "deleteExcluded",
        get label() {
          return t("rsync.deleteExcludedFilesFromThe");
        },
        flag: "--delete-excluded",
      },
      {
        kind: "optionalNumber",
        key: "maxDelete",
        get label() {
          return t("rsync.deleteAtMost");
        },
        flag: "--max-delete",
        unit: (count) => t("rsync.unitItems", { count }),
        max: 1_000_000_000,
        get hint() {
          return t("rsync.blankIsNoLimit0");
        },
      },
      {
        kind: "switch",
        key: "force",
        get label() {
          return t("rsync.replaceFoldersThatAreNot");
        },
        flag: "--force",
      },
      {
        kind: "switch",
        key: "ignoreErrors",
        get label() {
          return t("rsync.deleteEvenWhenPartOf");
        },
        flag: "--ignore-errors",
      },
    ],
  },
  {
    id: "filter",
    get title() {
      return t("rsync.filterRules");
    },
    fields: [
      {
        kind: "list",
        key: "excludes",
        get label() {
          return t("rsync.exclude");
        },
        flag: "--exclude",
        placeholder: "*.tmp\nnode_modules/\n/build",
        get hint() {
          return t("rsync.onePatternPerLineAs");
        },
      },
      {
        kind: "list",
        key: "includes",
        get label() {
          return t("rsync.include");
        },
        flag: "--include",
        placeholder: "*/\n*.html",
        get hint() {
          return t("rsync.checkedBeforeTheExcludeField");
        },
      },
      {
        kind: "list",
        key: "filterRules",
        get label() {
          return t("rsync.filterRules");
        },
        flag: "--filter",
        get placeholder() {
          return t("rsync.logPUploadsDirMerge");
        },
        get hint() {
          return t("rsync.rsyncsFilterSyntaxOneRule");
        },
      },
      {
        kind: "list",
        key: "excludeFrom",
        get label() {
          return t("rsync.excludePatternsFromFiles");
        },
        flag: "--exclude-from",
        placeholder: "excludes.txt",
        get hint() {
          return t("rsync.filesOnThisComputerOne");
        },
      },
      {
        kind: "list",
        key: "includeFrom",
        get label() {
          return t("rsync.includePatternsFromFiles");
        },
        flag: "--include-from",
      },
      {
        kind: "text",
        key: "filesFrom",
        get label() {
          return t("rsync.copyOnlyTheFilesListed");
        },
        flag: "--files-from",
        placeholder: "files.txt",
      },
      {
        kind: "switch",
        key: "from0",
        get label() {
          return t("rsync.namesInThoseFilesEnd");
        },
        flag: "-0",
      },
      {
        kind: "select",
        key: "filterFiles",
        get label() {
          return t("rsync.rulesInRsyncFilterFiles");
        },
        choices: [
          {
            value: "off",
            get label() {
              return t("rsync.ignoreThem");
            },
          },
          {
            value: "read",
            get label() {
              return t("rsync.readThemInEachFolder");
            },
          },
          {
            value: "readAndExclude",
            get label() {
              return t("rsync.readThemAndLeaveThem");
            },
          },
        ],
      },
      {
        kind: "switch",
        key: "cvsExclude",
        get label() {
          return t("rsync.leaveOutFilesCvsWould");
        },
        flag: "-C",
      },
    ],
  },
  {
    id: "attributes",
    get title() {
      return t("rsync.timesPermissionsAndOwnership");
    },
    fields: [
      {
        kind: "switch",
        key: "times",
        get label() {
          return t("rsync.keepModificationTimes");
        },
        flag: "-t",
      },
      {
        kind: "switch",
        key: "omitDirTimes",
        get label() {
          return t("rsync.leaveTheTimesOfFolders");
        },
        flag: "-O",
      },
      {
        kind: "switch",
        key: "perms",
        get label() {
          return t("rsync.keepPermissions");
        },
        flag: "-p",
      },
      {
        kind: "switch",
        key: "executability",
        get label() {
          return t("rsync.withoutPermissionsKeepWhetherFiles");
        },
        flag: "-E",
      },
      {
        kind: "text",
        key: "chmod",
        get label() {
          return t("rsync.changePermissions");
        },
        flag: "--chmod",
        placeholder: "D755,F644",
        get hint() {
          return t("rsync.asRsyncTakesThemD");
        },
      },
      {
        kind: "switch",
        key: "owner",
        get label() {
          return t("rsync.keepTheOwner");
        },
        flag: "-o",
      },
      {
        kind: "switch",
        key: "group",
        get label() {
          return t("rsync.keepTheGroup");
        },
        flag: "-g",
      },
      {
        kind: "text",
        key: "chown",
        get label() {
          return t("rsync.setTheOwnerAndGroup");
        },
        flag: "--chown",
        placeholder: "www-data:www-data",
      },
      {
        kind: "switch",
        key: "numericIds",
        get label() {
          return t("rsync.matchOwnersAndGroupsBy");
        },
        flag: "--numeric-ids",
      },
    ],
  },
  {
    id: "links",
    get title() {
      return t("rsync.symbolicLinks");
    },
    fields: [
      {
        kind: "select",
        key: "links",
        get label() {
          return t("rsync.links");
        },
        choices: [
          {
            value: "follow",
            get label() {
              return t("rsync.copyWhatTheyPointTo");
            },
          },
          {
            value: "preserve",
            get label() {
              return t("rsync.copyThemAsLinksL");
            },
          },
          {
            value: "skip",
            get label() {
              return t("rsync.leaveThemOut");
            },
          },
        ],
      },
      {
        kind: "switch",
        key: "safeLinks",
        get label() {
          return t("rsync.leaveOutLinksThatPoint");
        },
        flag: "--safe-links",
      },
      {
        kind: "switch",
        key: "copyUnsafeLinks",
        get label() {
          return t("rsync.copyWhatLinksOutsideThe");
        },
        flag: "--copy-unsafe-links",
      },
      {
        kind: "switch",
        key: "copyDirlinks",
        get label() {
          return t("rsync.copyLinksToFoldersAs");
        },
        flag: "-k",
      },
      {
        kind: "switch",
        key: "keepDirlinks",
        get label() {
          return t("rsync.treatLinksToFoldersOn");
        },
        flag: "-K",
      },
    ],
  },
  {
    id: "bandwidth",
    get title() {
      return t("rsync.bandwidthAndCompression");
    },
    fields: [
      {
        kind: "number",
        key: "bwlimitKib",
        get label() {
          return t("rsync.speedLimit");
        },
        flag: "--bwlimit",
        unit: () => "KiB/s",
        max: 100 * 1024 * 1024,
        get hint() {
          return t("rsync.n0IsUnlimitedSharedBy");
        },
      },
      {
        kind: "switch",
        key: "compress",
        get label() {
          return t("rsync.compressDataOnTheWay");
        },
        flag: "-z",
        get hint() {
          return t("rsync.overAConnectionOfIts");
        },
      },
    ],
  },
  {
    id: "backup",
    get title() {
      return t("rsync.backups");
    },
    fields: [
      {
        kind: "switch",
        key: "backup",
        get label() {
          return t("rsync.keepFilesThatAreReplaced");
        },
        flag: "-b",
      },
      {
        kind: "text",
        key: "backupDir",
        get label() {
          return t("rsync.backupFolder");
        },
        flag: "--backup-dir",
        get placeholder() {
          return t("rsync.besideEachFile");
        },
        get hint() {
          return t("rsync.onTheTargetsSideA");
        },
      },
      {
        kind: "text",
        key: "backupSuffix",
        get label() {
          return t("rsync.addedToBackupNames");
        },
        flag: "--suffix",
        get placeholder() {
          return t("rsync.withoutABackupFolder");
        },
      },
    ],
  },
  {
    id: "output",
    get title() {
      return t("rsync.dryRunAndReporting");
    },
    fields: [
      {
        kind: "switch",
        key: "dryRun",
        get label() {
          return t("rsync.dryRunShowWhatWould");
        },
        flag: "-n",
      },
      {
        kind: "switch",
        key: "itemize",
        get label() {
          return t("rsync.logEachChangeInRsyncs");
        },
        flag: "-i",
      },
      {
        kind: "switch",
        key: "verbose",
        get label() {
          return t("rsync.logEachFileCopied");
        },
        flag: "-v",
      },
      {
        kind: "switch",
        key: "stats",
        get label() {
          return t("rsync.logStatisticsAtTheEnd");
        },
        flag: "--stats",
      },
      {
        kind: "text",
        key: "logFile",
        get label() {
          return t("rsync.alsoWriteToTheLog");
        },
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
