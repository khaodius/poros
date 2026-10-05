import { create } from "zustand";
import { transfers as transferIpc } from "../lib/ipc";
import {
  EMPTY_SNAPSHOT,
  applyUpdate,
  snapshotFromList,
  type TransferSnapshot,
} from "../lib/transfers";
import type { ChangedDirectory, TransferUpdate } from "../lib/types";
import { panesShowing } from "./paneRegistry";

const REFRESH_DELAY_MILLIS = 400;

interface TransferState {
  snapshot: TransferSnapshot;
  load: () => Promise<void>;
  apply: (update: TransferUpdate) => void;
}

export const useTransferStore = create<TransferState>((set, get) => ({
  snapshot: EMPTY_SNAPSHOT,
  load: async () => {
    const list = await transferIpc.list().catch(() => null);
    if (list) set({ snapshot: snapshotFromList(list) });
  },
  apply: (update) => {
    set({ snapshot: applyUpdate(get().snapshot, update) });
    refreshChangedPanes(update.changedDirectories);
  },
}));

const pendingRefreshes = new Map<string, number>();

/** Panes showing a folder a transfer wrote to reload, once per burst of changes. */
function refreshChangedPanes(changed: ChangedDirectory[]): void {
  for (const directory of changed) {
    for (const pane of panesShowing(directory.side, directory.path, directory.sessionId)) {
      if (pendingRefreshes.has(pane.tabId)) continue;
      pendingRefreshes.set(
        pane.tabId,
        window.setTimeout(() => {
          pendingRefreshes.delete(pane.tabId);
          pane.refresh();
        }, REFRESH_DELAY_MILLIS),
      );
    }
  }
}
