// Mirrors src-tauri/src/settings.rs. The backend owns and clamps the transfer and connection
// sections; the interface, appearance, log and sync sections are stored as given, so they are
// checked here.

import { DETAIL_COLUMNS, type DetailColumn } from "./columns";
import { SORT_KEYS, type SortSpec } from "./sort";
import type { CompareMode, ExistsAction, LogLevel, SyncDirection } from "./types";

export const MAX_WORKERS = 16;
export const MAX_RECONNECT_MINUTES = 24 * 60;
export const SOCKET_BUFFER_LIMITS = { min: 4, max: 64 * 1024 };

export interface TransferSettings {
  workers: number;
  autoStart: boolean;
  segmented: boolean;
  segmentThresholdMib: number;
  maxSegments: number;
  requestSizeKib: number;
  requestsInFlight: number;
  /** KiB per second; zero means unlimited. */
  uploadLimitKib: number;
  downloadLimitKib: number;
  existsAction: ExistsAction;
  preserveTimestamps: boolean;
  preservePermissions: boolean;
  retryAttempts: number;
  retryDelaySecs: number;
  /** How long transfers wait for a server whose connection dropped; zero treats it as a failure. */
  reconnectMinutes: number;
  keepCompleted: boolean;
  /** Saves unfinished transfers so they come back, paused, at the next start. */
  keepQueue: boolean;
  /** Writes under a temporary name and renames into place once complete. */
  temporaryFiles: boolean;
  /** Compares checksums of both copies after each file, using the server's own command. */
  verifyChecksums: boolean;
  flushToDisk: boolean;
  separateConnections: boolean;
  logEachFile: boolean;
  /** Sends only the changed parts of files the other side already has, through rsync. */
  deltaTransfers: boolean;
  /** Smaller files are copied whole. */
  deltaThresholdKib: number;
  /** The command that starts rsync on the server. */
  rsyncPath: string;
}

export interface ConnectionSettings {
  timeoutSecs: number;
  keepaliveSecs: number;
  compression: boolean;
  /** Server tabs whose connection drops reconnect on their own, for `reconnectMinutes`. */
  autoReconnect: boolean;
  /** Lets the system size the TCP receive buffer; off uses `receiveBufferKib`. */
  autoTuneReceiveBuffer: boolean;
  receiveBufferKib: number;
  /** Lets the system size the TCP send buffer; off uses `sendBufferKib`. */
  autoTuneSendBuffer: boolean;
  sendBufferKib: number;
}

export type DoubleClickAction = "transfer" | "nothing";
export type DateFormat = "minutes" | "seconds" | "locale";
export const DATE_FORMATS: DateFormat[] = ["minutes", "seconds", "locale"];

export interface InterfaceSettings {
  doubleClickFile: DoubleClickAction;
  showHiddenFiles: boolean;
  confirmCloseWithTransfers: boolean;
  /** Adds every new quick connection to the saved connections. */
  saveQuickConnections: boolean;
  rememberLayout: boolean;
  /** Off: Poros draws its own window buttons in the top bar. */
  systemTitleBar: boolean;
  foldersFirst: boolean;
  /** The order new panes start with: the last one picked in any pane. */
  sort: SortSpec;
  dateFormat: DateFormat;
  hiddenColumns: DetailColumn[];
}

export interface AppearanceSettings {
  /** A built-in theme id (`builtin:...`) or the file name of a user theme. */
  theme: string;
  fontSize: number;
  /** A font family name; empty uses the theme's font or the default. */
  uiFont: string;
  monoFont: string;
  /** Corner radius in pixels; null uses the theme's. */
  radius: number | null;
  rowHeight: number;
  stripedRows: boolean;
}

export interface LogSettings {
  maxLines: number;
  timestamps: boolean;
  wrapLines: boolean;
  levels: Record<LogLevel, boolean>;
}

/** What a new folder synchronization starts with: the options used last. */
export interface SyncSettings {
  direction: SyncDirection;
  compare: CompareMode;
  deleteExtraneous: boolean;
  skipNewerOnTarget: boolean;
  ignoreExisting: boolean;
  timeToleranceSecs: number;
  /** rsync-style patterns, one per line. */
  excludes: string;
}

export interface Settings {
  transfers: TransferSettings;
  connection: ConnectionSettings;
  interface: InterfaceSettings;
  appearance: AppearanceSettings;
  log: LogSettings;
  sync: SyncSettings;
}

export const FONT_SIZE_LIMITS = { min: 10, max: 20 };
export const RADIUS_LIMITS = { min: 0, max: 16 };
export const ROW_HEIGHT_LIMITS = { min: 18, max: 36 };
const COMPACT_ROW_HEIGHT = 21;
export const LOG_LINE_LIMITS = { min: 200, max: 50000 };
export const TIME_TOLERANCE_LIMITS = { min: 0, max: 24 * 60 * 60 };
export const SYNC_DIRECTIONS: SyncDirection[] = ["upload", "download", "both"];
export const COMPARE_MODES: CompareMode[] = ["sizeAndTime", "sizeOnly", "checksum", "always"];

