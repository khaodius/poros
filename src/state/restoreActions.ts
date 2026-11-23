// Brings back tabs that were open when Poros last closed: connects to their servers again and
// opens their files, then swaps each waiting tab for the tab it was.

import { create } from "zustand";
import { reconnectPlan } from "../lib/connectDraft";
import { editor, toAppError } from "../lib/ipc";
import {
  allTabs,
  editorTab,
  findTab,
  isRestoring,
  type RestoringTab,
  type SavedServer,
} from "../lib/layout";
import type { DocumentLocation, SessionInfo } from "../lib/types";
import { NEEDS_CREDENTIALS, connectWithPrompts } from "./connectFlow";
import { useLayoutStore } from "./layoutStore";
import { useSavedConnections } from "./savedConnectionsStore";
import { disconnectUnused, useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

export type ReconnectStatus =
  | { phase: "connecting" }
  /** Reconnecting on start is off, so the user starts it. */
  | { phase: "waiting" }
  | { phase: "signIn"; message: string | null }
  | { phase: "failed"; message: string };

interface RestoreState {
  /** By the session id the tabs had before the restart. */
  servers: Record<string, ReconnectStatus>;
}

export const useRestoreStore = create<RestoreState>(() => ({ servers: {} }));

function setStatus(key: string, status: ReconnectStatus | null): void {
  useRestoreStore.setState((state) => {
    const servers = { ...state.servers };
    if (status) servers[key] = status;
    else delete servers[key];
    return { servers };
  });
}

/** What tabs waiting for the same server share: the session they had before. */
export function serverKey(tab: RestoringTab): string {
  return tab.sessionId ?? tab.id;
}

function waitingFor(key: string): RestoringTab[] {
  return allTabs(useLayoutStore.getState().root).filter(
    (tab): tab is RestoringTab => isRestoring(tab) && serverKey(tab) === key,
  );
}

/** Local files being opened, so a tab mounted twice opens its file once. */
const openingFiles = new Set<string>();

async function reopenEditor(tab: RestoringTab, location: DocumentLocation): Promise<void> {
  if (tab.kind !== "editor" || openingFiles.has(tab.id)) return;
  openingFiles.add(tab.id);
  const sessionId = location.side === "remote" ? location.sessionId : undefined;
  try {
    const document = await editor.open(location);
    const layout = useLayoutStore.getState();
    if (!findTab(layout.root, tab.id)) {
      void editor.close(document.id).catch(() => undefined);
      if (sessionId) disconnectUnused(sessionId);
      return;
    }
    layout.replaceTab(tab.id, { ...editorTab(document, sessionId), id: tab.id });
  } catch (caught) {
    useToastStore
      .getState()
      .show("error", `Could not reopen ${tab.name}: ${toAppError(caught).message}`);
    useLayoutStore.getState().closeTab(tab.id);
    if (sessionId) disconnectUnused(sessionId);
  } finally {
    openingFiles.delete(tab.id);
  }
}

/** Shows every tab that waited for a server in a session connected to it. */
export function reopenWithSession(key: string, session: SessionInfo): void {
  const tabs = waitingFor(key);
  setStatus(key, null);
  if (tabs.length === 0) {
    // They were closed, or went to another window, while the connection was made.
    void useSessionStore.getState().disconnect(session.id);
    return;
  }
  const layout = useLayoutStore.getState();
  for (const tab of tabs) {
    const { id } = tab;
    if (tab.kind === "remote") {
      layout.replaceTab(id, { id, kind: "remote", sessionId: session.id, path: tab.path });
    } else if (tab.kind === "terminal") {
      layout.replaceTab(id, { id, kind: "terminal", sessionId: session.id, label: tab.label });
    } else {
      void reopenEditor(tab, { side: "remote", sessionId: session.id, path: tab.path });
    }
  }
}

/** Connects to a server tabs are waiting for, unless that is under way or needs the user. */
export async function reconnect(key: string, server: SavedServer): Promise<void> {
  if (useRestoreStore.getState().servers[key]?.phase === "connecting") return;
  const plan = reconnectPlan(server, useSavedConnections.getState().connections);
  if (!plan) {
    setStatus(key, { phase: "failed", message: "Poros no longer knows how to reach it." });
    return;
  }
  if (!plan.profile) {
    setStatus(key, { phase: "signIn", message: null });
    return;
  }
  setStatus(key, { phase: "connecting" });
  const result = await connectWithPrompts(plan.profile);
  if ("session" in result) {
    reopenWithSession(key, result.session);
    return;
  }
  const { kind, message } = result.error;
  setStatus(
    key,
    NEEDS_CREDENTIALS.has(kind) ? { phase: "signIn", message } : { phase: "failed", message },
  );
}

/**
 * Starts bringing back a tab once it shows: its file right away, or its server, which tabs
 * that shared a session connect to once, unless the user would rather start that themselves.
 */
export function startRestoring(tab: RestoringTab): void {
  const server = tab.restore.server;
  if (!server) {
    if (tab.kind === "editor") void reopenEditor(tab, { side: "local", path: tab.path });
    return;
  }
  const key = serverKey(tab);
  if (useRestoreStore.getState().servers[key]) return;
  if (useSettingsStore.getState().settings.interface.reconnectOnStart) void reconnect(key, server);
  else setStatus(key, { phase: "waiting" });
}

/** Opens the connect dialog for a server that needs a password or sign-in; connecting there
 * brings back the tabs waiting for it. */
export function signInToRestore(tab: RestoringTab): void {
  const server = tab.restore.server;
  const plan = server && reconnectPlan(server, useSavedConnections.getState().connections);
  if (!plan) return;
  useUiStore.getState().open({ kind: "connect", draft: plan.draft, targetTabId: tab.id });
}
