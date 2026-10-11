// Mirrors src-tauri/src/settings.rs. The backend owns and clamps the transfer, connection and
// cloud sections; the interface, appearance, log, sync, updates and automation sections are
// stored as given, so they are checked here.

import { DETAIL_COLUMNS, type DetailColumn } from "./columns";
import { DEFAULT_SYNC_OPTIONS, legacySyncOptions, sanitizeSyncOptions } from "./rsyncOptions";
import { SORT_KEYS, type SortSpec } from "./sort";
import type { ExistsAction, LogLevel, PowerAction, SyncDirection, SyncOptions } from "./types";

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
  /** Copies between two FTP servers go directly from one to the other when both allow it. */
  fxp: boolean;
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
  proxy: ProxySettings;
}

export type ProxyKind = "none" | "socks5" | "socks4" | "http";
export const PROXY_KINDS: ProxyKind[] = ["none", "socks5", "socks4", "http"];

/** The proxy every SFTP connection goes through unless it is set to connect directly. */
export interface ProxySettings {
  kind: ProxyKind;
  host: string;
  port: number;
  username: string;
  /** The password is in the system keychain. Only the backend changes this. */
  hasPassword: boolean;
  /** The proxy looks up server names, so this computer's DNS never sees them. */
  remoteDns: boolean;
}

export function defaultProxyPort(kind: ProxyKind): number {
  return kind === "http" ? 8080 : 1080;
}

/** The apps Poros signs in to cloud storage with; empty uses the one built into the release. */
export interface CloudSettings {
  googleClientId: string;
  googleClientSecret: string;
  microsoftClientId: string;
  /** `common`, `consumers`, `organizations` or a directory (tenant) ID; empty means `common`. */
  microsoftTenant: string;
}

export type DoubleClickAction = "transfer" | "edit" | "nothing";
const DOUBLE_CLICK_ACTIONS: DoubleClickAction[] = ["transfer", "edit", "nothing"];
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

/** What a new folder synchronization starts with, before a site's own rsync arguments. */
export interface SyncSettings {
  direction: SyncDirection;
  options: SyncOptions;
  /** rsync arguments applied over the options, typed as rsync takes them. */
  arguments: string;
}

export interface UpdateSettings {
  /** Looks for a newer release each time the main window opens. */
  checkOnStart: boolean;
  /** A release the user chose to skip; checks at start-up stay quiet about it. */
  skippedVersion: string;
}

/** What happens when the transfer queue finishes: `close` quits Poros. */
export type WhenDoneAction = "nothing" | "close" | PowerAction | "runCommand";
export const WHEN_DONE_ACTIONS: WhenDoneAction[] = [
  "nothing",
  "close",
  "lock",
  "sleep",
  "hibernate",
  "logOff",
  "shutDown",
  "runCommand",
];

/** A command offered in the server's context menu. */
export interface ServerCommand {
  id: string;
  name: string;
  /** Run through the server's shell in the folder shown, with placeholders filled in. */
  command: string;
  /** Opens a window with the output; off runs it quietly and logs what it printed. */
  showOutput: boolean;
  /** Reloads the server's panes when the command finishes. */
  refresh: boolean;
}

export interface AutomationSettings {
  whenDone: WhenDoneAction;
  /** Goes back to doing nothing after the action has happened once. */
  whenDoneOnce: boolean;
  /** Run on this computer for `runCommand`. */
  whenDoneCommand: string;
  notify: boolean;
  notifyOnlyInBackground: boolean;
  sound: boolean;
  commands: ServerCommand[];
}

export interface Settings {
  transfers: TransferSettings;
  connection: ConnectionSettings;
  cloud: CloudSettings;
  interface: InterfaceSettings;
  appearance: AppearanceSettings;
  log: LogSettings;
  sync: SyncSettings;
  updates: UpdateSettings;
  automation: AutomationSettings;
}

