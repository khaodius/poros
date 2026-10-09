import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MAIN_WINDOW, RETURN_TAB_EVENT, TAB_DRAG_EVENT, transfers, windows } from "../lib/ipc";
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
import { landingCorner, type ScreenPoint } from "../lib/windowAreas";
import type { DropTarget } from "./dragStore";
import { askToSave, closeEditorTab } from "./editorActions";
import { getEditor } from "./editorRegistry";
import { useLayoutStore } from "./layoutStore";
import { getPane, lastActivePane } from "./paneRegistry";
import { useSessionStore, withoutSecrets } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { closeTerminalTab } from "./terminalActions";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

const TORN_OUT_SHARE = 0.6;
const TORN_OUT_MIN = { width: 640, height: 420 };
/** Where the pointer holds a torn-out window, from its top left corner. */
const TORN_OUT_GRAB = { x: 60, y: 16 };

/** The size, in logical pixels, of a window a tab is torn out into. */
export function tornOutSize(): { width: number; height: number } {
  return {
    width: Math.max(TORN_OUT_MIN.width, Math.round(window.innerWidth * TORN_OUT_SHARE)),
    height: Math.max(TORN_OUT_MIN.height, Math.round(window.innerHeight * TORN_OUT_SHARE)),
  };
}

/**
 * Where a window torn out at a point on the screen has its top left corner, in physical pixels:
 * the pointer holds it near that corner.
 */
export function tornOutCorner(pointer: ScreenPoint): ScreenPoint {
  return landingCorner(pointer, TORN_OUT_GRAB);
}

/** What a window hands another when tabs move: a layout and its sessions' profiles. */
export interface TabHandoff {
  layout: unknown;
  sessions: Record<string, ConnectProfile | null>;
}

/**
 * Sent by the window a tab is dragged out of to the window under the pointer, with the pointer
 * in that window's coordinates: it marks where the tab would land, and takes the tab on a drop.
 */
export type TabDragMessage =
  | { phase: "over"; x: number; y: number; label: string; kind: PaneTab["kind"] }
  | { phase: "leave" }
  | { phase: "drop"; x: number; y: number; handoff: TabHandoff };

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

/** The tab with the folder its pane shows now, or an editor's unsaved text. */
function withCurrentState(tab: PaneTab): PaneTab {
  if (tab.kind === "editor") return { ...tab, draft: getEditor(tab.id)?.draft() ?? undefined };
  if (tab.kind !== "local" && tab.kind !== "remote") return tab;
  const path = getPane(tab.id)?.path();
  return path ? { ...tab, path } : tab;
}

/** A tab about to leave this window, with what to reopen it with in the next. */
function departingTab(tabId: string): PaneTab | null {
  const tab = findTab(useLayoutStore.getState().root, tabId);
  return tab && withCurrentState(tab);
}

export function moveTabTo(tabId: string, groupId: string, index?: number): void {
  useLayoutStore.getState().moveTab(tabId, groupId, index);
}

export function dockTabBeside(tabId: string, groupId: string, side: DropSide): void {
  useLayoutStore.getState().dockTab(tabId, groupId, side);
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

/** Closes a tab, disconnecting its session; asks first when transfers for it are queued or
 * the file it edits has unsaved changes. */
export async function requestCloseTab(tabId: string): Promise<void> {
  const layout = useLayoutStore.getState();
  const tab = findTab(layout.root, tabId);
  if (!tab) return;
  if (tab.kind === "editor") {
    if (getEditor(tabId)?.draft()) askToSave([tabId]);
    else closeEditorTab(tabId);
    return;
  }
  if (tab.kind === "terminal") {
    closeTerminalTab(tabId);
    return;
  }
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

/** Opens a tab in a window of its own, held at the given screen point when the system allows. */
export async function tearOutTab(tabId: string, pointer?: ScreenPoint) {
  const tab = departingTab(tabId);
  if (!tab) return;
  const { width, height } = tornOutSize();
  try {
    await windows.open(handoffFor(tab), width, height, pointer && tornOutCorner(pointer));
    detach(tab);
  } catch (caught) {
    useToastStore.getState().show("error", (caught as { message?: string }).message ?? "");
  }
}

/** A torn-out window closes once the last of its tabs has left. */
async function closeIfEmptied(): Promise<void> {
  if (isMainWindow()) return;
  const remaining = allTabs(useLayoutStore.getState().root);
  if (remaining.length === 1 && remaining[0].kind === "welcome") {
    await getCurrentWindow().close();
  }
}

/** Sends a tab from a torn-out window back to the main window. */
export async function returnTabToMain(tabId: string): Promise<void> {
  const tab = departingTab(tabId);
  if (!tab) return;
  await emitTo(MAIN_WINDOW, RETURN_TAB_EVENT, handoffFor(tab));
  detach(tab);
  await closeIfEmptied();
}

/** Hands a tab dropped on another window to that window, at the point it was dropped. */
export async function moveTabToWindow(tabId: string, label: string, x: number, y: number) {
  const tab = departingTab(tabId);
  if (!tab) return;
  const message: TabDragMessage = { phase: "drop", x, y, handoff: handoffFor(tab) };
  try {
    await emitTo(label, TAB_DRAG_EVENT, message);
  } catch (caught) {
    useToastStore.getState().show("error", (caught as { message?: string }).message ?? "");
    return;
  }
  detach(tab);
  await closeIfEmptied();
}

/**
 * Adds tabs another window handed over where they were dropped, else to the active group, and
 * brings this window forward.
 */
export async function receiveTabs(handoff: unknown, target: DropTarget | null): Promise<void> {
  for (const [offset, tab] of (await adoptHandoff(handoff)).entries()) {
    const layout = useLayoutStore.getState();
    const groupId =
      target?.kind === "tabBar" || target?.kind === "dock" ? target.groupId : undefined;
    if (!groupId || !findGroup(layout.root, groupId)) layout.addTab(tab);
    else if (target?.kind === "tabBar") layout.addTab(tab, groupId, target.index + offset);
    else layout.addTab(tab, groupId);
  }
  await getCurrentWindow()
    .setFocus()
    .catch(() => undefined);
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
