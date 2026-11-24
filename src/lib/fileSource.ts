// Local and remote share one interface so panes treat both sides alike.

import { local, remote } from "./ipc";
import { listingScope } from "./listingCache";
import { remoteParent } from "./path";
import type { ConnectProfile, DirListing, SessionInfo } from "./types";
import { t } from "../i18n";

export type PathStyle = "posix" | "windows";

export interface FileSource {
  key: string;
  kind: "local" | "remote";
  label: string;
  pathStyle: PathStyle;
  /**
   * For a server: names the connection whose remembered listings this source's folders are
   * among, as `listingScope` does. Absent where nothing is remembered, as on this computer,
   * whose folders list at once.
   */
  listingScope?: string;
  initialPath(): Promise<string>;
  /** Where to open instead when the first folder cannot be listed, such as one deleted since. */
  fallbackPath?(): Promise<string>;
  home(): Promise<string>;
  /** Drive letters on Windows; absent where a single root exists. */
  roots?(): Promise<string[]>;
  list(path: string): Promise<DirListing>;
  mkdir(parent: string, name: string): Promise<string>;
  /** Creates an empty file; fails if the name is taken. */
  makeFile(parent: string, name: string): Promise<string>;
  rename(path: string, newName: string): Promise<string>;
  remove(paths: string[]): Promise<void>;
}

const isWindows = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);

export const localSource: FileSource = {
  key: "local",
  kind: "local",
  get label() {
    return t("workspace.local");
  },
  pathStyle: isWindows ? "windows" : "posix",
  initialPath: () => local.home(),
  home: () => local.home(),
  roots: isWindows ? () => local.roots() : undefined,
  list: (path) => local.list(path),
  mkdir: (parent, name) => local.mkdir(parent, name),
  makeFile: (parent, name) => local.makeFile(parent, name),
  rename: (path, newName) => local.rename(path, newName),
  remove: (paths) => local.remove(paths),
};

const remoteSources = new Map<string, FileSource>();

/** The key of a session's source, which tells its files from those of other connections. */
export function remoteSourceKey(sessionId: string): string {
  return `remote:${sessionId}`;
}

/** A folder a source changed on its server, and the entries in it that are not what they were. */
export interface RemoteChange {
  /** The connection, as `FileSource.listingScope` names it. */
  scope: string;
  folder: string;
  entries: string[];
}

const changeListeners = new Set<(change: RemoteChange) => void>();

/** Calls `listener` whenever a source creates, renames or deletes something on its server. */
export function onRemoteChange(listener: (change: RemoteChange) => void): () => void {
  changeListeners.add(listener);
  return () => {
    changeListeners.delete(listener);
  };
}

function announce(scope: string, entries: string[]): void {
  const folders = new Map<string, string[]>();
  for (const entry of entries) {
    const folder = remoteParent(entry);
    if (folder !== null) folders.set(folder, [...(folders.get(folder) ?? []), entry]);
  }
  for (const [folder, changed] of folders) {
    for (const listener of changeListeners) listener({ scope, folder, entries: changed });
  }
}

/**
 * One source per session, so a pane keeps its place when the session's details change. The
 * `profile` the session was opened with tells which connection its listings belong to.
 */
export function remoteSource(session: SessionInfo, profile: ConnectProfile | null): FileSource {
  const existing = remoteSources.get(session.id);
  if (existing) return existing;
  const id = session.id;
  const scope = listingScope(session, profile);
  const source: FileSource = {
    key: remoteSourceKey(id),
    kind: "remote",
    label: session.label,
    pathStyle: "posix",
    listingScope: scope,
    initialPath: async () => session.initialPath,
    home: async () => session.home,
    list: (path) => remote.list(id, path),
    mkdir: async (parent, name) => {
      const created = await remote.mkdir(id, parent, name);
      announce(scope, [created]);
      return created;
    },
    makeFile: async (parent, name) => {
      const created = await remote.makeFile(id, parent, name);
      announce(scope, [created]);
      return created;
    },
    rename: async (path, newName) => {
      const renamed = await remote.rename(id, path, newName);
      announce(scope, [path, renamed]);
      return renamed;
    },
    remove: async (paths) => {
      try {
        await remote.remove(id, paths);
      } finally {
        // Whatever went before a failure is gone too.
        announce(scope, paths);
      }
    },
  };
  remoteSources.set(id, source);
  return source;
}

export function forgetRemoteSource(sessionId: string): void {
  remoteSources.delete(sessionId);
}

/** The same source, opening at `path` instead of its usual first folder, or at that folder when
 * `path` cannot be listed. */
export function startingAt(source: FileSource, path: string | undefined): FileSource {
  return path
    ? { ...source, initialPath: async () => path, fallbackPath: source.initialPath }
    : source;
}
