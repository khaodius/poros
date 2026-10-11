import { useEffect } from "react";
import { DOCK_GAP, dropPlacement, placementRect, type Point, type Rect } from "../lib/docking";
import { dragPreview } from "../lib/ipc";
import { findTab, layoutPlacements } from "../lib/layout";
import { takePanePicture, type PanePicture } from "../lib/panePicture";
import type { PreviewDrop, PreviewPlacement } from "../lib/types";
import { onScreen } from "../lib/windowAreas";
import { isLifted, useDragStore, type DropTarget, type TabDrag } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { liftedLayers, liftedPane } from "../state/liftedPane";
import { takeWindowDroppedOnDesktop, tornOutCorner, tornOutSize } from "../state/tabActions";

interface PreviewFrame {
  /** The top left corner, in physical pixels. */
  x: number;
  y: number;
  width: number;
  height: number;
  toward: number;
  drop: PreviewDrop | null;
}

/**
 * The marker of a drop into the dock, placed on the lifted pane at `pane`, so the preview draws
 * it over the pane as this window does.
 */
function dropOnPane(target: DropTarget | null, dock: Rect, pane: Rect): PreviewDrop | null {
  if (target?.kind !== "dock") return null;
  const placements = layoutPlacements(useLayoutStore.getState().root, DOCK_GAP);
  const group = placements.get(target.groupId);
  if (!group) return null;
  const marker = placementRect(dropPlacement(group, target.side), { ...dock, left: 0, top: 0 });
  return {
    side: target.side,
    left: marker.left - pane.left,
    top: marker.top - pane.top,
    width: marker.width,
    height: marker.height,
  };
}

/**
 * Where the preview shows a dragged tab: the window it opens in once the pointer is off Poros,
 * and the lifted pane itself wherever it reaches past the edge of this window, which cannot
 * draw outside itself. None while the window draws all of it.
 */
function previewFrame(
  payload: TabDrag,
  pointer: Point,
  target: DropTarget | null,
): PreviewFrame | null {
  if (target?.kind === "outside") {
    const { x, y } = tornOutCorner(target.screen);
    return { x, y, ...tornOutSize(), toward: 1, drop: null };
  }
  if (target?.kind === "window" || !isLifted(payload, target)) return null;
  const { rect, toward } = liftedPane(payload, pointer, target, true);
  const left = payload.dock.left + rect.left;
  const top = payload.dock.top + rect.top;
  const inside =
    left >= 0 &&
    top >= 0 &&
    left + rect.width <= window.innerWidth &&
    top + rect.height <= window.innerHeight;
  const corner = inside ? null : onScreen({ x: left, y: top });
  if (!corner) return null;
  const drop = dropOnPane(target, payload.dock, rect);
  return { ...corner, width: rect.width, height: rect.height, toward, drop };
}

/**
 * Shows a dragged tab on the desktop where this window cannot: the part of the lifted pane past
 * the window's edge, and once the pointer leaves, the window the tab will open in, where and as
 * large as it will open. Dropped there, the preview stays until that window has drawn in its
 * place.
 */
export function useDragPreviewOnDesktop(): void {
  useEffect(() => {
    void dragPreview
      .available()
      .then((desktopPreview) => useDragStore.setState({ desktopPreview }))
      .catch(() => undefined);

    // One call at a time, the pane's picture first, and only the latest placement, so the
    // preview never lags behind.
    let picture: PanePicture | null = null;
    let pending: PreviewPlacement | "hide" | null = null;
    let sending = false;
    let shown = false;
    const send = () => {
      if (sending) return;
      let call: Promise<void>;
      if (picture) {
        call = dragPreview.picture(picture);
        picture = null;
      } else if (pending) {
        call = pending === "hide" ? dragPreview.hide() : dragPreview.place(pending);
        pending = null;
      } else {
        return;
      }
      sending = true;
      void call
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

    let drag = 0;
    let pictured = false;
    const unsubscribe = useDragStore.subscribe((state, previous) => {
      const { payload, target, pointer, desktopPreview } = state;
      if (payload?.kind === "tab" && previous.payload === null) {
        // Unique across windows, as the preview they share keeps the latest picture.
        drag = Math.max(Date.now(), drag + 1);
        pictured = false;
        // Opened ahead of time, so it shows at once when the pane first reaches past the edge.
        void dragPreview.prepare().catch(() => undefined);
      }
      const frame =
        payload?.kind === "tab" && desktopPreview ? previewFrame(payload, pointer, target) : null;
      if (!payload || payload.kind !== "tab" || !frame) {
        if (!shown) return;
        shown = false;
        if (payload) {
          hide();
          return;
        }
        // The drag ended with the preview up. A drop off Poros opens the tab in a new window,
        // which hides the preview once it has drawn; anything else hides it now.
        window.setTimeout(() => {
          void (takeWindowDroppedOnDesktop() ?? Promise.resolve(false)).then((opened) => {
            if (!opened && !shown) hide();
          });
        });
        return;
      }
      if (!pictured) {
        const panel = document.querySelector<HTMLElement>(".dock > .tab-panel.is-floating");
        if (panel) {
          pictured = true;
          picture = takePanePicture(panel, drag);
        } else if (liftedLayers(frame.toward).content > 0) {
          // The pane is drawn floating from the next frame on; it shows once it can be pictured.
          return;
        }
      }
      const kind = findTab(useLayoutStore.getState().root, payload.tabId)?.kind ?? "welcome";
      const toward = Math.round(frame.toward * 1000) / 1000;
      shown = true;
      pending = {
        content: { label: payload.label, kind, toward, drag, drop: frame.drop },
        x: frame.x,
        y: frame.y,
        width: frame.width,
        height: frame.height,
      };
      send();
    });

    return () => {
      unsubscribe();
      if (shown) void dragPreview.hide().catch(() => undefined);
    };
  }, []);
}
