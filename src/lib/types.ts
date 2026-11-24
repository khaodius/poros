// Mirrors the serde types in src-tauri/src (model.rs, error.rs, protocol.rs, ssh/mod.rs,
// session.rs, events.rs, transfer/, sync/, file_ops/, connections.rs, cloud/, themes.rs,
// fonts.rs, automation/, text.rs, editor.rs, terminal.rs, desktop.rs). Field names are camelCase
// on the wire.

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

export type Protocol =
  | "sftp"
  | "ftp"
  | "ftps"
  | "ftpsImplicit"
  | "webDav"
  | "webDavs"
  | "s3"
  | "backblazeB2"
  | "googleDrive"
  | "oneDrive"
  | "box"
  /** The session of an unlocked vault. Never something to connect to. */
  | "cryptomator";

export type CloudProvider = "google" | "microsoft" | "box";

/** Where the bucket goes in an S3 address. */
export type S3Addressing = "automatic" | "path" | "virtualHosted";

export interface S3Options {
  /** Empty takes it from the endpoint, or else `us-east-1`. */
  region: string;
  addressing: S3Addressing;
}

export type AuthMethod =
  | { type: "password"; password: string }
  | { type: "publicKey"; keyPath: string; passphrase?: string | null }
  | { type: "agent" }
  /**
   * A Google, Microsoft or Box account. `grantId` names a sign-in just made in the browser; without
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
  /** Skips the proxy from the connection settings. */
  bypassProxy?: boolean;
  /** A saved connection to reach this server through, like OpenSSH's ProxyJump. */
  jumpConnectionId?: string | null;
  /** The rsync program on the server, when it is not `rsync` on its PATH. */
  rsyncPath?: string | null;
  s3?: S3Options;
}

export interface HostKeyInfo {
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  /** Why the system did not trust a server's TLS certificate; absent for SSH host keys. */
  certificateProblem?: string;
}

export interface HostKeyApproval {
  fingerprint: string;
  remember: boolean;
}

/** An unlocked Cryptomator vault, as its session describes it. */
export interface VaultInfo {
  /** The session of the server the vault's files are on. */
  parentSessionId: string;
  /** The vault's folder on that server. The decrypted files have this path and those below it. */
  root: string;
  /** That folder's name. */
  name: string;
}

/** What a share link lets its holder do. */
export type LinkKind = "view" | "edit" | "download";

/** Who a share link works for: whoever holds it, the account's organization, or people the item
 * is already shared with. */
export type ShareAudience = "anyone" | "organization" | "invited";

/** Whether share links end by themselves. */
export type ShareExpiry = "never" | "optional" | "required";

/** What share links a session can make. Mirrors `ShareCapabilities` in src-tauri/src/share.rs. */
export interface ShareCapabilities {
  kinds: LinkKind[];
  audiences: ShareAudience[];
  expiry: ShareExpiry;
  /** The longest a link can last, where the service sets a limit. */
  maxLifetimeSecs?: number;
  password: boolean;
  /** Links stay with the item, so they can be listed and removed. */
  revocable: boolean;
  folders: boolean;
  /** The service keeps one link per item, so making a link changes the one it has. */
  singleLink: boolean;
  /** The organization the `organization` audience stands for, when its name is known. */
  organization?: string;
}

export interface ShareOptions {
  kind: LinkKind;
  audience: ShareAudience;
  lifetimeSecs?: number;
  password?: string;
}

/** A share link. Its address is a credential: it is shown and copied, never logged. */
export interface ShareLink {
  /** Names the link when removing it; empty for a link that cannot be removed. */
  id: string;
  url: string;
  kind: LinkKind;
  audience: ShareAudience;
  /** Seconds since the Unix epoch. */
  expiresAt?: number;
  hasPassword: boolean;
  revocable: boolean;
  /** What the user should know about the link, in whole sentences. */
  notes: string[];
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
  /** Set for the session of an unlocked vault, whose host, port and username are its server's. */
  vault?: VaultInfo;
  /** The share links the session can make; absent when the service has none. */
  sharing?: ShareCapabilities;
  /** Listings give times only to the minute, or to the day, as FTP's `LIST` does. */
  coarseTimes?: boolean;
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
  | "untrusted"
  | "cancelled"
  | "keychain"
  | "update"
  | "wrongPassword";

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

export type StoreName = "settings" | "connections" | "themes" | "schedules";

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
  /** The speed limits in force, and the rule of the schedule that set them. */
  limits: LimitStatus;
}

