import type { DateFormat } from "./settings";
import type { FileEntry } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit === 0 ? `${value} B` : `${value.toFixed(1)} ${UNITS[unit]}`;
}

function pad(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

const localeDateTime = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/**
 * Local time as `YYYY-MM-DD HH:MM` (sortable and the same in every locale), with seconds, or in
 * the system's own style.
 */
export function formatDate(epochSeconds: number | null, format: DateFormat = "minutes"): string {
  if (epochSeconds === null) return "";
  const date = new Date(epochSeconds * 1000);
  if (format === "locale") return localeDateTime.format(date);
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

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatSize(Math.max(0, Math.round(bytesPerSecond)))}/s`;
}

/** `45s`, `2m 5s`, `1h 2m`. */
export function formatDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.round(totalSeconds));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}
