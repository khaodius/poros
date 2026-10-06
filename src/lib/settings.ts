// Mirrors src-tauri/src/settings.rs. The backend owns and clamps the transfer and connection
// sections; the interface, appearance and log sections are stored as given, so they are
// checked here.

import type { ExistsAction, LogLevel } from "./types";

export const MAX_WORKERS = 16;

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
  keepCompleted: boolean;
  separateConnections: boolean;
  logEachFile: boolean;
}

export interface ConnectionSettings {
  timeoutSecs: number;
  keepaliveSecs: number;
  compression: boolean;
}

export type DoubleClickAction = "transfer" | "nothing";

export interface InterfaceSettings {
  doubleClickFile: DoubleClickAction;
  showHiddenFiles: boolean;
  confirmCloseWithTransfers: boolean;
  /** Adds every new quick connection to the saved connections. */
  saveQuickConnections: boolean;
  rememberLayout: boolean;
  /** Off: Poros draws its own window buttons in the top bar. */
  systemTitleBar: boolean;
}

export interface AppearanceSettings {
  /** A built-in theme id (`builtin:...`) or the file name of a user theme. */
  theme: string;
  fontSize: number;
  compactRows: boolean;
}

export interface LogSettings {
  maxLines: number;
  timestamps: boolean;
  wrapLines: boolean;
  levels: Record<LogLevel, boolean>;
}

export interface Settings {
  transfers: TransferSettings;
  connection: ConnectionSettings;
  interface: InterfaceSettings;
  appearance: AppearanceSettings;
  log: LogSettings;
}

export const FONT_SIZES = [11, 12, 13, 14, 15, 16];
export const LOG_LINE_LIMITS = { min: 200, max: 50000 };

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
    keepCompleted: true,
    separateConnections: true,
    logEachFile: false,
  },
  connection: { timeoutSecs: 20, keepaliveSecs: 30, compression: false },
  interface: {
    doubleClickFile: "transfer",
    showHiddenFiles: false,
    confirmCloseWithTransfers: true,
    saveQuickConnections: false,
    rememberLayout: true,
    systemTitleBar: false,
  },
  appearance: { theme: "builtin:system", fontSize: 13, compactRows: false },
  log: {
    maxLines: 5000,
    timestamps: true,
    wrapLines: true,
    levels: { info: true, warn: true, error: true, server: true },
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
    if (fallback !== null && typeof fallback === "object") {
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
  };
  if (!["transfer", "nothing"].includes(settings.interface.doubleClickFile)) {
    settings.interface.doubleClickFile = DEFAULT_SETTINGS.interface.doubleClickFile;
  }
  settings.appearance.fontSize = clamp(
    settings.appearance.fontSize,
    FONT_SIZES[0],
    FONT_SIZES[FONT_SIZES.length - 1],
  );
  settings.log.maxLines = clamp(settings.log.maxLines, LOG_LINE_LIMITS.min, LOG_LINE_LIMITS.max);
  return settings;
}
