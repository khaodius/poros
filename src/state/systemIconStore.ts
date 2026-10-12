import { create } from "zustand";
import { systemIcons, type SystemIconRequest } from "../lib/ipc";

interface SystemIconState {
  /** PNG data URLs by `iconCacheKey`; null where the system has none or the lookup failed. */
  icons: Record<string, string | null>;
}

export const useSystemIconStore = create<SystemIconState>(() => ({ icons: {} }));

export function iconCacheKey(key: string, size: number): string {
  return `${size}:${key}`;
}

interface Lookup {
  cacheKey: string;
  size: number;
  request: SystemIconRequest;
}

const queued = new Map<string, Lookup>();
const inFlight = new Set<string>();
let flushScheduled = false;

/**
 * Looks up an icon once. The rows of a folder ask in the same frame, so their icons are looked
 * up together.
 */
export function requestSystemIcon(cacheKey: string, size: number, request: SystemIconRequest) {
  if (cacheKey in useSystemIconStore.getState().icons) return;
  if (queued.has(cacheKey) || inFlight.has(cacheKey)) return;
  queued.set(cacheKey, { cacheKey, size, request });
  if (flushScheduled) return;
  flushScheduled = true;
  setTimeout(lookUpQueued);
}

function lookUpQueued() {
  flushScheduled = false;
  const bySize = new Map<number, Lookup[]>();
  for (const lookup of queued.values()) {
    bySize.set(lookup.size, [...(bySize.get(lookup.size) ?? []), lookup]);
  }
  queued.clear();
  for (const [size, lookups] of bySize) {
    lookups.forEach((lookup) => inFlight.add(lookup.cacheKey));
    void systemIcons
      .get(
        lookups.map((lookup) => lookup.request),
        size,
      )
      .catch(() => lookups.map(() => null))
      .then((urls) => {
        lookups.forEach((lookup) => inFlight.delete(lookup.cacheKey));
        useSystemIconStore.setState(({ icons }) => ({
          icons: {
            ...icons,
            ...Object.fromEntries(
              lookups.map((lookup, index) => [lookup.cacheKey, urls[index] ?? null]),
            ),
          },
        }));
      });
  }
}
