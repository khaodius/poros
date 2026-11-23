// The file panes open in this window, so transfers can find a pane to send files to and
// panes showing a folder a transfer changed can refresh.

import type { FileEntry } from "../lib/types";

export interface PaneHandle {
  tabId: string;
  kind: "local" | "remote";
  sessionId?: string;
  label: string;
  path: () => string | null;
  /**
   * Reloads the listing, keeping the selection, or selecting `focus` (a path, or several)
   * with the cursor on the first.
   */
  refresh: (focus?: string | readonly string[]) => void;
  /** Empties the pane at once, before it is given another session to list from. */
  clear?: () => void;
  /** Whether a listing is on its way, so what the pane shows may be about to change. */
  loading?: () => boolean;
  selected: () => FileEntry[];
  /** Everything the listing holds, also rows scrolled away or filtered out. */
  entries?: () => readonly FileEntry[];
  visible: boolean;
  activatedAt: number;
  /**
   * The pane shows the card of a locked vault in place of the folder's encrypted files, among
   * which nothing is to be copied unseen. Absent until the pane has said either way.
   */
  locked?: boolean;
}

const panes = new Map<string, PaneHandle>();

/**
 * Adds a pane. A pane registers again when its session or its name changes, as after a
 * reconnect or on entering a vault: what it reported since it first registered is still so.
 */
export function registerPane(handle: PaneHandle): () => void {
  const replaced = panes.get(handle.tabId);
  if (replaced) {
    handle.visible = replaced.visible;
    handle.activatedAt = replaced.activatedAt;
    handle.locked = replaced.locked;
  }
  panes.set(handle.tabId, handle);
  return () => {
    if (panes.get(handle.tabId) === handle) panes.delete(handle.tabId);
  };
}

/** Forgets a tab's pane, once the pane itself is gone. */
export function unregisterPane(tabId: string): void {
  panes.delete(tabId);
}

export function updatePane(tabId: string, change: Partial<PaneHandle>): void {
  const handle = panes.get(tabId);
  if (handle) Object.assign(handle, change);
}

export function getPane(tabId: string): PaneHandle | undefined {
  return panes.get(tabId);
}

export function listPanes(): PaneHandle[] {
  return [...panes.values()];
}

/** The pane on screen the user worked in last. */
export function activePane(): PaneHandle | null {
  let best: PaneHandle | null = null;
  for (const handle of panes.values()) {
    if (handle.visible && (!best || handle.activatedAt > best.activatedAt)) best = handle;
  }
  return best;
}

/** The pane of a kind the user looked at last, preferring ones on screen. */
export function lastActivePane(kind: "local" | "remote", sessionId?: string): PaneHandle | null {
  let best: PaneHandle | null = null;
  for (const handle of panes.values()) {
    if (handle.kind !== kind || handle.path() === null) continue;
    if (sessionId !== undefined && handle.sessionId !== sessionId) continue;
    const better =
      !best ||
      (handle.visible && !best.visible) ||
      (handle.visible === best.visible && handle.activatedAt > best.activatedAt);
    if (better) best = handle;
  }
  return best;
}

export function panesShowing(kind: "local" | "remote", path: string, sessionId?: string) {
  return [...panes.values()].filter(
    (handle) =>
      handle.kind === kind &&
      (kind === "local" || handle.sessionId === sessionId) &&
      handle.path() === path,
  );
}
