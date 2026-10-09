import { useEffect } from "react";
import { dragPreview } from "../lib/ipc";
import { findTab } from "../lib/layout";
import type { PreviewPlacement } from "../lib/types";
import { useDragStore } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { tornOutCorner, tornOutSize } from "../state/tabActions";

/** The preview's size against the window the tab will open in. */
const PREVIEW_SCALE = 0.4;

/**
 * While a tab is dragged out over the desktop, shows a small copy of the window it will open
 * in, its top left corner where the new window's will be.
 */
export function useDragPreviewOnDesktop(): void {
  useEffect(() => {
    void dragPreview
      .available()
      .then((desktopPreview) => useDragStore.setState({ desktopPreview }))
      .catch(() => undefined);

    // One call at a time, and only the latest placement, so the preview never lags behind.
    let pending: PreviewPlacement | "hide" | null = null;
    let sending = false;
    let shown = false;
    const send = () => {
      if (sending || !pending) return;
      const next = pending;
      pending = null;
      sending = true;
      void (next === "hide" ? dragPreview.hide() : dragPreview.place(next))
        .catch(() => undefined)
        .finally(() => {
          sending = false;
          send();
        });
    };

    const unsubscribe = useDragStore.subscribe(({ payload, target }) => {
      if (payload?.kind !== "tab" || target?.kind !== "outside") {
        if (!shown) return;
        shown = false;
        pending = "hide";
        send();
        return;
      }
      const { x, y } = tornOutCorner(target.screen);
      const { width, height } = tornOutSize();
      const kind = findTab(useLayoutStore.getState().root, payload.tabId)?.kind ?? "welcome";
      shown = true;
      pending = {
        content: { label: payload.label, kind },
        x,
        y,
        width: Math.round(width * PREVIEW_SCALE),
        height: Math.round(height * PREVIEW_SCALE),
      };
      send();
    });

    return () => {
      unsubscribe();
      if (shown) void dragPreview.hide().catch(() => undefined);
    };
  }, []);
}
