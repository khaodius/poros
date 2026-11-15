// Archives a pane can extract: told apart by name here, to offer Extract on them, and by their
// first bytes in the backend, which mirrors these names in src-tauri/src/file_ops/archive.

import type { ArchiveFormat, Extractor } from "./types";

/** Name endings, each before any it ends with, so `.tar.gz` wins over `.gz`. */
const ENDINGS: [string, ArchiveFormat][] = [
  [".tar.gz", "tarGzip"],
  [".tar.bz2", "tarBzip2"],
  [".tar.zstd", "tarZstd"],
  [".tar.lzma", "tarLzma"],
  [".tar.xz", "tarXz"],
  [".tar.zst", "tarZstd"],
  [".nupkg", "zip"],
  [".tbz2", "tarBzip2"],
  [".tzst", "tarZstd"],
  [".lzma", "lzma"],
  [".zstd", "zstd"],
  [".tgz", "tarGzip"],
  [".tbz", "tarBzip2"],
  [".txz", "tarXz"],
  [".tlz", "tarLzma"],
  [".tar", "tar"],
  [".zip", "zip"],
  [".jar", "zip"],
  [".war", "zip"],
  [".ear", "zip"],
  [".apk", "zip"],
  [".whl", "zip"],
  [".rar", "rar"],
  [".bz2", "bzip2"],
  [".zst", "zstd"],
  [".iso", "iso"],
  [".cab", "cab"],
  [".7z", "sevenZip"],
  [".gz", "gzip"],
  [".xz", "xz"],
];

function ending(name: string): [string, ArchiveFormat] | undefined {
  const lower = name.toLowerCase();
  return ENDINGS.find(([suffix]) => lower.length > suffix.length && lower.endsWith(suffix));
}

/** The format a file's name says it is, if it names an archive. */
export function archiveFormatOf(name: string): ArchiveFormat | null {
  return ending(name)?.[1] ?? null;
}

/** One compressed file, which unpacks to a single file rather than a folder of them. */
export function isSingleFile(format: ArchiveFormat): boolean {
  return ["gzip", "bzip2", "xz", "zstd", "lzma"].includes(format);
}

/** What an archive unpacks to: `site.tar.gz` -> `site`, `dump.sql.gz` -> `dump.sql`. */
export function unpackedName(name: string): string {
  const found = ending(name);
  const stem = (found ? name.slice(0, name.length - found[0].length) : name)
    .replace(/\.part\d+$/i, "")
    .replace(/[. ]+$/, "");
  return stem || name;
}

export const FORMAT_NAMES: Record<ArchiveFormat, string> = {
  zip: "ZIP",
  sevenZip: "7z",
  rar: "RAR",
  tar: "tar",
  tarGzip: "tar.gz",
  tarBzip2: "tar.bz2",
  tarXz: "tar.xz",
  tarZstd: "tar.zst",
  tarLzma: "tar.lzma",
  gzip: "gzip",
  bzip2: "bzip2",
  xz: "xz",
  zstd: "Zstandard",
  lzma: "LZMA",
  iso: "ISO",
  cab: "CAB",
};

/** Settings > Archives: which program extracts on this computer. */
export interface ExtractorSettings {
  /** "" for the best installed program for each format, "custom" for `program`, else a path. */
  choice: string;
  program: string;
  /** `{archive}` and `{folder}` stand for the archive and the folder it goes into. */
  arguments: string;
}

export const CUSTOM_EXTRACTOR = "custom";
export const CUSTOM_ARGUMENTS_EXAMPLE = 'x -y -o"{folder}" "{archive}"';

export function extractorOf(settings: ExtractorSettings): Extractor {
  if (settings.choice === CUSTOM_EXTRACTOR) {
    return { kind: "custom", program: settings.program, arguments: settings.arguments };
  }
  return settings.choice ? { kind: "installed", path: settings.choice } : { kind: "auto" };
}
