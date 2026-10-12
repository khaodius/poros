import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Update } from "@tauri-apps/plugin-updater";
import type { PanePicture } from "./panePicture";
import type {
  AppError,
  CloudProvider,
  CloudProviderStatus,
  CommandRequest,
  CommandResult,
  CompareRequest,
  Comparison,
  ConnectProfile,
  DirListing,
  DocumentInfo,
  DocumentLocation,
  EnqueueRequest,
  ExistsAction,
  FileDetails,
  FileEntry,
  FileLocation,
  FolderUsage,
  FontFamily,
  HostKeyApproval,
  JobState,
  LogRecord,
  MoveCopyRequest,
  OperationProgress,
  OperationSummary,
  OptionCheck,
  OutputChunk,
  PermissionRequest,
  PermissionSummary,
  PowerAction,
  PreviewContent,
  PreviewPlacement,
  PreviewSupport,
  QueueFinished,
  SaveOutcome,
  SaveRequest,
  SavedConnection,
  ScheduledTask,
  SessionClosed,
  SessionInfo,
  SignedIn,
  StoreName,
  SyncDirection,
  SyncOptions,
  SyncPlanView,
  SyncProgress,
  SyncRequest,
  SyncRunRequest,
  SyncRunSummary,
  TaskView,
  TerminalEvent,
  TerminalInfo,
  TextDocument,
  ThemeFile,
  TransferList,
  TransferUpdate,
  UpdateProgress,
  WindowCorners,
} from "./types";

// Mirrored in src-tauri/src/events.rs.
export const LOG_EVENT = "poros://log";
export const SESSION_CLOSED_EVENT = "poros://session-closed";
export const TRANSFERS_EVENT = "poros://transfers";
export const STORE_CHANGED_EVENT = "poros://store-changed";
export const SYNC_PROGRESS_EVENT = "poros://sync-progress";
export const QUEUE_FINISHED_EVENT = "poros://queue-finished";
export const COMMAND_OUTPUT_EVENT = "poros://command-output";
export const FILE_OPERATION_EVENT = "poros://file-operation";
/** Sent by a torn-out window to hand a tab back to the main window. */
export const RETURN_TAB_EVENT = "poros://return-tab";
/** Sent by the window a tab is dragged out of to the window under the pointer. */
export const TAB_DRAG_EVENT = "poros://tab-drag";
/** Mirrored in src-tauri/src/desktop.rs. Tells the drag preview what to show. */
export const DRAG_PREVIEW_EVENT = "poros://drag-preview";
/** Mirrored in src-tauri/src/desktop.rs. Gives the drag preview the picture of the pane. */
export const DRAG_PREVIEW_PICTURE_EVENT = "poros://drag-preview-picture";
/** Mirrored in src-tauri/src/desktop.rs. Says the drag preview shows a drag's tab. */
export const DRAG_PREVIEW_SHOWN_EVENT = "poros://drag-preview-shown";

/** The label of the window Poros starts with; windows torn out of it get others. */
export const MAIN_WINDOW = "main";
/** The window that previews a tab dragged out onto the desktop. */
export const DRAG_PREVIEW_WINDOW = "drag-preview";

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
  makeFile: (parent: string, name: string) => call<string>("local_make_file", { parent, name }),
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
  makeFile: (sessionId: string, parent: string, name: string) =>
    call<string>("remote_make_file", { sessionId, parent, name }),
  rename: (sessionId: string, path: string, newName: string) =>
    call<string>("remote_rename", { sessionId, path, newName }),
  remove: (sessionId: string, paths: string[]) => call<void>("remote_delete", { sessionId, paths }),
};

export const fileOperations = {
  /** The names among `names` already taken in `directory`. */
  conflicts: (location: FileLocation, names: string[], directory: string) =>
    call<string[]>("files_conflicts", { location, names, directory }),
  moveOrCopy: (request: MoveCopyRequest) =>
    call<OperationSummary>("files_move_or_copy", { request }),
  cancel: (operationId: string) => call<void>("files_cancel", { operationId }),
  details: (sessionId: string, path: string) =>
    call<FileDetails>("files_details", { sessionId, path }),
  measure: (operationId: string, sessionId: string, paths: string[]) =>
    call<FolderUsage>("files_measure", { operationId, sessionId, paths }),
  setPermissions: (request: PermissionRequest) =>
    call<PermissionSummary>("files_set_permissions", { request }),
  compare: (request: CompareRequest) => call<Comparison>("files_compare", { request }),
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
};

