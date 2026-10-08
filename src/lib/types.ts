// Mirrors the serde types in src-tauri/src (model.rs, error.rs, protocol.rs, ssh/mod.rs,
// session.rs, events.rs, transfer/, sync/, connections.rs, cloud/, themes.rs, fonts.rs). Field
// names are camelCase on the wire.

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

export type Protocol = "sftp" | "ftp" | "ftps" | "ftpsImplicit" | "googleDrive" | "oneDrive";

export type CloudProvider = "google" | "microsoft";

export type AuthMethod =
  | { type: "password"; password: string }
  | { type: "publicKey"; keyPath: string; passphrase?: string | null }
  | { type: "agent" }
  /**
   * A Google or Microsoft account. `grantId` names a sign-in just made in the browser; without
   * it, a saved connection's account comes from the system keychain.
   */
  | { type: "oauth"; grantId?: string | null };

export interface ConnectProfile {
  protocol: Protocol;
  host: string;
  port: number;
  username: string;
  auth: AuthMethod;
  initialPath?: string | null;
  timeoutSecs?: number | null;
  keepaliveSecs?: number | null;
  compression?: boolean;
  /** A fixed TCP receive buffer; absent or null leaves it to the system. */
  receiveBufferKib?: number | null;
  /** A fixed TCP send buffer; absent or null leaves it to the system. */
  sendBufferKib?: number | null;
  /** Lets the backend fill an empty password or passphrase from the system keychain. */
  savedConnectionId?: string | null;
  /** FTP data connections come from the server (active mode) instead of passive mode. */
  ftpActive?: boolean;
}

export interface HostKeyInfo {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  /** Why the system did not trust an FTPS server's TLS certificate; absent for SSH host keys. */
  certificateProblem?: string;
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
  protocol: Protocol;
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
  | "rsync"
  | "ftp"
  | "cloud"
  | "unsupported"
  | "integrity"
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

/** `relay` copies from one server to another. */
export type Direction = "upload" | "download" | "relay";
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
  /** The server written to, or read from for a download. */
  sessionId: string;
  /** The server a relay reads from. */
  sourceSessionId?: string;
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
  /** File data sent so far when rsync updates the file; absent for whole-file copies. */
  deltaBytes?: number;
  /** Two FTP servers sent the file straight to each other (FXP). */
  direct?: boolean;
  /** Queued while its server is out of reach after a dropped connection. */
  reconnecting?: boolean;
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
  /** The server a relay reads from. */
  sourceSessionId?: string | null;
  direction: Direction;
  targetDirectory: string;
  items: TransferItem[];
}

export type SyncDirection = "upload" | "download" | "both";
export type CompareMode = "sizeAndTime" | "sizeOnly" | "checksum" | "always";
export type SyncAction = "upload" | "download" | "deleteLocal" | "deleteRemote" | "conflict";
export type SyncReason =
  | "new"
  | "changed"
  | "contentDiffers"
  | "always"
  | "newer"
  | "extraneous"
  | "typeDiffers"
  | "bothChanged";

export interface SyncRequest {
  /** Names the comparison for progress events and cancelling. */
  requestId: string;
  sessionId: string;
  localPath: string;
  remotePath: string;
  direction: SyncDirection;
  compare: CompareMode;
  deleteExtraneous: boolean;
  skipNewerOnTarget: boolean;
  ignoreExisting: boolean;
  timeToleranceSecs: number;
  excludes: string[];
}

export interface SyncFacts {
  isDir: boolean;
  size: number;
  /** Seconds since the Unix epoch. */
  modified: number | null;
}

export interface SyncItem {
  id: number;
  /** Relative to the synchronized folders, with `/` between components. */
  path: string;
  isDir: boolean;
  action: SyncAction;
  reason: SyncReason;
  /** 1 for a file; for a folder, the files inside it. */
  files: number;
  bytes: number;
  local?: SyncFacts;
  remote?: SyncFacts;
}

export interface SyncCounts {
  unchanged: number;
  /** Left alone because the target's copy is newer, or because existing files are kept. */
  kept: number;
  /** Only on the target, and kept because deleting is off. */
  extraOnTarget: number;
  excluded: number;
  passedOver: number;
}

export interface SyncPlanView {
  planId: string;
  localRoot: string;
  remoteRoot: string;
  items: SyncItem[];
  counts: SyncCounts;
  /** A sample of the paths that were passed over. */
  passedOver: string[];
}

export interface SyncProgress {
  requestId: string;
  stage: "listing" | "comparing";
  localEntries: number;
  remoteEntries: number;
  compared: number;
  toCompare: number;
}

export interface SyncChoice {
  id: number;
  action: SyncAction;
}

export interface SyncRunRequest {
  planId: string;
  choices: SyncChoice[];
}

export interface SyncRunSummary {
  queuedFiles: number;
  queuedBytes: number;
  deleted: number;
  createdFolders: number;
  failures: string[];
}

export type AuthType = AuthMethod["type"];

export interface SavedConnection {
  /** Empty for a connection not saved yet. */
  id: string;
  protocol: Protocol;
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
  ftpActive?: boolean;
}

export interface CloudProviderStatus {
  provider: CloudProvider;
  /** An app to sign in with is set in Settings or built into this release. */
  configured: boolean;
  builtIn: boolean;
}

/** A finished browser sign-in, until it is saved with a connection or used to connect. */
export interface SignedIn {
  grantId: string;
  provider: CloudProvider;
  /** The account's email address or name. */
  account: string;
}

export interface FontFamily {
  name: string;
  /** Older per-style names of the family, for web views that do not know its main name. */
  alternates: string[];
  monospaced: boolean;
}

export interface ThemeFile {
  id: string;
  path: string;
  theme?: unknown;
  /** Why the file could not be read. */
  error?: string;
}
