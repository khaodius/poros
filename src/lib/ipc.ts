import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  ConnectProfile,
  DirListing,
  DocumentInfo,
  EnqueueRequest,
  ExistsAction,
  FileEntry,
  FileLocation,
  FontFamily,
  HostKeyApproval,
  JobState,
  LogRecord,
  SaveOutcome,
  SaveRequest,
  SavedConnection,
  SessionClosed,
  SessionInfo,
  StoreName,
  SyncPlanView,
  SyncProgress,
  SyncRequest,
  SyncRunRequest,
  SyncRunSummary,
  TerminalEvent,
  TerminalInfo,
  TextDocument,
  ThemeFile,
  TransferList,
  TransferUpdate,
} from "./types";

// Mirrored in src-tauri/src/events.rs.
export const LOG_EVENT = "poros://log";
export const SESSION_CLOSED_EVENT = "poros://session-closed";
export const TRANSFERS_EVENT = "poros://transfers";
export const STORE_CHANGED_EVENT = "poros://store-changed";
export const SYNC_PROGRESS_EVENT = "poros://sync-progress";
/** Sent by a torn-out window to hand a tab back to the main window. */
export const RETURN_TAB_EVENT = "poros://return-tab";
/** Sent by the window a tab is dragged out of to the window under the pointer. */
export const TAB_DRAG_EVENT = "poros://tab-drag";

/** The label of the window Poros starts with; windows torn out of it get others. */
export const MAIN_WINDOW = "main";

export function toAppError(rejection: unknown): AppError {
  if (rejection && typeof rejection === "object" && "kind" in rejection && "message" in rejection) {
    return rejection as AppError;
  }
  const message = rejection instanceof Error ? rejection.message : String(rejection);
  return { kind: "io", message };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (rejection) {
    throw toAppError(rejection);
  }
}

export const local = {
  home: () => call<string>("local_home"),
  roots: () => call<string[]>("local_roots"),
  list: (path: string) => call<DirListing>("local_list", { path }),
  stat: (paths: string[]) => call<FileEntry[]>("local_stat", { paths }),
  mkdir: (parent: string, name: string) => call<string>("local_mkdir", { parent, name }),
  rename: (path: string, newName: string) => call<string>("local_rename", { path, newName }),
  remove: (paths: string[]) => call<void>("local_delete", { paths }),
};

export const remote = {
  connect: (profile: ConnectProfile, hostKeyApproval?: HostKeyApproval) =>
    call<SessionInfo>("connect", { profile, hostKeyApproval: hostKeyApproval ?? null }),
  reconnect: (sessionId: string, hostKeyApproval?: HostKeyApproval) =>
    call<SessionInfo>("reconnect", { sessionId, hostKeyApproval: hostKeyApproval ?? null }),
  adopt: (sessionId: string) => call<SessionInfo>("adopt_session", { sessionId }),
  disconnect: (sessionId: string) => call<void>("disconnect", { sessionId }),
  list: (sessionId: string, path: string) => call<DirListing>("remote_list", { sessionId, path }),
  mkdir: (sessionId: string, parent: string, name: string) =>
    call<string>("remote_mkdir", { sessionId, parent, name }),
  rename: (sessionId: string, path: string, newName: string) =>
    call<string>("remote_rename", { sessionId, path, newName }),
  remove: (sessionId: string, paths: string[]) => call<void>("remote_delete", { sessionId, paths }),
};

export const transfers = {
  enqueue: (request: EnqueueRequest) => call<number>("transfer_enqueue", { request }),
  list: () => call<TransferList>("transfer_list"),
  setPaused: (paused: boolean) => call<void>("transfer_set_paused", { paused }),
  pause: (ids: number[]) => call<void>("transfer_pause", { ids }),
  resume: (ids: number[]) => call<void>("transfer_resume", { ids }),
  remove: (ids: number[]) => call<void>("transfer_remove", { ids }),
  clear: (states: JobState[]) => call<void>("transfer_clear", { states }),
  move: (ids: number[], toTop: boolean) => call<void>("transfer_move", { ids, toTop }),
  resolve: (id: number, action: ExistsAction, applyToAll: boolean) =>
    call<void>("transfer_resolve", { id, action, applyToAll }),
  sessionJobs: (sessionId: string) => call<number>("transfer_session_jobs", { sessionId }),
  pauseSession: (sessionId: string) => call<number>("transfer_pause_session", { sessionId }),
  saveQueue: () => call<void>("transfer_save_queue"),
};

