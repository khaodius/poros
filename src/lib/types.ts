// Mirrors the serde types in src-tauri/src (model.rs, error.rs, ssh/mod.rs, session.rs,
// events.rs, transfer/, connections.rs, themes.rs). Field names are camelCase on the wire.

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
  compression?: boolean;
  /** Lets the backend fill an empty password or passphrase from the system keychain. */
  savedConnectionId?: string | null;
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
  savedConnectionId?: string;
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
  | "ssh"
  | "cancelled"
  | "keychain";

export interface AppError {
  kind: ErrorKind;
  message: string;
  hostKey?: HostKeyInfo;
  path?: string;
}

export type LogLevel = "info" | "warn" | "error" | "server";

export interface LogRecord {
  /** Milliseconds since the Unix epoch. */
  timestamp: number;
  level: LogLevel;
  sessionId?: string;
  message: string;
}

export interface SessionClosed {
  sessionId: string;
  reason: string;
}

export type StoreName = "settings" | "connections" | "themes";

export type Direction = "upload" | "download";
export type JobKind = "file" | "folder";
export type JobState = "queued" | "running" | "paused" | "conflict" | "done" | "skipped" | "failed";
export type ExistsAction =
  "ask" | "overwrite" | "overwriteIfNewer" | "overwriteIfDifferent" | "resume" | "rename" | "skip";

export interface ConflictInfo {
  sourceSize: number;
  sourceModified: number | null;
  targetSize: number;
  targetModified: number | null;
}

export interface JobSnapshot {
  id: number;
  /** Sorts jobs in processing order. */
  rank: string;
  sessionId: string;
  direction: Direction;
  kind: JobKind;
  name: string;
  source: string;
  target: string;
  targetDirectory: string;
  size: number;
  transferred: number;
  /** Bytes per second. */
  speed: number;
  connections: number;
  state: JobState;
  error?: string;
  conflict?: ConflictInfo;
  attempts: number;
}

export interface QueueCounts {
  queued: number;
  running: number;
  paused: number;
  conflict: number;
  done: number;
  skipped: number;
  failed: number;
}

export interface TransferStats {
  counts: QueueCounts;
  remainingBytes: number;
  uploadSpeed: number;
  downloadSpeed: number;
  queuePaused: boolean;
}

export interface ChangedDirectory {
  side: "local" | "remote";
  sessionId?: string;
  path: string;
}

export interface TransferUpdate {
  jobs: JobSnapshot[];
  removed: number[];
  changedDirectories: ChangedDirectory[];
  stats: TransferStats;
}

export interface TransferList {
  jobs: JobSnapshot[];
  stats: TransferStats;
}

export interface TransferItem {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
}

export interface EnqueueRequest {
  sessionId: string;
  direction: Direction;
  targetDirectory: string;
  items: TransferItem[];
}

export type AuthType = AuthMethod["type"];

export interface SavedConnection {
  /** Empty for a connection not saved yet. */
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: AuthType;
  keyPath?: string | null;
  remotePath?: string | null;
  /** A password or passphrase is in the system keychain. */
  saveSecret: boolean;
  /** Milliseconds since the Unix epoch. */
  lastUsed?: number | null;
}

export interface ThemeFile {
  id: string;
  path: string;
  theme?: unknown;
  /** Why the file could not be read. */
  error?: string;
}
