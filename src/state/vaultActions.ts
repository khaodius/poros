// Cryptomator vaults in the panes: moving a pane between a server and its unlocked vaults as
// its path crosses a vault's folder, and locking a vault again.

import { remoteSource } from "../lib/fileSource";
import type { Handover } from "../hooks/usePane";
import { transfers } from "../lib/ipc";
import { allTabs, isRestoring } from "../lib/layout";
import { sessionShowing } from "../lib/vaults";
import { useLayoutStore } from "./layoutStore";
import { useLogStore } from "./logStore";
import { mountedDrives } from "./mountStore";
import { getPane } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useUiStore } from "./uiStore";

/**
 * The session to list `path` with for the pane in `tabId`, when that is not the pane's own: a
 * vault and its server share their paths, so the path alone tells which of them shows it. Taking
 * the handover gives the tab that session, with its pane and history as they are.
 */
export function vaultHandover(tabId: string, sessionId: string, path: string): Handover | null {
  const { sessions } = useSessionStore.getState();
  const showing = sessionShowing(sessions, sessionId, path);
  if (showing === sessionId) return null;
  return {
    source: remoteSource(sessions[showing].info, sessions[showing].profile),
    take: (listed) =>
      useLayoutStore
        .getState()
        .replaceTab(tabId, { id: tabId, kind: "remote", sessionId: showing, path: listed }),
  };
}

/**
 * Locks a vault: its session closes and its keys are forgotten. Every tab showing it goes back
 * to the vault's folder on the server, emptied first so no decrypted name stays on screen.
 */
export async function lockVault(sessionId: string): Promise<void> {
  const info = useSessionStore.getState().sessions[sessionId]?.info;
  if (!info?.vault) return;
  const { parentSessionId, root } = info.vault;
  const layout = useLayoutStore.getState();
  for (const tab of allTabs(layout.root)) {
    if (tab.kind !== "remote" || isRestoring(tab) || tab.sessionId !== sessionId) continue;
    getPane(tab.id)?.clear?.();
    layout.replaceTab(tab.id, {
      id: tab.id,
      kind: "remote",
      sessionId: parentSessionId,
      path: root,
    });
  }
  await useSessionStore.getState().disconnect(sessionId);
  useLogStore.getState().write("info", `Locked vault ${root}`, parentSessionId);
}

/** Locks a vault, asking first when transfers for it are still in the queue or it is mounted. */
export async function requestLockVault(sessionId: string): Promise<void> {
  const info = useSessionStore.getState().sessions[sessionId]?.info;
  if (!info?.vault) return;
  const confirm = useSettingsStore.getState().settings.interface.confirmCloseWithTransfers;
  const pendingTransfers = confirm ? await transfers.sessionJobs(sessionId).catch(() => 0) : 0;
  const drives = mountedDrives([sessionId]).map((drive) => drive.mountPoint);
  if (pendingTransfers > 0 || drives.length > 0) {
    useUiStore
      .getState()
      .open({ kind: "lockVault", sessionId, name: info.vault.name, pendingTransfers, drives });
    return;
  }
  await lockVault(sessionId);
}