export const sync = {
  compare: (request: SyncRequest) => call<SyncPlanView>("sync_compare", { request }),
  cancel: (requestId: string) => call<void>("sync_cancel", { requestId }),
  /** Forgets a comparison that will not be run. */
  discard: (planId: string) => call<void>("sync_discard", { planId }),
  run: (request: SyncRunRequest) => call<SyncRunSummary>("sync_run", { request }),
};

export const settingsStore = {
  get: () => call<unknown>("settings_get"),
  set: (value: unknown) => call<unknown>("settings_set", { value }),
};

export const savedConnections = {
  list: () => call<SavedConnection[]>("connections_list"),
  /** `secret` replaces the stored password or passphrase; omitted keeps it. */
  save: (connection: SavedConnection, secret?: string | null) =>
    call<SavedConnection>("connections_save", { connection, secret: secret ?? null }),
  remove: (id: string) => call<void>("connections_delete", { id }),
};

export const themeFiles = {
  list: () => call<ThemeFile[]>("themes_list"),
  /** Without an id, the backend picks one from the theme's name. Returns the id. */
  save: (id: string | null, theme: unknown) => call<string>("theme_save", { id, theme }),
  remove: (id: string) => call<void>("theme_delete", { id }),
  importFile: (path: string) => call<string>("theme_import", { path }),
  openFolder: () => call<void>("themes_open_folder"),
};

export const fonts = {
  list: () => call<FontFamily[]>("fonts_list"),
};

export const editor = {
  open: (location: FileLocation) => call<DocumentInfo>("editor_open", { location }),
  /** Reads the file; this window becomes the document's owner. */
  load: (documentId: string) => call<TextDocument>("editor_load", { documentId }),
  /** Takes over a document another window opened, without reading it again. */
  adopt: (documentId: string) => call<void>("editor_adopt", { documentId }),
  save: (request: SaveRequest) => call<SaveOutcome>("editor_save", { request }),
  close: (documentId: string) => call<void>("editor_close", { documentId }),
};

function outputChannel(onEvent: (event: TerminalEvent) => void): Channel<TerminalEvent> {
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = onEvent;
  return channel;
}

export const terminal = {
  open: (
    sessionId: string,
    columns: number,
    rows: number,
    onEvent: (event: TerminalEvent) => void,
  ) =>
    call<TerminalInfo>("terminal_open", {
      sessionId,
      columns,
      rows,
      output: outputChannel(onEvent),
    }),
  /** Shows the terminal's output here from now on, starting with what it showed before. */
  attach: (terminalId: string, onEvent: (event: TerminalEvent) => void) =>
    call<void>("terminal_attach", { terminalId, output: outputChannel(onEvent) }),
  write: (terminalId: string, data: string) => call<void>("terminal_write", { terminalId, data }),
  resize: (terminalId: string, columns: number, rows: number) =>
    call<void>("terminal_resize", { terminalId, columns, rows }),
  restart: (terminalId: string) => call<void>("terminal_restart", { terminalId }),
  close: (terminalId: string) => call<void>("terminal_close", { terminalId }),
};

export const files = {
  saveText: (path: string, contents: string) => call<void>("save_text_file", { path, contents }),
};

export const windows = {
  /** Opens a window showing `layout`; returns its label. */
  open: (layout: unknown, width: number, height: number, x?: number, y?: number) =>
    call<string>("window_open", { layout, width, height, x: x ?? null, y: y ?? null }),
  initialLayout: () => call<unknown>("window_initial_layout"),
};

export const application = {
  /** Starts Poros again, once an update has replaced it. */
  restart: () => call<void>("app_restart"),
};

function subscribe<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (received) => handler(received.payload));
}

export const onLog = (handler: (record: LogRecord) => void) => subscribe(LOG_EVENT, handler);
export const onSessionClosed = (handler: (closed: SessionClosed) => void) =>
  subscribe(SESSION_CLOSED_EVENT, handler);
export const onTransfers = (handler: (update: TransferUpdate) => void) =>
  subscribe(TRANSFERS_EVENT, handler);
export const onStoreChanged = (handler: (store: StoreName) => void) =>
  subscribe(STORE_CHANGED_EVENT, handler);
export const onSyncProgress = (handler: (progress: SyncProgress) => void) =>
  subscribe(SYNC_PROGRESS_EVENT, handler);
