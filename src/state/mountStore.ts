import { create } from "zustand";
import { mounts, onDriveChanges, onMounts } from "../lib/ipc";
import { drivesShowing } from "../lib/mounts";
import type { DriveChanges, DriveView, MountSupport, UnsentFile } from "../lib/types";
import { forgetListing } from "./listings";
import { refreshChangedPanes } from "./transferStore";

/** How often the drives are read while any is mounted, for what they are busy with. */
const POLL_MILLIS = 1500;

interface MountState {
  drives: DriveView[];
  /** Files kept from drives that are gone. */
  unsent: UnsentFile[];
  /** Null until asked. */
  support: MountSupport | null;
  /** Reads the drives and the kept files again; a read already under way serves every caller. */
  load: () => Promise<void>;
  loadSupport: () => Promise<MountSupport>;
}

let reading: Promise<void> | null = null;
let polling: ReturnType<typeof setTimeout> | null = null;

export const useMountStore = create<MountState>((set, get) => ({
  drives: [],
  unsent: [],
  support: null,
  load: () => {
    reading ??= (async () => {
      try {
        const [drives, unsent] = await Promise.all([mounts.list(), mounts.unsent()]);
        set({ drives, unsent });
      } catch {
        // Kept as it was: the next event or poll reads again.
      } finally {
        reading = null;
      }
      schedulePoll(get().drives.length > 0);
    })();
    return reading;
  },
  loadSupport: async () => {
    const known = get().support;
    if (known) return known;
    const support = await mounts.support();
    set({ support });
    return support;
  },
}));

function schedulePoll(wanted: boolean): void {
  if (polling !== null) clearTimeout(polling);
  polling = wanted
    ? setTimeout(() => {
        polling = null;
        void useMountStore.getState().load();
      }, POLL_MILLIS)
    : null;
}

/**
 * What programs change on a server through a drive happens outside Poros's own operations, so
 * the listings remembered of those folders are forgotten and the panes showing them reload.
 */
function forgetDriveChanges({ sessionId, folders }: DriveChanges): void {
  for (const { folder, entries } of folders) {
    if (entries.length > 0) forgetListing(sessionId, folder, entries);
  }
  refreshChangedPanes(folders.map(({ folder }) => ({ side: "remote", sessionId, path: folder })));
}

/** Keeps the drives up to date in this window; returns what stops it. */
export function watchMounts(): () => void {
  const subscriptions = [
    onMounts(() => void useMountStore.getState().load()),
    onDriveChanges(forgetDriveChanges),
  ];
  void useMountStore.getState().load();
  return () => {
    for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten());
    schedulePoll(false);
  };
}

/** The drives that show any of the sessions, as last read. */
export function mountedDrives(sessionIds: string[]): DriveView[] {
  return drivesShowing(useMountStore.getState().drives, sessionIds);
}
