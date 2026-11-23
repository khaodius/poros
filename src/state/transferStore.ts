import { create } from "zustand";
import { transfers as transferIpc } from "../lib/ipc";
import {
  EMPTY_SNAPSHOT,
  applyUpdate,
  isSettled,
  missesUpdates,
  replayOnList,
  type TransferSnapshot,
} from "../lib/transfers";
import type { ChangedDirectory, TransferUpdate } from "../lib/types";
import { noteTransferUpdate } from "./fileFlight";
import { forgetListing } from "./listings";
import { panesShowing } from "./paneRegistry";

const REFRESH_DELAY_MILLIS = 400;
/** Reads of the list in a row while updates keep going missing, before waiting for the next. */
const MAX_LIST_READS = 3;

interface TransferState {
  snapshot: TransferSnapshot;
  /** Reads the whole queue; a read already under way serves every caller. */
  load: () => Promise<void>;
  apply: (update: TransferUpdate) => void;
}

let reading: Promise<void> | null = null;
/** Updates that arrived while the list was being read, to merge into it. */
let arrivedWhileReading: TransferUpdate[] | null = null;

export const useTransferStore = create<TransferState>((set, get) => ({
  snapshot: EMPTY_SNAPSHOT,
  load: () => {
    reading ??= (async () => {
      try {
        for (let read = 0; read < MAX_LIST_READS; read++) {
          arrivedWhileReading = [];
          const list = await transferIpc.list().catch(() => null);
          const arrived = arrivedWhileReading;
          arrivedWhileReading = null;
          if (!list) return;
          const { snapshot, complete } = replayOnList(list, arrived);
          set({ snapshot });
          if (complete) return;
        }
      } finally {
        arrivedWhileReading = null;
        reading = null;
      }
    })();
    return reading;
  },
  apply: (update) => {
    arrivedWhileReading?.push(update);
    refreshChangedPanes(update.changedDirectories);
    const current = get().snapshot;
    if (update.sequence <= current.sequence) return;
    noteTransferUpdate(current, update);
    set({ snapshot: applyUpdate(current, update) });
    // A missed update can leave a row in a state its job has left for good, such as a finished
    // transfer still shown running, so the whole queue is read again. It is also read once each
    // time the queue settles, so what it shows then is what the queue holds.
    if (missesUpdates(current, update) || (isSettled(update.stats) && !isSettled(current.stats))) {
      void get().load();
    }
  },
}));

const pendingRefreshes = new Map<string, number>();
/** Server folders whose listing was just forgotten, and whether they changed again since. */
const justForgotten = new Map<string, boolean>();

/**
 * Forgets the remembered listing of a server folder a transfer wrote to: at once, then once
 * more after a burst of changes rather than for every file of it.
 */
function forgetChangedListing(sessionId: string, path: string): void {
  const key = `${sessionId}\n${path}`;
  if (justForgotten.has(key)) {
    justForgotten.set(key, true);
    return;
  }
  justForgotten.set(key, false);
  forgetListing(sessionId, path);
  window.setTimeout(() => {
    const changedSince = justForgotten.get(key);
    justForgotten.delete(key);
    if (changedSince) forgetChangedListing(sessionId, path);
  }, REFRESH_DELAY_MILLIS);
}

/** Panes showing a folder a transfer wrote to reload, once per burst of changes. */
function refreshChangedPanes(changed: ChangedDirectory[]): void {
  for (const directory of changed) {
    if (directory.side === "remote" && directory.sessionId) {
      forgetChangedListing(directory.sessionId, directory.path);
    }
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
