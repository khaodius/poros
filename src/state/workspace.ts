// Keeps each window's tabs and panes for the next start, and gives the main window its own back.
// The backend keeps them with where each window was, and opens the other windows again itself.

import { windows } from "../lib/ipc";
import { parseLayout, persistableLayout, type LayoutNode, type SavedServer } from "../lib/layout";
import { useLayoutStore } from "./layoutStore";
import { getPane } from "./paneRegistry";
import { savedServerOf, useSessionStore, vaultOf } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";

const SAVE_DELAY_MILLIS = 400;
/** Where the main window kept its local tabs in earlier versions; read once, then removed. */
const OLD_STORAGE_KEY = "poros.layout";

/** What a window keeps of itself, told apart from tabs another window hands over. */
interface SavedLayout {
  kind: "workspace";
  layout: LayoutNode;
}

function remembering(): boolean {
  return useSettingsStore.getState().settings.interface.rememberLayout;
}

/**
 * Servers of sessions this window has had, also once another window took the session over with
 * a tab: a terminal or editor left here still reconnects to it.
 */
const knownServers = new Map<string, SavedServer>();

function rememberServers(sessionIds: string[]): void {
  for (const sessionId of sessionIds) {
    const server = savedServerOf(sessionId);
    if (server) knownServers.set(sessionId, server);
  }
}

function snapshot(): SavedLayout | null {
  if (!remembering()) return null;
  const layout = persistableLayout(useLayoutStore.getState().root, {
    folderOf: (tabId) => getPane(tabId)?.path() ?? null,
    serverOf: (sessionId) => savedServerOf(sessionId) ?? knownServers.get(sessionId) ?? null,
    vaultOf,
  });
  return layout && { kind: "workspace", layout };
}

let started = false;
let stopped = false;
let timer = 0;

function save(): Promise<void> {
  window.clearTimeout(timer);
  if (stopped) return Promise.resolve();
  return windows.saveWorkspace(snapshot()).catch(() => undefined);
}

/** Saves soon, so a pane resized or a folder opened several times in a row saves once. */
export function workspaceChanged(): void {
  if (!started || stopped) return;
  window.clearTimeout(timer);
  timer = window.setTimeout(() => void save(), SAVE_DELAY_MILLIS);
}

/** Saves what is open now, as the window closes. */
export function saveWorkspaceNow(): Promise<void> {
  return started ? save() : Promise.resolve();
}

/** Keeps what was saved last: the tabs that close as the window does are tabs to reopen. */
export function stopSavingWorkspace(): void {
  stopped = true;
  window.clearTimeout(timer);
}

/** Saves this window's tabs whenever they, their folders or their servers change. */
export function startSavingWorkspace(): void {
  if (started) return;
  started = true;
  useLayoutStore.subscribe((state, previous) => {
    if (state.root !== previous.root) workspaceChanged();
  });
  rememberServers(Object.keys(useSessionStore.getState().sessions));
  useSessionStore.subscribe((state, previous) => {
    if (state.sessions === previous.sessions) return;
    rememberServers(Object.keys(state.sessions));
    workspaceChanged();
  });
  useSettingsStore.subscribe((state, previous) => {
    const remember = state.settings.interface.rememberLayout;
    if (remember !== previous.settings.interface.rememberLayout) void save();
  });
  // A window opened again has no saved tabs under its new name until it saves.
  void save();
}

/** The tabs a window kept of itself, or null for anything else, such as tabs handed over. */
export function savedLayout(value: unknown): LayoutNode | null {
  const saved = value as Partial<SavedLayout> | null;
  return saved?.kind === "workspace" ? parseLayout(saved.layout) : null;
}

function takeOldLayout(): LayoutNode | null {
  try {
    const stored = localStorage.getItem(OLD_STORAGE_KEY);
    localStorage.removeItem(OLD_STORAGE_KEY);
    return stored ? parseLayout(JSON.parse(stored)) : null;
  } catch {
    return null;
  }
}

/**
 * The main window's tabs from the last time Poros ran, as the other windows open again around
 * it; null when there were none or the user would rather start afresh.
 */
export async function restoreWorkspace(): Promise<LayoutNode | null> {
  const oldLayout = takeOldLayout();
  if (!remembering()) {
    await windows.clearWorkspace().catch(() => undefined);
    return null;
  }
  const saved = await windows.restoreWorkspace().catch(() => null);
  return savedLayout(saved) ?? oldLayout;
}
