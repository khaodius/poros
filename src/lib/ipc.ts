import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  ConnectProfile,
  DirListing,
  HostKeyApproval,
  LogRecord,
  SessionClosed,
  SessionInfo,
} from "./types";

export const LOG_EVENT = "poros://log";
export const SESSION_CLOSED_EVENT = "poros://session-closed";

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
  mkdir: (parent: string, name: string) => call<string>("local_mkdir", { parent, name }),
  rename: (path: string, newName: string) => call<string>("local_rename", { path, newName }),
  remove: (paths: string[]) => call<void>("local_delete", { paths }),
};

export const remote = {
  connect: (profile: ConnectProfile, hostKeyApproval?: HostKeyApproval) =>
    call<SessionInfo>("connect", { profile, hostKeyApproval: hostKeyApproval ?? null }),
  disconnect: (sessionId: string) => call<void>("disconnect", { sessionId }),
  list: (sessionId: string, path: string) => call<DirListing>("remote_list", { sessionId, path }),
  mkdir: (sessionId: string, parent: string, name: string) =>
    call<string>("remote_mkdir", { sessionId, parent, name }),
  rename: (sessionId: string, path: string, newName: string) =>
    call<string>("remote_rename", { sessionId, path, newName }),
  remove: (sessionId: string, paths: string[]) => call<void>("remote_delete", { sessionId, paths }),
};

export function onLog(handler: (record: LogRecord) => void): Promise<UnlistenFn> {
  return listen<LogRecord>(LOG_EVENT, (event) => handler(event.payload));
}

export function onSessionClosed(handler: (closed: SessionClosed) => void): Promise<UnlistenFn> {
  return listen<SessionClosed>(SESSION_CLOSED_EVENT, (event) => handler(event.payload));
}
