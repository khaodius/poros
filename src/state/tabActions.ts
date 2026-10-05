import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { RETURN_TAB_EVENT, transfers, windows } from "../lib/ipc";
import {
  allGroups,
  allTabs,
  findGroup,
  findTab,
  group,
  groupOfTab,
  localTab,
  newId,
  parseLayout,
  welcomeTab,
  type DropSide,
  type GroupNode,
  type PaneTab,
} from "../lib/layout";
import type { ConnectProfile, SessionInfo } from "../lib/types";
import { useLayoutStore } from "./layoutStore";
import { getPane, lastActivePane } from "./paneRegistry";
import { useSessionStore, withoutSecrets } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

export const MAIN_WINDOW = "main";
const TORN_OUT_SHARE = 0.6;
const TORN_OUT_MIN = { width: 640, height: 420 };

/** What a window hands another when tabs move: a layout and its sessions' profiles. */
export interface TabHandoff {
  layout: unknown;
  sessions: Record<string, ConnectProfile | null>;
}

export function isMainWindow(): boolean {
  return getCurrentWindow().label === MAIN_WINDOW;
}

/**
 * Shows a new session in the given tab, else in a new-tab page on screen (the active group's
 * first), else as a new tab beside the server tab used last.
 */
export function showSession(session: SessionInfo, targetTabId?: string): void {
  const layout = useLayoutStore.getState();
  const showing = (candidate: GroupNode) =>
    candidate.tabs.find((tab) => tab.id === candidate.activeTabId);
  const activeGroup = findGroup(layout.root, layout.activeGroupId);
  const groups = activeGroup ? [activeGroup, ...allGroups(layout.root)] : allGroups(layout.root);
  const target: PaneTab | undefined = targetTabId
    ? (findTab(layout.root, targetTabId) ?? undefined)
    : groups.map(showing).find((tab) => tab?.kind === "welcome");
  const remoteTab: PaneTab = { id: newId("tab"), kind: "remote", sessionId: session.id };
  if (target?.kind === "welcome") {
    layout.replaceTab(target.id, { ...remoteTab, id: target.id });
    layout.activateTab(target.id);
    return;
  }
  const lastRemote = lastActivePane("remote");
  layout.addTab(remoteTab, lastRemote ? groupOfTab(layout.root, lastRemote.tabId)?.id : undefined);
}

/** The tab with the folder its pane shows now, so it reopens there after moving. */
function withCurrentPath(tab: PaneTab): PaneTab {
  if (tab.kind === "welcome") return tab;
  const path = getPane(tab.id)?.path();
  return path ? { ...tab, path } : tab;
}

function rememberPath(tabId: string): PaneTab | null {
  const layout = useLayoutStore.getState();
  const tab = findTab(layout.root, tabId);
  if (!tab) return null;
  const updated = withCurrentPath(tab);
  if (updated !== tab) layout.replaceTab(tabId, updated);
  return updated;
}

export function moveTabTo(tabId: string, groupId: string, index?: number): void {
  if (rememberPath(tabId)) useLayoutStore.getState().moveTab(tabId, groupId, index);
}

export function dockTabBeside(tabId: string, groupId: string, side: DropSide): void {
  if (rememberPath(tabId)) useLayoutStore.getState().dockTab(tabId, groupId, side);
}

/** Moves a tab to one side of its group, or opens a new tab there when it is alone. */
export function splitTab(tabId: string, side: DropSide): void {
  const layout = useLayoutStore.getState();
  const owner = groupOfTab(layout.root, tabId);
  const tab = findTab(layout.root, tabId);
  if (!owner || !tab) return;
  if (owner.tabs.length > 1) {
    dockTabBeside(tabId, owner.id, side);
    return;
  }
  const sibling =
    tab.kind === "local" ? localTab(getPane(tabId)?.path() ?? undefined) : welcomeTab();
  layout.addTab(sibling, owner.id);
  useLayoutStore.getState().dockTab(sibling.id, owner.id, side);
}