/** Mirrors src-tauri/src/transfer/limit_schedule.rs. `paused` holds that direction's transfers. */
export type LimitMode = "unlimited" | "limited" | "paused";

export interface RateLimit {
  mode: LimitMode;
  /** KiB per second, kept while the mode is another so switching back finds it again. */
  kib: number;
}

export interface LimitRule {
  id: string;
  name: string;
  /** The weekdays the rule starts on, Monday being 0. */
  days: number[];
  /** Minutes after midnight, local time. */
  startMinute: number;
  /** At or before the start, the rule runs into the next day; equal to it, for 24 hours. */
  endMinute: number;
  upload: RateLimit;
  download: RateLimit;
}

export interface LimitSchedule {
  enabled: boolean;
  /** Where several cover the same time, the one latest in the list applies. */
  rules: LimitRule[];
}

/** Mirrors src-tauri/src/transfer/scheduled_limits.rs. Times are seconds since the epoch. */
export interface LimitStatus {
  upload: RateLimit;
  download: RateLimit;
  /** The rule in force; while the schedule is ignored, the one that would be. */
  rule: { index: number; name: string } | null;
  /** The schedule's next change, which also ends ignoring it. */
  until: number | null;
  ignored: boolean;
  /** When transfers waiting for the schedule continue, by direction. */
  pauseEnds: Record<Direction, number | null>;
}

export interface ChangedDirectory {
  side: "local" | "remote";
  sessionId?: string;
  path: string;
}

export interface TransferUpdate {
  /** One more than the update before, so a missed one shows. */
  sequence: number;
  jobs: JobSnapshot[];
  removed: number[];
  changedDirectories: ChangedDirectory[];
  stats: TransferStats;
}

export interface TransferList {
  /** The latest update the list already holds. */
  sequence: number;
  jobs: JobSnapshot[];
  stats: TransferStats;
}

export interface TransferItem {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
}

/** Files a pane drags out of Poros onto the desktop or into another program. */
export interface DragOutRequest {
  /** The server the files are on; `null` for files on this computer. */
  sessionId: string | null;
  items: TransferItem[];
}

/** Mirrored in src-tauri/src/drag_out/mod.rs. */
export type DragOutOutcome =
  | { kind: "dropped" }
  | { kind: "cancelled" }
  /**
   * Let go over the window the drag came from. `at` is where in its page, for the page to drop
   * the files itself; `null` when the page was told of the drop directly.
   */
  | { kind: "returned"; at: { x: number; y: number } | null; copy: boolean };

/**
 * Mirrored in src-tauri/src/drag_out/mod.rs. Where in its page the pointer is while files this
 * window dragged out of Poros are back over it; `at` is `null` once the pointer leaves.
 */
export interface DragOutHover {
  at: { x: number; y: number } | null;
  /** The copy key is held. */
  copy: boolean;
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
  | "bothChanged"
  | "append"
  | "attributes";

export type DeleteTiming = "before" | "during" | "delay" | "after";
export type LinkMode = "follow" | "preserve" | "skip";
export type AppendMode = "off" | "append" | "verify";
export type FilterFiles = "off" | "read" | "readAndExclude";

/** rsync's options as a synchronization carries them out; see src-tauri/src/sync/options.rs. */
export interface SyncOptions {
  compare: CompareMode;
  /** `--modify-window`. */
  timeToleranceSecs: number;
  /** `-u`. */
  skipNewerOnTarget: boolean;
  ignoreExisting: boolean;
  /** `--delete`. */
  deleteExtraneous: boolean;
  excludes: string[];

  existingOnly: boolean;
  /** Bytes. */
  minSize: number | null;
  maxSize: number | null;

  recursive: boolean;
  dirs: boolean;
  mkpath: boolean;
  pruneEmptyDirs: boolean;
  wholeFile: boolean;
  inplace: boolean;
  append: AppendMode;
  blockSize: number | null;
  removeSourceFiles: boolean;
  fsync: boolean;
  rsyncPath: string;

  deleteTiming: DeleteTiming;
  deleteExcluded: boolean;
  maxDelete: number | null;
  force: boolean;
  ignoreErrors: boolean;

