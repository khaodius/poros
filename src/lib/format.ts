import { dateTimeFormat, formatNumber, t } from "../i18n";
import type { DateFormat } from "./settings";
import type { FileEntry } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** Digits as the language writes them, without grouping, as sizes have always been shown. */
const SIZE_DIGITS: Intl.NumberFormatOptions = { useGrouping: false };
const SIZE_DECIMALS: Intl.NumberFormatOptions = {
  useGrouping: false,
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
};

/** `1.5 KB` in English, `1,5 KB` in German. The units stay as they are in every language. */
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit === 0
    ? `${formatNumber(value, SIZE_DIGITS)} B`
    : `${formatNumber(value, SIZE_DECIMALS)} ${UNITS[unit]}`;
}

/** A whole percentage, `62%` in English, `62 %` in German. */
export function formatPercent(percent: number): string {
  return formatNumber(percent / 100, { style: "percent", maximumFractionDigits: 0 });
}

function pad(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

const LOCALE_DATE_TIME: Intl.DateTimeFormatOptions = { dateStyle: "medium", timeStyle: "short" };

/**
 * Local time as `YYYY-MM-DD HH:MM` (sortable and the same in every language), with seconds, or
 * in the style of the interface's language.
 */
export function formatDate(epochSeconds: number | null, format: DateFormat = "minutes"): string {
  if (epochSeconds === null) return "";
  const date = new Date(epochSeconds * 1000);
  if (format === "locale") return dateTimeFormat(LOCALE_DATE_TIME).format(date);
  const day = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
  const time = `${pad(date.getHours())}:${pad(date.getMinutes())}`;
  return format === "seconds" ? `${day} ${time}:${pad(date.getSeconds())}` : `${day} ${time}`;
}

export function formatTime(epochMillis: number): string {
  const date = new Date(epochMillis);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

/** `ls -l` style: `drwxr-xr-x`, including setuid/setgid/sticky bits. */
export function formatPermissions(entry: Pick<FileEntry, "kind" | "permissions">): string {
  if (entry.permissions === null) return "";
  const mode = entry.permissions;
  const type = entry.kind === "dir" ? "d" : entry.kind === "symlink" ? "l" : "-";
  const triplet = (shift: number, specialBit: number, specialChar: string) => {
    const read = mode & (4 << shift) ? "r" : "-";
    const write = mode & (2 << shift) ? "w" : "-";
    const executable = (mode & (1 << shift)) !== 0;
    if ((mode & specialBit) === 0) return read + write + (executable ? "x" : "-");
    return read + write + (executable ? specialChar : specialChar.toUpperCase());
  };
  return type + triplet(6, 0o4000, "s") + triplet(3, 0o2000, "s") + triplet(0, 0o1000, "t");
}

/** `0.1.0` -> `v0.1`, `0.1.2` -> `v0.1.2`. */
export function formatVersion(version: string): string {
  const [major = "0", minor = "0", patch = "0"] = version.split(/[.+-]/);
  return patch === "0" ? `v${major}.${minor}` : `v${major}.${minor}.${patch}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatSize(Math.max(0, Math.round(bytesPerSecond)))}/s`;
}

/** `45s`, `2m 5s`, `1h 2m` in English; each language writes its own units. */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  if (seconds < 60) return t("common.durationSeconds", { seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return t("common.durationMinutes", { minutes, seconds: seconds % 60 });
  }
  return t("common.durationHours", { hours: Math.floor(minutes / 60), minutes: minutes % 60 });
}