export const DEFAULT_SETTINGS: Settings = {
  transfers: {
    workers: 4,
    autoStart: true,
    segmented: true,
    segmentThresholdMib: 32,
    maxSegments: 4,
    requestSizeKib: 32,
    requestsInFlight: 64,
    uploadLimitKib: 0,
    downloadLimitKib: 0,
    existsAction: "ask",
    preserveTimestamps: true,
    preservePermissions: false,
    retryAttempts: 3,
    retryDelaySecs: 5,
    reconnectMinutes: 10,
    keepCompleted: true,
    keepQueue: true,
    temporaryFiles: true,
    verifyChecksums: false,
    flushToDisk: false,
    separateConnections: true,
    logEachFile: false,
    deltaTransfers: true,
    deltaThresholdKib: 1024,
    rsyncPath: "rsync",
  },
  connection: {
    timeoutSecs: 20,
    keepaliveSecs: 30,
    compression: false,
    autoReconnect: true,
    autoTuneReceiveBuffer: true,
    receiveBufferKib: 128,
    autoTuneSendBuffer: true,
    sendBufferKib: 128,
  },
  interface: {
    doubleClickFile: "transfer",
    showHiddenFiles: false,
    confirmCloseWithTransfers: true,
    saveQuickConnections: false,
    rememberLayout: true,
    systemTitleBar: false,
    foldersFirst: true,
    sort: { key: "name", direction: 1 },
    dateFormat: "minutes",
    hiddenColumns: [],
  },
  appearance: {
    theme: "builtin:system",
    fontSize: 13,
    uiFont: "",
    monoFont: "",
    radius: null,
    rowHeight: 24,
    stripedRows: false,
  },
  log: {
    maxLines: 5000,
    timestamps: true,
    wrapLines: true,
    levels: { info: true, warn: true, error: true, server: true },
  },
  sync: {
    direction: "upload",
    compare: "sizeAndTime",
    deleteExtraneous: false,
    skipNewerOnTarget: false,
    ignoreExisting: false,
    timeToleranceSecs: 2,
    excludes: "",
  },
};

type Section = Record<string, unknown>;

function asSection(value: unknown): Section {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Section) : {};
}

/** Keeps each stored value whose type matches the default's, so old or hand-edited files load. */
function mergeSection<T extends object>(defaults: T, stored: unknown): T {
  const source = asSection(stored);
  const merged = { ...defaults } as Record<string, unknown>;
  for (const [key, fallback] of Object.entries(defaults)) {
    const value = source[key];
    if (Array.isArray(fallback)) {
      if (Array.isArray(value)) merged[key] = value;
    } else if (fallback !== null && typeof fallback === "object") {
      merged[key] = mergeSection(fallback as object, value);
    } else if (typeof value === typeof fallback && !Number.isNaN(value)) {
      merged[key] = value;
    }
  }
  return merged as T;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function sanitizeSettings(stored: unknown): Settings {
  const source = asSection(stored);
  const settings: Settings = {
    transfers: mergeSection(DEFAULT_SETTINGS.transfers, source.transfers),
    connection: mergeSection(DEFAULT_SETTINGS.connection, source.connection),
    interface: mergeSection(DEFAULT_SETTINGS.interface, source.interface),
    appearance: mergeSection(DEFAULT_SETTINGS.appearance, source.appearance),
    log: mergeSection(DEFAULT_SETTINGS.log, source.log),
    sync: mergeSection(DEFAULT_SETTINGS.sync, source.sync),
  };
  const options = settings.interface;
  if (!["transfer", "nothing"].includes(options.doubleClickFile)) {
    options.doubleClickFile = DEFAULT_SETTINGS.interface.doubleClickFile;
  }
  if (!DATE_FORMATS.includes(options.dateFormat)) {
    options.dateFormat = DEFAULT_SETTINGS.interface.dateFormat;
  }
  if (!SORT_KEYS.includes(options.sort.key) || ![1, -1].includes(options.sort.direction)) {
    options.sort = DEFAULT_SETTINGS.interface.sort;
  }
  options.hiddenColumns = DETAIL_COLUMNS.filter((column) => options.hiddenColumns.includes(column));

  const appearance = settings.appearance;
  const storedAppearance = asSection(source.appearance);
  appearance.fontSize = clamp(appearance.fontSize, FONT_SIZE_LIMITS.min, FONT_SIZE_LIMITS.max);
  appearance.uiFont = appearance.uiFont.trim();
  appearance.monoFont = appearance.monoFont.trim();
  appearance.radius =
    typeof storedAppearance.radius === "number" && Number.isFinite(storedAppearance.radius)
      ? clamp(storedAppearance.radius, RADIUS_LIMITS.min, RADIUS_LIMITS.max)
      : null;
  // Before row heights could be picked, a switch chose between two.
  if (storedAppearance.rowHeight === undefined && storedAppearance.compactRows === true) {
    appearance.rowHeight = COMPACT_ROW_HEIGHT;
  }
  appearance.rowHeight = clamp(appearance.rowHeight, ROW_HEIGHT_LIMITS.min, ROW_HEIGHT_LIMITS.max);
  settings.log.maxLines = clamp(settings.log.maxLines, LOG_LINE_LIMITS.min, LOG_LINE_LIMITS.max);

  const sync = settings.sync;
  if (!SYNC_DIRECTIONS.includes(sync.direction)) sync.direction = DEFAULT_SETTINGS.sync.direction;
  if (!COMPARE_MODES.includes(sync.compare)) sync.compare = DEFAULT_SETTINGS.sync.compare;
  sync.timeToleranceSecs = clamp(
    sync.timeToleranceSecs,
    TIME_TOLERANCE_LIMITS.min,
    TIME_TOLERANCE_LIMITS.max,
  );
  return settings;
}