  filterRules: string[];
  includes: string[];
  includeFrom: string[];
  excludeFrom: string[];
  filesFrom: string;
  from0: boolean;
  filterFiles: FilterFiles;
  cvsExclude: boolean;

  times: boolean;
  omitDirTimes: boolean;
  perms: boolean;
  executability: boolean;
  chmod: string;
  owner: boolean;
  group: boolean;
  chown: string;
  numericIds: boolean;

  links: LinkMode;
  safeLinks: boolean;
  copyUnsafeLinks: boolean;
  copyDirlinks: boolean;
  keepDirlinks: boolean;

  /** KiB per second; zero is unlimited. */
  bwlimitKib: number;
  compress: boolean;

  backup: boolean;
  backupDir: string;
  backupSuffix: string;

  dryRun: boolean;
  itemize: boolean;
  verbose: boolean;
  stats: boolean;
  logFile: string;
}

/** Options with rsync arguments applied, checked, and shown as the command line. */
export interface OptionCheck {
  options: SyncOptions;
  arguments: string;
  warnings: string[];
  errors: string[];
}

export interface SyncRequest extends SyncOptions {
  /** Names the comparison for progress events and cancelling. */
  requestId: string;
  sessionId: string;
  localPath: string;
  remotePath: string;
  direction: SyncDirection;
  /** rsync arguments applied over the options. */
  arguments: string;
}

export interface SyncFacts {
  isDir: boolean;
  size: number;
  /** Seconds since the Unix epoch. */
  modified: number | null;
  permissions?: number;
  isLink: boolean;
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
  /** The change as rsync's --itemize-changes shows it, such as `<f.st......`. */
  itemized: string;
}

export interface SyncCounts {
  unchanged: number;
  /** Left alone because the target's copy is newer, or because existing files are kept. */
  kept: number;
  /** Only on the target, and kept because deleting is off. */
  extraOnTarget: number;
  excluded: number;
  passedOver: number;
  /** Only on the target, and kept by a protect rule. */
  protected: number;
  /** New, and not created because only existing files are updated. */
  notCreated: number;
  /** Outside the minimum or maximum size. */
  sizeLimited: number;
  /** Deletions past the --max-delete limit. */
  heldBack: number;
  /** Folders left out because no file would end up in them. */
  pruned: number;
}

export interface SyncPlanView {
  planId: string;
  localRoot: string;
  remoteRoot: string;
  items: SyncItem[];
  counts: SyncCounts;
  /** A sample of the paths that were passed over. */
  passedOver: string[];
  /** Notes on the options and the plan. */
  notes: string[];
  /** The options as the rsync command line they amount to. */
  arguments: string;
  dryRun: boolean;
  /** The folder synchronized to does not exist yet and is created. */
  createsTarget: boolean;
  /** Unchanged source files that are removed, with --remove-source-files. */
  removesUnchanged: number;
}

export interface SyncProgress {
  requestId: string;
  stage: "listing" | "comparing";
  localEntries: number;
  remoteEntries: number;
  compared: number;
  toCompare: number;
}

/** One of two compared folders. The backend knows itself whether its listings' times are coarse. */
export interface FolderRef {
  location: FileLocation;
  path: string;
}

export interface FolderCompareRequest {
  /** Names the comparison for `SyncProgress` events and for `sync_cancel`. */
  requestId: string;
  left: FolderRef;
  right: FolderRef;
  sizeOnly: boolean;
  timeToleranceSecs: number;
  ignoreHourOffsets: boolean;
  includeHidden: boolean;
  ignoreCase: boolean;
}

/** How the files in one folder that both sides have compare, at any depth below it. */
export interface FolderSummary {
  /** Below the compared folders, with `/` between names; in lower case when case is ignored. */
  path: string;
  identical: number;
  newerLeft: number;
  newerRight: number;
  onlyLeft: number;
  onlyRight: number;
  different: number;
  /** Folders that could not be read, links that lead nowhere and special files. */
  notCompared: number;
}

export interface FolderComparison {
  /** Shallowest first. */
  folders: FolderSummary[];
  /** More folders were compared than are reported; the deepest are left out. */
  truncated: boolean;
  leftEntries: number;
  rightEntries: number;
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
  runId: string;
  /** Nothing was changed; the counts are what would have happened. */
  dryRun: boolean;
  queuedFiles: number;
  queuedBytes: number;
  deleted: number;
  /** Deletions that wait until the queued files are copied. */
  deletingLater: number;
  createdFolders: number;
  createdLinks: number;
  /** Files whose time, permissions or owner were set without copying. */
  updated: number;
  /** Unchanged files removed from the source. */
  removedSources: number;
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
  bypassProxy?: boolean;
  /** Another saved connection this one is reached through. */
  jumpConnectionId?: string | null;
  /** The rsync program on the server, when it is not `rsync` on its PATH. */
  rsyncPath?: string | null;
  /** rsync arguments this site's synchronizations start from, over the defaults. */
  syncArguments?: string | null;
  s3?: S3Options;
}

/** A client whose saved sites can be imported. Mirrors `ImportSource` in import/mod.rs. */
export type ImportSource =
  | "fileZilla"
  | "winScp"
  | "smartFtp"
  | "cyberduck"
  | "putty"
  | "mobaXterm"
  | "openSsh"
  | "fileManager"
  | "urlList";

export interface ImportSourceInfo {
  source: ImportSource;
  /** The files, folders and registry keys with its sites on this computer. */
  found: string[];
  /** Where the client keeps its sites on this system, when none were found. */
  places: string[];
  /** Why what was found cannot be read, and what to do instead. */
  note?: string | null;
}

export interface ImportEntry {
  /** Not saved yet: no id, and the name has no folder in it. */
  connection: SavedConnection;
  /** The folders the site was in, outermost first. */
  folder: string[];
  /** A password came with it; it stays in the backend until the import. */
  hasPassword: boolean;
  /** The entry this one is reached through. */
  jump?: number | null;
  /** A saved connection to the same server and account. */
  existingId?: string | null;
  /** What did not carry over. */
  notes: string[];
}

export interface ImportSkipped {
  name: string;
  reason: string;
}

export interface ImportScan {
  scanId: string;
  source: ImportSource;
  /** The files and registry keys read. */
  origins: string[];
  entries: ImportEntry[];
  skipped: ImportSkipped[];
  warnings: string[];
}

export interface ImportRequest {
  scanId: string;
  /** Entry indexes; the jump hosts they go through come along. */
  selected: number[];
  savePasswords: boolean;
  /** Puts the folders a site was in before its name. */
  folderNames: boolean;
}

export interface ImportOutcome {
  imported: number;
  /** Imported without their password, since the system keychain refused it. */
  withoutPasswords: number;
  failures: string[];
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

/** Sent when the transfer queue runs out of work. */
export interface QueueFinished {
  done: number;
  failed: number;
  skipped: number;
  bytes: number;
  elapsedMillis: number;
  /** Jobs still waiting because they are paused; the queue is not really finished. */
  paused: number;
}

export type PowerAction = "lock" | "sleep" | "hibernate" | "logOff" | "shutDown";

export interface CommandRequest {
  /** Names this run for its output events and for stopping it. */
  runId: string;
  sessionId: string;
  command: string;
  /** The folder to run the command in; the login folder when absent. */
  directory?: string | null;
}

export interface OutputChunk {
  runId: string;
  stream: "stdout" | "stderr";
  text: string;
}

export interface CommandResult {
  /** Absent when the server did not say, for example after a stop. */
  exitStatus: number | null;
  /** The signal that ended the command, if one did. */
  signal: string | null;
  stopped: boolean;
  elapsedMillis: number;
}

export type Trigger =
  /** Seconds since the Unix epoch. */
  | { type: "once"; at: number }
  /** From `start` (seconds since the Unix epoch), then every `minutes`. */
  | { type: "every"; minutes: number; start: number }
  /** At a local time of day on the chosen weekdays, Monday being 0. */
  | { type: "daily"; minuteOfDay: number; weekdays: number[] };

export type TaskAction =
  | ({
      type: "sync";
      connectionId: string;
      localPath: string;
      remotePath: string;
      direction: SyncDirection;
      /** rsync arguments applied over the options. */
      arguments: string;
    } & SyncOptions)
  | { type: "command"; connectionId: string; command: string; directory?: string | null };

export interface LastRun {
  /** Seconds since the Unix epoch. */
  started: number;
  finished: number;
  succeeded: boolean;
  message: string;
}

export interface ScheduledTask {
  /** Empty for a task not saved yet. */
  id: string;
  name: string;
  enabled: boolean;
  trigger: Trigger;
  action: TaskAction;
  /** Runs once at the next start when Poros was closed at the time. */
  runMissed: boolean;
  /** Seconds since the Unix epoch. Worked out by the backend. */
  nextRun?: number | null;
  lastRun?: LastRun | null;
}

export interface TaskView extends ScheduledTask {
  running: boolean;
}

/** Where files are: this computer or the server of a session. */
export type FileLocation = { kind: "local" } | { kind: "remote"; sessionId: string };

/** Mirrored in src-tauri/src/file_ops/archive/formats.rs. */
export type ArchiveFormat =
  | "zip"
  | "sevenZip"
  | "rar"
  | "tar"
  | "tarGzip"
  | "tarBzip2"
  | "tarXz"
  | "tarZstd"
  | "tarLzma"
  | "gzip"
  | "bzip2"
  | "xz"
  | "zstd"
  | "lzma"
  | "iso"
  | "cab";

/** Mirrored in src-tauri/src/file_ops/archive/tools.rs. */
export type ArchiveToolKind =
  | "sevenZip"
  | "sevenZipStandalone"
  | "winRar"
  | "unRar"
  | "unrarFree"
  | "unar"
  | "bsdTar"
  | "gnuTar"
  | "unzip"
  | "gzip"
  | "bzip2"
  | "xz"
  | "zstd";

/** An extracting program found on this computer. */
export interface InstalledArchiveTool {
  kind: ArchiveToolKind;
  name: string;
  path: string;
  formats: ArchiveFormat[];
}

/** Into the folder the archive is in, or a new folder beside it named after it. */
export type ExtractInto = "here" | "folder";

/** The program that extracts on this computer. */
export type Extractor =
  | { kind: "auto" }
  | { kind: "installed"; path: string }
  | { kind: "custom"; program: string; arguments: string };

export interface ExtractRequest {
  operationId: string;
  location: FileLocation;
  /** The archive. */
  path: string;
  into: ExtractInto;
  extractor?: Extractor;
}

export interface ExtractSummary {
  /** The new folder, the unpacked file, the one item extracted here, or the folder. */
  destination: string;
  /** The folder the archive is in. */
  folder: string;
  /** What now stands beside the archive: the new folder, or each item extracted here. */
  placed: string[];
  format: ArchiveFormat;
  /** The program that extracted it. */
  tool: string;
  /** What the program said while it succeeded. */
  warning?: string;
}

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
  /** The name the one item takes in the target folder, when not its own. */
  targetName?: string;
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
  /** How many entries the operation covers, as far as it has found them. */
  total?: number;
  /** More entries may yet be found, so `total` can still grow. */
  counting?: boolean;
  failed?: number;
  lastFailure?: OperationFailure;
}

export interface DeleteRequest {
  operationId: string;
  sessionId: string;
  paths: string[];
}

export interface DeleteSummary {
  /** Entries removed, counting everything that was inside folders. */
  deleted: number;
  /** Entries that could not be removed, of which `failures` holds the first ones. */
  failed: number;
  failures: OperationFailure[];
  methods: OperationMethod[];
}

/** One item's name in its folder, and the name it is to have. */
export interface PlannedRename {
  from: string;
  to: string;
}

/**
 * Renames that go through together or not at all, in an order in which each new name is free
 * by the time it is needed. Names that trade places form a ring with no such order: the first
 * item of the ring then waits under `temporary` while the others move, and takes its name last.
 */
export interface RenameGroup {
  temporary?: string;
  renames: PlannedRename[];
}

export interface RenameRequest {
  operationId: string;
  location: FileLocation;
  /** The folder every item is in. */
  directory: string;
  /** Run in this order. */
  groups: RenameGroup[];
}

export interface RenamedItem extends PlannedRename {
  /** Where the item is now. */
  path: string;
}

export interface RenameFailure extends PlannedRename {
  message: string;
}

/** An item left under its temporary name, because it could not be moved back either. */
export interface StrandedItem extends PlannedRename {
  /** The name it has now. */
  temporary: string;
  /** Why it could not be moved back. */
  message: string;
}

export interface RenameSummary {
  /** The items that have their new name, in the order they got it. */
  renamed: RenamedItem[];
  /** The rename that stopped the rest, and why. The items after it keep their names. */
  failure?: RenameFailure;
  stranded?: StrandedItem;
  /** Stopped by the user before every item was renamed. */
  cancelled: boolean;
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

/** A file the editor opens: on this computer or on the server of a session. */
export type DocumentLocation =
  { side: "local"; path: string } | { side: "remote"; sessionId: string; path: string };

export interface DocumentInfo {
  id: string;
  name: string;
  path: string;
  /** "Local", or the label of the server the file is on. */
  origin: string;
}

export type TextEncoding = "utf8" | "utf8Bom" | "utf16Le" | "utf16Be" | "latin1";
export type TextLineEnding = "lf" | "crlf";

export interface FileStamp {
  size: number;
  /** Milliseconds since the Unix epoch. */
  modified: number | null;
}

export interface TextDocument {
  /** Lines end in `\n` only; `lineEnding` says what the file uses. */
  text: string;
  encoding: TextEncoding;
  lineEnding: TextLineEnding;
  stamp: FileStamp;
}

export interface SaveRequest {
  documentId: string;
  text: string;
  encoding: TextEncoding;
  lineEnding: TextLineEnding;
  /** The file as last read or saved; null overwrites whatever is there. */
  expected: FileStamp | null;
}

export type SaveOutcome =
  | { status: "saved"; stamp: FileStamp }
  /** Something else changed or removed the file; `current` is null when removed. */
  | { status: "changed"; current: FileStamp | null };

export interface TerminalInfo {
  id: string;
  /** The server's label. */
  label: string;
}

export type TerminalEvent = { type: "output"; data: string } | { type: "exit"; message: string };

/** How the system draws a window's corners, where it draws them. */
export type WindowCorners = "square" | "small" | "round";

/** What the preview of a dragged tab shows. */
export interface PreviewContent {
  label: string;
  kind: "local" | "remote" | "welcome" | "editor" | "terminal";
  /** How far the pane has turned into the window it opens as, from 0 to 1. */
  toward: number;
  /** The drag whose picture of the pane to show. */
  drag: number;
  /** How the pane docks where the pointer is, which its title bar names. */
  drop: "left" | "right" | "top" | "bottom" | "center" | null;
  /** The pane's size, in logical pixels. */
  width: number;
  height: number;
  /**
   * The room around the pane, in logical pixels, where a see-through preview draws its shadow;
   * none where the preview cannot be see-through.
   */
  room: number;
}

/** What the drag preview can do on this system. */
export interface PreviewSupport {
  /** It can follow the pointer. */
  available: boolean;
  /** It can be see-through, and so draw the pane as it looks in its window, shadow and all. */
  seeThrough: boolean;
}

/** Where the drag preview sits on screen and what it shows. */
export interface PreviewPlacement {
  content: PreviewContent;
  /** The top left corner, in physical pixels. */
  x: number;
  y: number;
  /** The size, in logical pixels. */
  width: number;
  height: number;
}

/**
 * How far downloading and installing an update has got. `total` is null when the server does not
 * say how large the download is.
 */
export type UpdateProgress =
  { stage: "downloading"; received: number; total: number | null } | { stage: "installing" };

/** What to open a file with. Mirrored in src-tauri/src/external_editor/mod.rs. */
export type OpenWith =
  | { kind: "default" }
  /** A program from Settings > Interface > Open with. */
  | { kind: "program"; id: string }
  /** A program picked for this one file. */
  | { kind: "custom"; command: string };

export interface ExternalOpenRequest {
  sessionId: string;
  path: string;
  with: OpenWith;
  /** The user agreed to open a file of a type that runs, in the program named. */
  confirmedExecutable?: boolean;
}

export type ExternalEditState =
  | "downloading"
  /** Open in its program, with nothing to upload. */
  | "watching"
  /** Saved, and waiting for the user to say it should be uploaded. */
  | "changed"
  /** An upload waits for the server or for the transfer queue. */
  | "waiting"
  /** The server's copy changed since it was downloaded. */
  | "conflict"
  | "uploading"
  | "failed";

/** The two copies of a file whose server copy changed. */
export interface ExternalEditConflict {
  serverSize: number | null;
  /** Seconds since the Unix epoch. */
  serverModified: number | null;
  /** The file is gone from the server. */
  deleted: boolean;
  localSize: number;
  /** Milliseconds since the Unix epoch. */
  localModified: number | null;
}

/** A server file open in another program. */
export interface ExternalEdit {
  id: string;
  name: string;
  remotePath: string;
  /** The session uploads go through now, which changes when a tab reconnects. */
  sessionId: string;
  /** The server's label. */
  server: string;
  inVault: boolean;
  /** Saves are still uploaded once the tab is closed; a vault's are not, as it locks. */
  outlivesSession: boolean;
  localPath: string;
  /** What the file was opened with; empty for a file taken up from an earlier run. */
  program: string;
  state: ExternalEditState;
  transferred: number;
  size: number;
  /** Why an upload failed or waits. */
  detail?: string;
  conflict?: ExternalEditConflict;
  /** Downloaded but not opened, because its content is a program's. */
  blocked: boolean;
  uploads: number;
  /** Milliseconds since the Unix epoch. */
  lastUpload?: number;
}

export type ExternalOpenOutcome =
  | { kind: "opening"; edit: ExternalEdit }
  /** The file was open already; its working copy opens again. */
  | { kind: "alreadyOpen"; edit: ExternalEdit }
  /** A file on this computer was handed to its program; nothing follows it. */
  | { kind: "opened" }
  /** A file of a type that runs: nothing was downloaded until the user names an editor. */
  | { kind: "needsConfirmation"; name: string };

/** Told once, by the window in use. */
export type ExternalEditNotice =
  | { kind: "uploaded"; editId: string; name: string; renamedTo: string | null }
  | { kind: "uploadFailed"; editId: string; name: string; message: string }
  | { kind: "conflict"; editId: string; name: string }
  | { kind: "saved"; editId: string; name: string }
  | { kind: "blocked"; editId: string; name: string }
  | { kind: "openFailed"; name: string; message: string }
  | { kind: "ended"; name: string; message: string };

export interface ExternalEditsUpdate {
  edits: ExternalEdit[];
  notice?: ExternalEditNotice;
}

/** What to do when the server's copy changed. */
export type ExternalResolution = "overwrite" | "keepBoth" | "cancel";

/** A working copy an earlier run left with changes the server never got. */
export interface ExternalLeftover {
  id: string;
  name: string;
  remotePath: string;
  /** The server's label. */
  server: string;
  /** The server's own protocol, also for a file in a vault on it. */
  protocol: Protocol;
  host: string;
  port: number;
  username: string;
  vaultRoot?: string;
  size: number;
  /** Milliseconds since the Unix epoch. */
  modified?: number;
}

/** Mirrored in src-tauri/src/search/pattern.rs. */
export type SearchPatternKind = "text" | "regex";

/** Mirrored in src-tauri/src/search/criteria.rs. */
export type SearchKind = "any" | "files" | "folders";

/** Mirrored in src-tauri/src/search/mod.rs. */
export interface SearchRequest {
  searchId: string;
  location: FileLocation;
  folder: string;
  /** Empty finds every name. */
  pattern: string;
  patternKind: SearchPatternKind;
  caseSensitive: boolean;
  kind: SearchKind;
  minSize: number | null;
  maxSize: number | null;
  /** Seconds since the Unix epoch, both ends included. */
  modifiedAfter: number | null;
  modifiedBefore: number | null;
  /** 1 looks only at what is directly in the folder; null goes all the way down. */
  maxDepth: number | null;
  /** Whether hidden folders are searched too. */
  hidden: boolean;
  followLinks: boolean;
  limit: number | null;
}

export interface SearchReport {
  /** What was found since the last report. */
  entries: FileEntry[];
  /** How many folders were looked at so far. */
  folders: number;
  /** The folder being looked at. */
  current: string;
  /** Folders that could not be read since the last report. */
  unreadable: OperationFailure[];
  /**
   * The start folder as its listing names it, once, when not as asked for: a link to a folder,
   * `/var` that is `/private/var`, a short name on Windows. Results are below this name.
   */
  start?: string;
}

export type SearchEngine = "find" | "walker";

export type SearchOutcome = "finished" | "stopped" | "capped";

export interface SearchSummary {
  engine: SearchEngine;
  outcome: SearchOutcome;
  found: number;
  folders: number;
  /** How many folders could not be read, also those no report listed. */
  unreadable: number;
}

/** What became of results a search view still shows. */
export interface SearchRecheck {
  /** The items still there, as their folders list them now. */
  entries: FileEntry[];
  /** The paths nothing is at any more. */
  missing: string[];
}
