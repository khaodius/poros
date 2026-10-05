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

function pad(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** `YYYY-MM-DD HH:MM` in local time: sortable, unambiguous across locales. */
export function formatDate(secs: number | null): string {
  if (secs === null) return "";
  const d = new Date(secs * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function formatTime(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** `ls -l` style: `drwxr-xr-x`, including setuid/setgid/sticky bits. */
export function formatPermissions(entry: Pick<FileEntry, "kind" | "permissions">): string {
  if (entry.permissions === null) return "";
  const mode = entry.permissions;
  const type = entry.kind === "dir" ? "d" : entry.kind === "symlink" ? "l" : "-";
  const triplet = (shift: number, special: number, specialChar: string) => {
    const r = mode & (4 << shift) ? "r" : "-";
    const w = mode & (2 << shift) ? "w" : "-";
    const xBit = mode & (1 << shift);
    const sBit = mode & special;
    const x = sBit ? (xBit ? specialChar : specialChar.toUpperCase()) : xBit ? "x" : "-";
    return r + w + x;
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

export function pluralize(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
