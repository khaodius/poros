// Lists ahead the folders `lib/listingPrefetch.ts` picks, on the session that shows each, and
// remembers their listings like any other.

import { remote } from "../lib/ipc";
import { listingScope } from "../lib/listingCache";
import {
  PrefetchLanes,
  mayPrefetch,
  prefetchTargets,
  type PrefetchJob,
} from "../lib/listingPrefetch";
import { isSettled } from "../lib/transfers";
import type { DirListing } from "../lib/types";
import { sessionShowing } from "../lib/vaults";
import { listingShare } from "./listings";
import { serverEntry, useSessionStore, type SessionEntry } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useTransferStore } from "./transferStore";

const lanes = new PrefetchLanes();

/** Whether listings are still remembered, and the session is still open in this window. */
function stillListing(sessionId: string): boolean {
  const remembering = useSettingsStore.getState().settings.interface.cachedListings;
  const entry = useSessionStore.getState().sessions[sessionId] as SessionEntry | undefined;
  return remembering && entry !== undefined;
}

/**
 * Lists ahead the folders likely to be opened next from `listing`, which a pane on `sessionId`
 * shows with the cursor on `cursorPath`. One folder at a time per server, and none where the
 * settings or the server's protocol say not to.
 */
export function prefetchAround(
  sessionId: string,
  listing: DirListing,
  cursorPath: string | null,
): void {
  const { sessions } = useSessionStore.getState();
  const server = serverEntry(sessions, sessionId);
  if (!server || server.status === "lost") return;
  const lane = server.info.id;
  const settings = useSettingsStore.getState().settings.interface;
  const transfersRunning = !isSettled(useTransferStore.getState().snapshot.stats);
  if (!mayPrefetch(server.info.protocol, settings, transfersRunning)) {
    lanes.clear(lane);
    return;
  }
  const cache = listingShare.cache;
  const jobs = prefetchTargets(listing, cursorPath).flatMap((path): PrefetchJob[] => {
    // A vault and its server share their paths: the path tells which of them lists it.
    const showing = sessions[sessionShowing(sessions, sessionId, path)] as SessionEntry | undefined;
    if (!showing) return [];
    const scope = listingScope(showing.info, showing.profile);
    if (cache.has(scope, path)) return [];
    const run = async () => {
      // A job that waited its turn while its session left, as a vault that locked, or while
      // the setting was turned off, asks for nothing that would be kept under a forgotten name.
      if (cache.has(scope, path) || !stillListing(showing.info.id)) return;
      const ticket = cache.begin(scope, path);
      try {
        listingShare.store(scope, await remote.list(showing.info.id, path), ticket);
      } catch {
        // A folder that cannot be listed says so when the user opens it.
        cache.cancel(ticket);
      }
    };
    return [{ key: `${scope}\n${path}`, run }];
  });
  lanes.want(lane, jobs);
}
