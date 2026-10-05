// Local and remote share one interface so panes, and later the transfer queue, treat both
// sides alike.

import { local, remote } from "./ipc";
import type { DirListing, SessionInfo } from "./types";

export type PathStyle = "posix" | "windows";

export interface FileSource {
  key: string;
  kind: "local" | "remote";
  label: string;
  pathStyle: PathStyle;
  initialPath(): Promise<string>;
  home(): Promise<string>;
  /** Drive letters on Windows; absent where a single root exists. */
  roots?(): Promise<string[]>;
  list(path: string): Promise<DirListing>;
  mkdir(parent: string, name: string): Promise<string>;
  rename(path: string, newName: string): Promise<string>;
  remove(paths: string[]): Promise<void>;
}

const isWindows = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);

export const localSource: FileSource = {
  key: "local",
  kind: "local",
  label: "Local",
  pathStyle: isWindows ? "windows" : "posix",
  initialPath: () => local.home(),
  home: () => local.home(),
  roots: isWindows ? () => local.roots() : undefined,
  list: (path) => local.list(path),
  mkdir: (parent, name) => local.mkdir(parent, name),
  rename: (path, newName) => local.rename(path, newName),
  remove: (paths) => local.remove(paths),
};

export function remoteSource(session: SessionInfo): FileSource {
  const id = session.id;
  return {
    key: `remote:${id}`,
    kind: "remote",
    label: session.label,
    pathStyle: "posix",
    initialPath: async () => session.initialPath,
    home: async () => session.home,
    list: (path) => remote.list(id, path),
    mkdir: (parent, name) => remote.mkdir(id, parent, name),
    rename: (path, newName) => remote.rename(id, path, newName),
    remove: (paths) => remote.remove(id, paths),
  };
}
