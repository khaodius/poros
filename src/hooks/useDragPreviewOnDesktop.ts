import { useEffect } from "react";
import { DOCK_GAP, dropPlacement, placementRect, type Point, type Rect } from "../lib/docking";
import { dragPreview, onDragPreviewShown } from "../lib/ipc";
import { findTab, layoutPlacements } from "../lib/layout";
import { takePanePicture, type PanePicture } from "../lib/panePicture";
import type { PreviewDrop, PreviewPlacement } from "../lib/types";
import { onScreen, scaleAt } from "../lib/windowAreas";
import { isLifted, useDragStore, type DropTarget, type TabDrag } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { liftedLayers, liftedPane } from "../state/liftedPane";
import { takeWindowDroppedOnDesktop, tornOutCorner, tornOutSize } from "../state/tabActions";

/**
 * The room left around the pane in a see-through preview, in logical pixels, for its shadow. In
 * whole pixels at every scale Windows offers, so the pane sits on whole pixels in the preview.
 */
const SHADOW_ROOM = 40;

interface PreviewFrame {
  /** The top left corner of the pane, or of the room around it, in physical pixels. */
  x: number;
  y: number;
  /** The pane's size, in logical pixels. */
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
 * Where the preview shows a dragged tab, with `room` around it: the window it opens in once the
 * pointer is off Poros, and the lifted pane itself wherever it reaches past the edge of this
 * window, which cannot draw outside itself. None while the window draws all of it.
 */
function previewFrame(
  payload: TabDrag,
  pointer: Point,
  target: DropTarget | null,
  room: number,
): PreviewFrame | null {
  if (target?.kind === "outside") {
    const corner = tornOutCorner(target.screen);
    // In the pixels of the screen the window opens on, as the preview there draws it.
    const margin = Math.round(room * scaleAt(corner));
    return {
      x: corner.x - margin,
      y: corner.y - margin,
      ...tornOutSize(),
      toward: 1,
      drop: null,
    };
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
  const corner = inside ? null : onScreen({ x: left - room, y: top - room });
  if (!corner) return null;
  const drop = dropOnPane(target, payload.dock, rect);
  return { ...corner, width: rect.width, height: rect.height, toward, drop };
}

/**
 * Shows a dragged tab on the desktop where this window cannot: the lifted pane once it reaches
 * past the window's edge, and once the pointer leaves, the window the tab will open in, where
 * and as large as it will open. While the preview shows the tab, this window leaves it out, so
 * only one of the two draws it. Dropped on the desktop, the preview stays until the new window
 * has drawn in its place.
 */
export function useDragPreviewOnDesktop(): void {
  useEffect(() => {
    void dragPreview
      .available()
      .then(({ available, seeThrough }) =>
        useDragStore.setState({ desktopPreview: available, previewSeeThrough: seeThrough }),
      )
      .catch(() => undefined);

    // One call at a time, the pane's picture first, and only the latest placement, so the
    // preview never lags behind.
    let picture: PanePicture | null = null;
    let pending: PreviewPlacement | "hide" | null = null;
    let sending = false;
    /** This drag wants the preview up. */
    let shown = false;
    /** The preview has come up with this drag's tab, and has not been hidden since. */
    let up = false;
    const drawnOnDesktop = (drawn: boolean) => {
      if (useDragStore.getState().drawnOnDesktop !== drawn) {
        useDragStore.setState({ drawnOnDesktop: drawn });
      }
    };
    const send = () => {
      if (sending) return;
      let call: Promise<void>;
      if (picture) {
        call = dragPreview.picture(picture);
        picture = null;
      } else if (pending === "hide") {
        up = false;
        call = dragPreview.hide();
        pending = null;
      } else if (pending) {
        call = dragPreview.place(pending);
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
    // This window leaves the tab out once the preview has drawn it, a frame later than the
    // preview came up, so the two overlap rather than leave a gap.
    const listening = onDragPreviewShown((shownDrag) => {
      if (shownDrag !== drag) return;
      up = true;
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          if (up && shown) drawnOnDesktop(true);
        }),
      );
    });

    const unsubscribe = useDragStore.subscribe((state, previous) => {
      const { payload, target, pointer, desktopPreview, previewSeeThrough } = state;
      if (
        payload === previous.payload &&
        target === previous.target &&
        pointer === previous.pointer &&
        desktopPreview === previous.desktopPreview &&
        previewSeeThrough === previous.previewSeeThrough
      ) {
        return;
      }
      if (payload?.kind === "tab" && previous.payload === null) {
        // Unique across windows, as the preview they share keeps the latest picture.
        drag = Math.max(Date.now(), drag + 1);
        pictured = false;
        up = false;
        // Opened ahead of time, so it shows at once when the pane first reaches past the edge.
        void dragPreview.prepare().catch(() => undefined);
      }
      const room = previewSeeThrough ? SHADOW_ROOM : 0;
      const frame =
        payload?.kind === "tab" && desktopPreview
          ? previewFrame(payload, pointer, target, room)
          : null;
      if (!payload || payload.kind !== "tab" || !frame) {
        drawnOnDesktop(false);
        if (!shown) return;
        shown = false;
        if (payload) {
          // Once this window has drawn the tab again, so the two overlap rather than leave a gap.
          requestAnimationFrame(() =>
            requestAnimationFrame(() => {
              if (!shown) hide();
            }),
          );
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
      const { width, height } = frame;
      shown = true;
      // Still up from earlier in this drag, as a hide was overtaken before it went out.
      if (up) drawnOnDesktop(true);
      pending = {
        content: {
          label: payload.label,
          kind,
          toward,
          drag,
          drop: frame.drop,
          width,
          height,
          room,
        },
        x: frame.x,
        y: frame.y,
        width: width + 2 * room,
        height: height + 2 * room,
      };
      send();
    });

    return () => {
      unsubscribe();
      void listening.then((unlisten) => unlisten());
      if (shown) void dragPreview.hide().catch(() => undefined);
    };
  }, []);
}
