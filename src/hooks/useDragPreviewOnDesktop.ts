import { useEffect } from "react";
import { dragPreview } from "../lib/ipc";
import { findTab } from "../lib/layout";
import type { PreviewPlacement } from "../lib/types";
import { useDragStore } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { takeWindowDroppedOnDesktop, tornOutCorner, tornOutSize } from "../state/tabActions";

/**
 * While a tab is dragged out over the desktop, shows the window it will open in, where and as
 * large as it will open. Dropped there, the preview stays until that window has drawn in its
 * place.
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
    const hide = () => {
      pending = "hide";
      send();
    };

    const unsubscribe = useDragStore.subscribe(({ payload, target }, previous) => {
      // Opened ahead of time, so it shows at once when the tab first leaves the window.
      if (payload?.kind === "tab" && previous.payload === null) {
        void dragPreview.prepare().catch(() => undefined);
      }
      if (payload?.kind !== "tab" || target?.kind !== "outside") {
        if (!shown) return;
        shown = false;
        if (payload) {
          hide();
          return;
        }
        // The drag ended out there. A drop opens the tab in a new window, which hides the
        // preview once it has drawn; anything else hides it now.
        window.setTimeout(() => {
          void (takeWindowDroppedOnDesktop() ?? Promise.resolve(false)).then((opened) => {
            if (!opened && !shown) hide();
          });
        });
        return;
      }
      const { x, y } = tornOutCorner(target.screen);
      const { width, height } = tornOutSize();
      const kind = findTab(useLayoutStore.getState().root, payload.tabId)?.kind ?? "welcome";
      shown = true;
      pending = { content: { label: payload.label, kind }, x, y, width, height };
      send();
    });

    return () => {
      unsubscribe();
      if (shown) void dragPreview.hide().catch(() => undefined);
    };
  }, []);
}
