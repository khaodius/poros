// The file panes open in this window, so transfers can find a pane to send files to and
// panes showing a folder a transfer changed can refresh.

import type { FileEntry } from "../lib/types";

export interface PaneHandle {
  tabId: string;
  kind: "local" | "remote";
  sessionId?: string;
  label: string;
  path: () => string | null;
  /** Reloads the listing, keeping the selection, or moving the cursor to `focusPath`. */
  refresh: (focusPath?: string) => void;
  selected: () => FileEntry[];
  visible: boolean;
  activatedAt: number;
}

const panes = new Map<string, PaneHandle>();

export function registerPane(handle: PaneHandle): () => void {
  panes.set(handle.tabId, handle);
  return () => {
    if (panes.get(handle.tabId) === handle) panes.delete(handle.tabId);
  };
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
