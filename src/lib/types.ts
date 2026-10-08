// Mirrors the serde types in src-tauri/src (model.rs, error.rs, ssh/mod.rs, session.rs,
// events.rs, transfer/, sync/, file_ops/, connections.rs, themes.rs, fonts.rs). Field names are
// camelCase on the wire.

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
  /** A fixed TCP receive buffer; absent or null leaves it to the system. */
  receiveBufferKib?: number | null;
  /** A fixed TCP send buffer; absent or null leaves it to the system. */
  sendBufferKib?: number | null;
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
  | "rsync"
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
  /** File data sent so far when rsync updates the file; absent for whole-file copies. */
  deltaBytes?: number;
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

/** Where files are: this computer or the server of a session. */
export type FileLocation = { kind: "local" } | { kind: "remote"; sessionId: string };

export type PlaceMode = "move" | "copy";
/** Replace overwrites files and merges folders; keepBoth names the newcomer `name (2)`. */
export type NameClash = "replace" | "skip" | "keepBoth";

export interface MoveCopyRequest {
  operationId: string;
  location: FileLocation;
  mode: PlaceMode;
  sources: string[];
  targetDirectory: string;
  conflict: NameClash;
}

export type OperationMethod = "rename" | "copyData" | "command" | "stream" | "local";

export interface OperationFailure {
  path: string;
  message: string;
}

export interface OperationSummary {
  /** Where each moved or copied item is now. */
  placed: string[];
  skipped: number;
  failures: OperationFailure[];
  methods: OperationMethod[];
}

export interface OperationProgress {
  operationId: string;
  files: number;
  bytes: number;
  current?: string;
}

export interface FileDetails {
  uid: number | null;
  gid: number | null;
  /** Seconds since the Unix epoch. */
  accessed: number | null;
  linkTarget?: string;
}

export interface FolderUsage {
  files: number;
  folders: number;
  bytes: number;
}

/** Bits to turn on and off; the rest keep each entry's own value. */
export interface ModeChange {
  set: number;
  clear: number;
}

export interface PermissionRequest {
  operationId: string;
  sessionId: string;
  paths: string[];
  files: ModeChange;
  folders: ModeChange;
  recursive: boolean;
  /** A name or number; null keeps it. */
  owner: string | null;
  group: string | null;
}

export interface PermissionSummary {
  changed: number;
  skippedLinks: number;
  failures: OperationFailure[];
}

export interface FileRef {
  location: FileLocation;
  path: string;
}

export interface CompareRequest {
  operationId: string;
  left: FileRef;
  right: FileRef;
  ignoreWhitespace: boolean;
  ignoreCase: boolean;
}

export type CompareRowKind = "equal" | "changed" | "removed" | "added";
export type LineEnding = "none" | "lf" | "crlf" | "mixed";

/** Changes are `[start, end)` in string indexes; a changed row without any differs throughout. */
export interface CompareRow {
  kind: CompareRowKind;
  left?: number;
  right?: number;
  leftChanges?: [number, number][];
  rightChanges?: [number, number][];
}

export type CompareContent =
  | {
      kind: "text";
      leftLines: string[];
      rightLines: string[];
      rows: CompareRow[];
      added: number;
      removed: number;
      changed: number;
      leftLineEnding: LineEnding;
      rightLineEnding: LineEnding;
    }
  | { kind: "binary"; tooLarge: boolean };

export interface Comparison {
  leftSize: number;
  rightSize: number;
  /** Byte for byte the same. */
  identical: boolean;
  content: CompareContent;
}
