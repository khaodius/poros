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
  selected: () => FileEntry[];
  /** Everything the listing holds, also rows scrolled away or filtered out. */
  entries?: () => readonly FileEntry[];
  visible: boolean;
  activatedAt: number;
  /**
   * The pane shows the card of a locked vault in place of the folder's encrypted files, among
   * which nothing is to be copied unseen.
   */
  locked?: boolean;
  /**
   * The pane shows search results in place of its folder. Files let go of on its blank space
   * go into the folder the search started in, where nothing shows them, so it is said.
   */
  showsResults?: () => boolean;
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
