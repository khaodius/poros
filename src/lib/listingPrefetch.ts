// Listing ahead the folders the user is most likely to open next, so they are remembered by
// the time they are opened: the folder under the cursor and the parent of the one on show. Kept
// modest: one request per connection at a time, none where requests are counted or compete
// with transfers. Pure, so it is testable; `state/listingPrefetch.ts` runs it.

import { isCloud, isFtp } from "./protocols";
import { isDirLike } from "./sort";
import type { DirListing, Protocol } from "./types";

export interface PrefetchSettings {
  cachedListings: boolean;
  prefetchListings: boolean;
  prefetchOnCloud: boolean;
}

/**
 * Whether folders may be listed ahead on a connection. Cloud storage counts and limits requests,
 * so it takes a switch of its own; an FTP server has one connection for listings and transfers,
 * so nothing is listed ahead there while files are on their way.
 */
export function mayPrefetch(
  protocol: Protocol,
  settings: PrefetchSettings,
  transfersRunning: boolean,
): boolean {
  if (!settings.cachedListings || !settings.prefetchListings) return false;
  if (isCloud(protocol) && !settings.prefetchOnCloud) return false;
  return !(isFtp(protocol) && transfersRunning);
}

/** The folders worth listing ahead from `listing`: the one under the cursor, then its parent. */
export function prefetchTargets(listing: DirListing, cursorPath: string | null): string[] {
  const under = listing.entries.find((entry) => entry.path === cursorPath);
  const targets = [under && isDirLike(under) ? under.path : null, listing.parent];
  return targets.filter((target): target is string => target !== null && target !== listing.path);
}

/** One folder to list ahead: `key` tells it from the others, `run` lists it. */
export interface PrefetchJob {
  key: string;
  run: () => Promise<unknown>;
}

/**
 * Runs the jobs of each connection one at a time. Jobs still waiting on a connection give way
 * to the ones wanted next, as the cursor has moved on by then.
 */
export class PrefetchLanes {
  private readonly running = new Map<string, string>();
  private readonly waiting = new Map<string, PrefetchJob[]>();

  /** Lists `jobs` on `lane` once it is free, in place of whatever was waiting there. */
  want(lane: string, jobs: readonly PrefetchJob[]): void {
    const wanted = jobs.filter((job) => job.key !== this.running.get(lane));
    if (wanted.length === 0) this.waiting.delete(lane);
    else this.waiting.set(lane, wanted);
    this.advance(lane);
  }

  /** Drops what waits on `lane`; a request already made runs to its end. */
  clear(lane: string): void {
    this.waiting.delete(lane);
  }

  isBusy(lane: string): boolean {
    return this.running.has(lane);
  }

  private advance(lane: string): void {
    if (this.running.has(lane)) return;
    const queue = this.waiting.get(lane);
    const next = queue?.shift();
    if (!next) {
      this.waiting.delete(lane);
      return;
    }
    if (queue?.length === 0) this.waiting.delete(lane);
    this.running.set(lane, next.key);
    const done = () => {
      this.running.delete(lane);
      this.advance(lane);
    };
    next.run().then(done, done);
  }
}