/** Closes a tab, disconnecting its session; asks first when transfers for it are queued. */
export async function requestCloseTab(tabId: string): Promise<void> {
  const layout = useLayoutStore.getState();
  const tab = findTab(layout.root, tabId);
  if (!tab) return;
  if (tab.kind !== "remote") {
    layout.closeTab(tabId);
    return;
  }
  const entry = useSessionStore.getState().sessions[tab.sessionId];
  const confirm = useSettingsStore.getState().settings.interface.confirmCloseWithTransfers;
  const pendingTransfers = confirm ? await transfers.sessionJobs(tab.sessionId).catch(() => 0) : 0;
  if (pendingTransfers > 0) {
    useUiStore.getState().open({
      kind: "closeTab",
      tabId,
      sessionId: tab.sessionId,
      label: entry?.info.label ?? "this server",
      pendingTransfers,
    });
    return;
  }
  closeRemoteTab(tabId, tab.sessionId);
}

export function closeRemoteTab(tabId: string, sessionId: string): void {
  useLayoutStore.getState().closeTab(tabId);
  void useSessionStore.getState().disconnect(sessionId);
}

function handoffFor(tab: PaneTab): TabHandoff {
  const sessions: TabHandoff["sessions"] = {};
  if (tab.kind === "remote") {
    sessions[tab.sessionId] = withoutSecrets(
      useSessionStore.getState().sessions[tab.sessionId]?.profile ?? null,
    );
  }
  return { layout: group([tab]), sessions };
}

/** Hands a tab to this window's caller-chosen destination and forgets it here. */
function detach(tab: PaneTab): void {
  useLayoutStore.getState().closeTab(tab.id);
  if (tab.kind === "remote") useSessionStore.getState().release(tab.sessionId);
}

/** Opens a tab in a window of its own, near the given screen point when the system allows. */
export async function tearOutTab(tabId: string, screenX?: number, screenY?: number) {
  const tab = rememberPath(tabId);
  if (!tab) return;
  const width = Math.max(TORN_OUT_MIN.width, Math.round(window.innerWidth * TORN_OUT_SHARE));
  const height = Math.max(TORN_OUT_MIN.height, Math.round(window.innerHeight * TORN_OUT_SHARE));
  const hasPoint = screenX !== undefined && screenY !== undefined && (screenX > 0 || screenY > 0);
  try {
    await windows.open(
      handoffFor(tab),
      width,
      height,
      hasPoint ? screenX - 60 : undefined,
      hasPoint ? screenY - 16 : undefined,
    );
    detach(tab);
  } catch (caught) {
    useToastStore.getState().show("error", (caught as { message?: string }).message ?? "");
  }
}

/** Sends a tab from a torn-out window back to the main window. */
export async function returnTabToMain(tabId: string): Promise<void> {
  const tab = rememberPath(tabId);
  if (!tab) return;
  await emitTo(MAIN_WINDOW, RETURN_TAB_EVENT, handoffFor(tab));
  detach(tab);
  const remaining = allTabs(useLayoutStore.getState().root);
  if (remaining.length === 1 && remaining[0].kind === "welcome") {
    await getCurrentWindow().close();
  }
}

/** Takes over the sessions of tabs another window handed over; returns the usable tabs. */
export async function adoptHandoff(handoff: unknown): Promise<PaneTab[]> {
  const value = handoff as Partial<TabHandoff> | null;
  const layout = parseLayout(value?.layout);
  if (!layout) return [];
  const sessions = value?.sessions ?? {};
  const adopted: PaneTab[] = [];
  for (const tab of allTabs(layout)) {
    if (tab.kind !== "remote") {
      adopted.push(tab);
      continue;
    }
    try {
      await useSessionStore.getState().adopt(tab.sessionId, sessions[tab.sessionId] ?? null);
      adopted.push(tab);
    } catch {
      adopted.push({ id: tab.id, kind: "welcome" });
    }
  }
  return adopted;
}
