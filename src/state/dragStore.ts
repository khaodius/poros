// Drags inside a window use pointer events rather than HTML drag and drop: Tauri takes over
// native drag and drop on Windows to report files dropped from outside the app.

import type { PointerEvent as ReactPointerEvent } from "react";
import { create } from "zustand";
import type { DropSide } from "../lib/layout";
import type { FileEntry } from "../lib/types";

export type DragPayload =
  | { kind: "files"; sourceTabId: string; entries: FileEntry[] }
  | { kind: "tab"; tabId: string; label: string }
  /** Files dragged in from the operating system. */
  | { kind: "external"; paths: string[] };

export type DropTarget =
  | { kind: "pane"; tabId: string; folder: string | null }
  | { kind: "dock"; groupId: string; side: DropSide }
  | { kind: "tabBar"; groupId: string; index: number }
  | { kind: "outside"; screenX: number; screenY: number };

interface DragState {
  payload: DragPayload | null;
  target: DropTarget | null;
  pointer: { x: number; y: number };
}

export const useDragStore = create<DragState>(() => ({
  payload: null,
  target: null,
  pointer: { x: 0, y: 0 },
}));

const DRAG_THRESHOLD = 5;
const DOCK_EDGE_SHARE = 0.28;

interface PointerPosition {
  clientX: number;
  clientY: number;
  screenX: number;
  screenY: number;
}

/** Finds what is under the pointer: a folder row, a pane, a tab bar slot or a dock zone. */
export function hitTest(position: PointerPosition, payload: DragPayload): DropTarget | null {
  const { clientX: x, clientY: y } = position;
  if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) {
    return payload.kind === "tab"
      ? { kind: "outside", screenX: position.screenX, screenY: position.screenY }
      : null;
  }
  const element = document.elementFromPoint(x, y);
  if (!element) return null;

  if (payload.kind === "tab") {
    const bar = element.closest<HTMLElement>("[data-tab-bar]");
    if (bar) {
      const tabs = [...bar.querySelectorAll<HTMLElement>("[data-tab-id]")];
      const index = tabs.findIndex((tab) => {
        const bounds = tab.getBoundingClientRect();
        return x < bounds.left + bounds.width / 2;
      });
      return {
        kind: "tabBar",
        groupId: bar.dataset.tabBar!,
        index: index < 0 ? tabs.length : index,
      };
    }
    const dock = element.closest<HTMLElement>("[data-dock-group]");
    if (!dock) return null;
    const bounds = dock.getBoundingClientRect();
    const horizontal = (x - bounds.left) / bounds.width;
    const vertical = (y - bounds.top) / bounds.height;
    const edges: [DropSide, number][] = [
      ["left", horizontal],
      ["right", 1 - horizontal],
      ["top", vertical],
      ["bottom", 1 - vertical],
    ];
    const [side, distance] = edges.reduce((nearest, edge) =>
      edge[1] < nearest[1] ? edge : nearest,
    );
    return {
      kind: "dock",
      groupId: dock.dataset.dockGroup!,
      side: distance < DOCK_EDGE_SHARE ? side : "center",
    };
  }

  const pane = element.closest<HTMLElement>("[data-drop-pane]");
  if (!pane) return null;
  const folder = element.closest<HTMLElement>("[data-drop-folder]")?.dataset.dropFolder ?? null;
  return { kind: "pane", tabId: pane.dataset.dropPane!, folder };
}

/** Swallows the click that follows a drag, so dropping does not also select a row. */
function swallowNextClick(): void {
  const swallow = (event: MouseEvent) => {
    event.stopPropagation();
    event.preventDefault();
  };
  window.addEventListener("click", swallow, { capture: true, once: true });
  window.setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
}

/**
 * Starts tracking a press that may become a drag. `payload` is built once the pointer has
 * moved far enough; `onDrop` receives the target under the pointer on release.
 */
export function beginDrag(
  event: ReactPointerEvent<HTMLElement>,
  payload: () => DragPayload | null,
  onDrop: (target: DropTarget, payload: DragPayload) => void,
): void {
  if (event.button !== 0) return;
  const source = event.currentTarget;
  const pointerId = event.pointerId;
  const startX = event.clientX;
  const startY = event.clientY;
  let dragging: DragPayload | null = null;

  const move = (moveEvent: PointerEvent) => {
    if (moveEvent.pointerId !== pointerId) return;
    if (!dragging) {
      if (Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) < DRAG_THRESHOLD) {
        return;
      }
      dragging = payload();
      if (!dragging) {
        stop();
        return;
      }
      try {
        source.setPointerCapture(pointerId);
      } catch {
        // The source may have left the document; window listeners still see the pointer.
      }
    }
    useDragStore.setState({
      payload: dragging,
      pointer: { x: moveEvent.clientX, y: moveEvent.clientY },
      target: hitTest(moveEvent, dragging),
    });
  };
  const release = (upEvent: PointerEvent) => {
    if (upEvent.pointerId !== pointerId) return;
    const finished = dragging;
    const target = useDragStore.getState().target;
    stop();
    if (finished) {
      swallowNextClick();
      if (target) onDrop(target, finished);
    }
  };
  const cancelOnEscape = (keyEvent: KeyboardEvent) => {
    if (keyEvent.key === "Escape" && dragging) stop();
  };
  const stop = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", release);
    window.removeEventListener("pointercancel", stop);
    window.removeEventListener("keydown", cancelOnEscape, true);
    useDragStore.setState({ payload: null, target: null });
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", release);
  window.addEventListener("pointercancel", stop);
  window.addEventListener("keydown", cancelOnEscape, true);
}