export const sync = {
  compare: (request: SyncRequest) => call<SyncPlanView>("sync_compare", { request }),
  cancel: (requestId: string) => call<void>("sync_cancel", { requestId }),
  /** Forgets a comparison that will not be run. */
  discard: (planId: string) => call<void>("sync_discard", { planId }),
  run: (request: SyncRunRequest) => call<SyncRunSummary>("sync_run", { request }),
  /** Applies each layer of rsync arguments over `options` in turn and checks the result. */
  checkOptions: (options: SyncOptions, layers: string[], direction: SyncDirection) =>
    call<OptionCheck>("sync_check_options", { options, arguments: layers, direction }),
};

export const settingsStore = {
  get: () => call<unknown>("settings_get"),
  set: (value: unknown) => call<unknown>("settings_set", { value }),
  /** Keeps the proxy password in the system keychain; an empty one forgets it. */
  setProxyPassword: (password: string) => call<unknown>("proxy_password_set", { password }),
};

export const savedConnections = {
  list: () => call<SavedConnection[]>("connections_list"),
  /**
   * `secret` replaces the stored password or passphrase, and `oauthGrant` the stored cloud
   * account; omitted keeps what is stored.
   */
  save: (connection: SavedConnection, secret?: string | null, oauthGrant?: string | null) =>
    call<SavedConnection>("connections_save", {
      connection,
      secret: secret ?? null,
      oauthGrant: oauthGrant ?? null,
    }),
  remove: (id: string) => call<void>("connections_delete", { id }),
};

export const cloud = {
  providers: () => call<CloudProviderStatus[]>("cloud_providers"),
  /** Opens the provider's sign-in page in the browser; resolves once the account is back. */
  signIn: (requestId: string, provider: CloudProvider) =>
    call<SignedIn>("cloud_sign_in", { requestId, provider }),
  cancelSignIn: (requestId: string) => call<void>("cloud_cancel_sign_in", { requestId }),
};

export const themeFiles = {
  list: () => call<ThemeFile[]>("themes_list"),
  /** Without an id, the backend picks one from the theme's name. Returns the id. */
  save: (id: string | null, theme: unknown) => call<string>("theme_save", { id, theme }),
  remove: (id: string) => call<void>("theme_delete", { id }),
  importFile: (path: string) => call<string>("theme_import", { path }),
  openFolder: () => call<void>("themes_open_folder"),
};

/** Commands run through the server's shell, with their output sent as events. */
export const serverCommands = {
  run: (request: CommandRequest) => call<CommandResult>("remote_command_run", { request }),
  stop: (runId: string) => call<void>("remote_command_stop", { runId }),
};

export const system = {
  powerAction: (action: PowerAction) => call<void>("power_action", { action }),
  /** Runs a command through this computer's shell, with `environment` added. */
  runCommand: (command: string, environment: Record<string, string>) =>
    call<void>("local_command_run", { command, environment }),
  exit: () => call<void>("app_exit"),
};

export const schedules = {
  list: () => call<TaskView[]>("schedules_list"),
  save: (task: ScheduledTask) => call<TaskView>("schedule_save", { task }),
  remove: (id: string) => call<void>("schedule_delete", { id }),
  runNow: (id: string) => call<void>("schedule_run_now", { id }),
};

export const fonts = {
  list: () => call<FontFamily[]>("fonts_list"),
};

/** A file to show the system's icon for; mirrors `IconRequest` in system_icons.rs. */
export interface SystemIconRequest {
  name: string;
  folder: boolean;
  link: boolean;
}

export const systemIcons = {
  /** Each icon as a PNG data URL, `size` pixels square, or null where the system has none. */
  get: (requests: SystemIconRequest[], size: number) =>
    call<(string | null)[]>("system_icons", { requests, size }),
};

