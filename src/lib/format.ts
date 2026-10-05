import type { FileEntry } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** Binary units with one decimal above 1 KB: `0 B`, `512 B`, `1.5 KB`, `2.0 GB`. */
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

/** `YYYY-MM-DD HH:MM` in local time: sortable, unambiguous across locales. */
export function formatDate(epochSeconds: number | null): string {
  if (epochSeconds === null) return "";
  const date = new Date(epochSeconds * 1000);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
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
  // One `rwx` group; `shift` selects owner (6), group (3) or other (0).
  const triplet = (shift: number, specialBit: number, specialChar: string) => {
    const read = mode & (4 << shift) ? "r" : "-";
    const write = mode & (2 << shift) ? "w" : "-";
    const executable = (mode & (1 << shift)) !== 0;
    const special = (mode & specialBit) !== 0;
    const execute = special
      ? executable
        ? specialChar
        : specialChar.toUpperCase()
      : executable
        ? "x"
        : "-";
    return read + write + execute;
  };
  return (
    type + triplet(6, 0o4000, "s") + triplet(3, 0o2000, "s") + triplet(0, 0o1000, "t")
  );
}

/** `0.1.0` -> `v0.1`, `0.1.2` -> `v0.1.2`. */
export function formatVersion(version: string): string {
  const [major = "0", minor = "0", patch = "0"] = version.split(/[.+-]/);
  return patch === "0" ? `v${major}.${minor}` : `v${major}.${minor}.${patch}`;
}

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
