import { useEffect } from "react";
import { prefetchAround } from "../state/listingPrefetch";
import { useSettingsStore } from "../state/settingsStore";
import type { PaneController } from "./usePane";

/** How long the cursor rests on a row before the folder under it is listed ahead. */
const REST_MILLIS = 350;

/**
 * Lists ahead the folders the user is likely to open next from a server pane on screen: the
 * one under the cursor and the parent, once the pane's own folder is listed and the cursor rests.
 */
export function useListingPrefetch(
  pane: PaneController,
  sessionId: string | undefined,
  visible: boolean,
): void {
  const wanted = useSettingsStore(
    (state) => state.settings.interface.cachedListings && state.settings.interface.prefetchListings,
  );
  const { listing, cursorPath, loading, stale } = pane;
  useEffect(() => {
    if (!wanted || !visible || !sessionId || !listing || loading || stale !== null) return;
    const resting = window.setTimeout(
      () => prefetchAround(sessionId, listing, cursorPath),
      REST_MILLIS,
    );
    return () => window.clearTimeout(resting);
  }, [wanted, visible, sessionId, listing, cursorPath, loading, stale]);
}
