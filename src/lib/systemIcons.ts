import type { SystemIconRequest } from "./ipc";
import type { FileEntry } from "./types";

/** The size icons are drawn at in file lists, in CSS pixels. */
export const FILE_ICON_SIZE = 16;

export type IconSubject = Pick<FileEntry, "name" | "kind" | "linkTarget">;

export interface SystemIconLookup {
  /** Files that share it share an icon, so each is looked up once. */
  key: string;
  request: SystemIconRequest;
}

/**
 * What to ask the system for to draw `entry`'s icon. Icons follow the extension, so every file
 * with one asks about a stand-in name; files without one are asked about by name, which Linux
 * uses to tell a Makefile from a README. Broken links keep Poros's own warning icon.
 */
export function systemIconLookup(entry: IconSubject): SystemIconLookup | null {
  const link = entry.kind === "symlink";
  if (link && entry.linkTarget === "broken") return null;
  const folder = entry.kind === "dir" || (link && entry.linkTarget === "dir");
  const prefix = `${folder ? "folder" : "file"}${link ? "-link" : ""}`;
  if (folder) return { key: prefix, request: { name: "folder", folder, link } };
  const dot = entry.name.lastIndexOf(".");
  const extension = dot > 0 ? entry.name.slice(dot + 1).toLowerCase() : "";
  if (extension) {
    return { key: `${prefix}:.${extension}`, request: { name: `file.${extension}`, folder, link } };
  }
  return {
    key: `${prefix}:${entry.name.toLowerCase()}`,
    request: { name: entry.name, folder, link },
  };
}