export const FONT_SIZE_LIMITS = { min: 10, max: 20 };
export const RADIUS_LIMITS = { min: 0, max: 16 };
export const ROW_HEIGHT_LIMITS = { min: 18, max: 36 };
const COMPACT_ROW_HEIGHT = 21;
export const LOG_LINE_LIMITS = { min: 200, max: 50000 };
export const SYNC_DIRECTIONS: SyncDirection[] = ["upload", "download", "both"];

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
    fxp: true,
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
    proxy: {
      kind: "none",
      host: "",
      port: 1080,
      username: "",
      hasPassword: false,
      remoteDns: true,
    },
  },
  cloud: {
    googleClientId: "",
    googleClientSecret: "",
    microsoftClientId: "",
    microsoftTenant: "",
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
    options: DEFAULT_SYNC_OPTIONS,
    arguments: "",
  },
  updates: {
    checkOnStart: true,
    skippedVersion: "",
  },
  automation: {
    whenDone: "nothing",
    whenDoneOnce: true,
    whenDoneCommand: "",
    notify: true,
    notifyOnlyInBackground: true,
    sound: false,
    commands: [],
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
  const transfers = mergeSection(DEFAULT_SETTINGS.transfers, source.transfers);
  const settings: Settings = {
    transfers,
    connection: mergeSection(DEFAULT_SETTINGS.connection, source.connection),
    cloud: mergeSection(DEFAULT_SETTINGS.cloud, source.cloud),
    interface: mergeSection(DEFAULT_SETTINGS.interface, source.interface),
    appearance: mergeSection(DEFAULT_SETTINGS.appearance, source.appearance),
    log: mergeSection(DEFAULT_SETTINGS.log, source.log),
    sync: sanitizeSync(source.sync, transfers),
    updates: mergeSection(DEFAULT_SETTINGS.updates, source.updates),
    automation: mergeSection(DEFAULT_SETTINGS.automation, source.automation),
  };
  const options = settings.interface;
  if (!DOUBLE_CLICK_ACTIONS.includes(options.doubleClickFile)) {
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

  settings.updates.skippedVersion = settings.updates.skippedVersion.trim();

  const proxy = settings.connection.proxy;
  if (!PROXY_KINDS.includes(proxy.kind)) proxy.kind = DEFAULT_SETTINGS.connection.proxy.kind;
  if (!Number.isInteger(proxy.port) || proxy.port < 1 || proxy.port > 65535) {
    proxy.port = defaultProxyPort(proxy.kind);
  }

  const automation = settings.automation;
  if (!WHEN_DONE_ACTIONS.includes(automation.whenDone)) {
    automation.whenDone = DEFAULT_SETTINGS.automation.whenDone;
  }
  automation.commands = sanitizeCommands(automation.commands);
  return settings;
}

/**
 * The synchronization defaults, read from the options saved before rsync's full set too. Those
 * synchronizations kept times and permissions as the transfer settings said, so the new -t and
 * -p start from them.
 */
function sanitizeSync(stored: unknown, transfers: TransferSettings): SyncSettings {
  const source = asSection(stored);
  const direction = source.direction as SyncDirection;
  return {
    direction: SYNC_DIRECTIONS.includes(direction) ? direction : DEFAULT_SETTINGS.sync.direction,
    options:
      "options" in source
        ? sanitizeSyncOptions(source.options)
        : {
            ...legacySyncOptions(source),
            times: transfers.preserveTimestamps,
            perms: transfers.preservePermissions,
          },
    arguments: typeof source.arguments === "string" ? source.arguments : "",
  };
}

/** Keeps each command that has one to run, with a unique id. */
function sanitizeCommands(stored: unknown[]): ServerCommand[] {
  const seen = new Set<string>();
  return stored.flatMap((value, index) => {
    const command = asSection(value);
    if (typeof command.command !== "string" || command.command.trim() === "") return [];
    let id = typeof command.id === "string" && command.id ? command.id : `command-${index + 1}`;
    if (seen.has(id)) id = `${id}-${index + 1}`;
    seen.add(id);
    return [
      {
        id,
        name: typeof command.name === "string" ? command.name : "",
        command: command.command,
        showOutput: typeof command.showOutput === "boolean" ? command.showOutput : true,
        refresh: typeof command.refresh === "boolean" ? command.refresh : false,
      },
    ];
  });
}