export const editor = {
  open: (location: DocumentLocation) => call<DocumentInfo>("editor_open", { location }),
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
  /**
   * Opens a window showing `layout`, `width` by `height` logical pixels, its top left corner at
   * `corner` in physical pixels or else centered; returns its label.
   */
  open: (layout: unknown, width: number, height: number, corner?: { x: number; y: number }) =>
    call<string>("window_open", {
      layout,
      width,
      height,
      x: corner?.x ?? null,
      y: corner?.y ?? null,
    }),
  initialLayout: () => call<unknown>("window_initial_layout"),
  setCorners: (corners: WindowCorners) => call<void>("window_set_corners", { corners }),
};

export const dragPreview = {
  /** Moves the preview of a tab dragged onto the desktop, opening it if needed. */
  place: (placement: PreviewPlacement) => call<void>("drag_preview_place", { placement }),
  /** Whether the system lets the preview follow the pointer, and see through it. */
  available: () => call<PreviewSupport>("drag_preview_available"),
  /** Opens the preview hidden, so it shows at once when first placed. */
  prepare: () => call<void>("drag_preview_prepare"),
  /** Gives the preview the picture of the dragged pane to draw. */
  picture: (picture: PanePicture) => call<void>("drag_preview_picture", { picture }),
  hide: () => call<void>("drag_preview_hide"),
  /**
   * Called by a new window once it has drawn: if a tab dropped on the desktop opened in it, the
   * preview that stood in its place leaves.
   */
  takenOver: () => call<void>("drag_preview_taken_over"),
  /** Called by the preview once it listens; returns what to show, if anything. */
  ready: () =>
    call<{ content: PreviewContent | null; picture: PanePicture | null }>("drag_preview_ready"),
  /** Shows the preview once it has drawn its content. */
  reveal: () => call<void>("drag_preview_reveal"),
};

export const updates = {
  /**
   * Downloads the update `check()` found and verifies its signature. It then installs, or on
   * Windows waits to install as Poros restarts or quits.
   */
  download: (updateId: number, onProgress: (progress: UpdateProgress) => void) => {
    const progress = new Channel<UpdateProgress>();
    progress.onmessage = onProgress;
    return call<void>("update_download", { updateId, progress });
  },
  /** Starts the new version; on Windows the installer closes Poros and starts it once done. */
  restart: () => call<void>("update_restart"),
  /** The releases this install can switch to, newest first, older ones included. */
  versions: () => call<string[]>("update_versions"),
  /** One release from that list, to download and install like an update. */
  findRelease: async (version: string) =>
    new Update(await call<UpdateMetadata>("update_find_release", { version })),
};

type UpdateMetadata = ConstructorParameters<typeof Update>[0];

export const application = {
  /**
   * What the backend logged before the main window listened, such as a settings file it could
   * not read. Taken once; later lines arrive as log events.
   */
  startupLog: () => call<LogRecord[]>("log_startup"),
};

function subscribe<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (received) => handler(received.payload));
}

export const onLog = (handler: (record: LogRecord) => void) => subscribe(LOG_EVENT, handler);
/** The drag preview has come up showing the tab of the drag `drag`. */
export const onDragPreviewShown = (handler: (drag: number) => void) =>
  subscribe(DRAG_PREVIEW_SHOWN_EVENT, handler);
export const onSessionClosed = (handler: (closed: SessionClosed) => void) =>
  subscribe(SESSION_CLOSED_EVENT, handler);
export const onTransfers = (handler: (update: TransferUpdate) => void) =>
  subscribe(TRANSFERS_EVENT, handler);
export const onStoreChanged = (handler: (store: StoreName) => void) =>
  subscribe(STORE_CHANGED_EVENT, handler);
export const onSyncProgress = (handler: (progress: SyncProgress) => void) =>
  subscribe(SYNC_PROGRESS_EVENT, handler);
export const onQueueFinished = (handler: (finished: QueueFinished) => void) =>
  subscribe(QUEUE_FINISHED_EVENT, handler);
export const onCommandOutput = (handler: (chunk: OutputChunk) => void) =>
  subscribe(COMMAND_OUTPUT_EVENT, handler);
export const onFileOperation = (handler: (progress: OperationProgress) => void) =>
  subscribe(FILE_OPERATION_EVENT, handler);
