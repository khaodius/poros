// Mirrors the serde types in src-tauri/src (model.rs, error.rs, ssh/mod.rs, session.rs,
// events.rs). Field names are camelCase on the wire.

export type EntryKind = "dir" | "file" | "symlink" | "other";
export type LinkTarget = "dir" | "file" | "broken";

export interface FileEntry {
  name: string;
  /** Absolute path in the source's own syntax (`/` remote, native local). */
  path: string;
  kind: EntryKind;
  linkTarget?: LinkTarget;
  size: number;
  /** Seconds since the Unix epoch. */
  modified: number | null;
  /** `mode & 0o7777`. */
  permissions: number | null;
  owner: string | null;
  group: string | null;
  hidden: boolean;
}

export interface DirListing {
  path: string;
  parent: string | null;
  entries: FileEntry[];
}

export type AuthMethod =
  | { type: "password"; password: string }
  | { type: "publicKey"; keyPath: string; passphrase?: string | null }
  | { type: "agent" };

export interface ConnectProfile {
  host: string;
  port: number;
  username: string;
  auth: AuthMethod;
  initialPath?: string | null;
  timeoutSecs?: number | null;
  keepaliveSecs?: number | null;
}

export interface HostKeyInfo {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
}

export interface HostKeyApproval {
  fingerprint: string;
  remember: boolean;
}

export interface SessionInfo {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  home: string;
  initialPath: string;
}

export type ErrorKind =
  | "hostKeyUnknown"
  | "hostKeyChanged"
  | "authFailed"
  | "passphraseRequired"
  | "connection"
  | "timeout"
  | "sessionNotFound"
  | "disconnected"
  | "notFound"
  | "permissionDenied"
  | "alreadyExists"
  | "invalidInput"
  | "io"
  | "sftp"
  | "ssh";

export interface AppError {
  kind: ErrorKind;
  message: string;
  hostKey?: HostKeyInfo;
  path?: string;
}

export type LogLevel = "info" | "warn" | "error" | "server";

export interface LogRecord {
  ts: number;
  level: LogLevel;
  sessionId?: string;
  message: string;
}

export interface SessionClosed {
  sessionId: string;
  reason: string;
}
