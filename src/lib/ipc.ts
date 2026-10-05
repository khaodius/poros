// Typed wrappers for the Tauri commands in src-tauri/src/commands.rs.

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

/** Normalizes anything a command can reject with into an `AppError`. */
export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    return e as AppError;
  }
  return { kind: "io", message: e instanceof Error ? e.message : String(e) };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
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
  remove: (sessionId: string, paths: string[]) =>
    call<void>("remote_delete", { sessionId, paths }),
};

export function onLog(handler: (record: LogRecord) => void): Promise<UnlistenFn> {
  return listen<LogRecord>(LOG_EVENT, (event) => handler(event.payload));
}

export function onSessionClosed(handler: (closed: SessionClosed) => void): Promise<UnlistenFn> {
  return listen<SessionClosed>(SESSION_CLOSED_EVENT, (event) => handler(event.payload));
}
