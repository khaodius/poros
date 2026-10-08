// Where the files a pane shows live, so moves and copies can tell when two panes share a
// filesystem and the work can happen in place instead of through a transfer.

import type { FileLocation } from "./types";

export interface ServerIdentity {
  host: string;
  port: number;
  username: string;
}

export interface FileOrigin {
  kind: "local" | "remote";
  sessionId?: string;
  /** For a server: who is signed in where, which another window's session can match. */
  server?: ServerIdentity;
}

/** Both on this computer, or on one server signed in as the same user. */
export function sameFilesystem(first: FileOrigin, second: FileOrigin): boolean {
  if (first.kind !== second.kind) return false;
  if (first.kind === "local") return true;
  if (first.sessionId && first.sessionId === second.sessionId) return true;
  const one = first.server;
  const other = second.server;
  return (
    !!one &&
    !!other &&
    one.host.toLowerCase() === other.host.toLowerCase() &&
    one.port === other.port &&
    one.username === other.username
  );
}

export function toLocation(origin: FileOrigin): FileLocation {
  return origin.kind === "remote" && origin.sessionId
    ? { kind: "remote", sessionId: origin.sessionId }
    : { kind: "local" };
}
