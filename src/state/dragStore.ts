// Drags inside a window use pointer events rather than HTML drag and drop: Tauri takes over
// native drag and drop on Windows to report files dropped from outside the app.

import type { PointerEvent as ReactPointerEvent } from "react";
import { create } from "zustand";
import {
  changesLayout,
  dropSide,
  floatingFrame,
  floatingRect,
  groupNearest,
  groupRects,
  rectCenter,
  TAB_BAR_HEIGHT,
  type Point,
  type Rect,
  type Size,
} from "../lib/docking";
import type { DropSide } from "../lib/layout";
import type { FileEntry } from "../lib/types";
import { useLayoutStore } from "./layoutStore";

export interface TabDrag {
  kind: "tab";
  tabId: string;
  label: string;
  sourceGroupId: string;
  /** Where the dock sits in the window. */
  dock: Rect;
  /** Where the pointer holds the lifted pane, from its top left corner. */
  grab: Point;
  size: Size;
}

export type DragPayload =
  | { kind: "files"; sourceTabId: string; entries: FileEntry[] }
  | TabDrag
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
  /** The copy key is held: Ctrl, or Option on macOS. Files dropped then are copied. */
  copy: boolean;
}

export const useDragStore = create<DragState>(() => ({
  payload: null,
  target: null,
  pointer: { x: 0, y: 0 },
  copy: false,
}));

const DRAG_THRESHOLD = 5;

const COPY_KEY = navigator.userAgent.includes("Mac") ? "altKey" : "ctrlKey";

function holdsCopyKey(event: MouseEvent | KeyboardEvent): boolean {
  return event[COPY_KEY];
}

interface PointerPosition {
  clientX: number;
  clientY: number;
  screenX: number;
  screenY: number;
}

/** Builds a tab drag from the pressed tab button; the pane lifts out at its own size. */
export function tabDrag(
  button: HTMLElement,
  tabId: string,
  label: string,
  start: Point,
): TabDrag | null {
  const frame = button.closest<HTMLElement>("[data-dock-group]");
  const dock = button.closest<HTMLElement>(".dock");
  if (!frame || !dock) return null;
  const { left, top, width, height } = dock.getBoundingClientRect();
  const dockBounds = { left, top, width, height };
  return {
    kind: "tab",
    tabId,
    label,
    sourceGroupId: frame.dataset.dockGroup!,
    dock: dockBounds,
    ...floatingFrame(frame.getBoundingClientRect(), dockBounds, start),
  };
}

/** A dragged tab's pane stays in place while the pointer is on its own tab bar. */
export function isLifted(payload: TabDrag, target: DropTarget | null): boolean {
  return !(target?.kind === "tabBar" && target.groupId === payload.sourceGroupId);
}

/** The insertion index for a pointer `offset` pixels from the left of a group's tab bar. */
function tabIndexAt(groupId: string, offset: number): number {
  const bar = document.querySelector<HTMLElement>(`[data-tab-bar="${CSS.escape(groupId)}"]`);
  if (!bar) return 0;
  const tabs = [...bar.querySelectorAll<HTMLElement>("[data-tab-id]")];
  const index = tabs.findIndex(
    (tab) => offset + bar.scrollLeft < tab.offsetLeft + tab.offsetWidth / 2,
  );
  return index < 0 ? tabs.length : index;
}

/**
 * A tab bar under the pointer, else the group under the middle of the lifted pane: dropped over
 * the group's middle it joins the group's tabs, and toward an edge it splits the group there.
 */
function tabTarget(
  { clientX: x, clientY: y }: PointerPosition,
  payload: TabDrag,
): DropTarget | null {
  const { root } = useLayoutStore.getState();
  const groups = groupRects(root, payload.dock);
  const bar = groups.find(
    ({ rect }) =>
      x >= rect.left &&
      x < rect.left + rect.width &&
      y >= rect.top &&
      y < rect.top + TAB_BAR_HEIGHT,
  );
  if (bar) {
    return {
      kind: "tabBar",
      groupId: bar.groupId,
      index: tabIndexAt(bar.groupId, x - bar.rect.left),
    };
  }
  const { dock } = payload;
  const center = rectCenter(floatingRect(payload, { x, y }));
  const inDock =
    center.x >= dock.left &&
    center.x < dock.left + dock.width &&
    center.y >= dock.top &&
    center.y < dock.top + dock.height;
  const under = inDock ? groupNearest(groups, center) : null;
  if (!under) return null;
  const side = dropSide(under.rect, center);
  return changesLayout(root, payload.tabId, under.groupId, side)
    ? { kind: "dock", groupId: under.groupId, side }
    : null;
}

/** Finds what is under the pointer: a folder row, a pane, a tab bar slot or a dock zone. */
export function hitTest(position: PointerPosition, payload: DragPayload): DropTarget | null {
  const { clientX: x, clientY: y } = position;
  if (x < 0 || y < 0 || x > window.innerWidth || y > window.innerHeight) {
    return payload.kind === "tab"
      ? { kind: "outside", screenX: position.screenX, screenY: position.screenY }
      : null;
  }
  if (payload.kind === "tab") return tabTarget(position, payload);
  const element = document.elementFromPoint(x, y);
  if (!element) return null;
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
 * moved far enough; `onDrop` receives the target under the pointer on release, and whether
 * the copy key was held.
 */
export function beginDrag(
  event: ReactPointerEvent<HTMLElement>,
  payload: () => DragPayload | null,
  onDrop: (target: DropTarget, payload: DragPayload, copy: boolean) => void,
): void {
  if (event.button !== 0) return;
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
        // The body, since the source can unmount while the layout previews a drop.
        document.body.setPointerCapture(pointerId);
      } catch {
        // The pointer may already be up; window listeners still see the release.
      }
    }
    useDragStore.setState({
      payload: dragging,
      pointer: { x: moveEvent.clientX, y: moveEvent.clientY },
      target: hitTest(moveEvent, dragging),
      copy: holdsCopyKey(moveEvent),
    });
  };
  const release = (upEvent: PointerEvent) => {
    if (upEvent.pointerId !== pointerId) return;
    const finished = dragging;
    const target = useDragStore.getState().target;
    stop();
    if (finished) {
      swallowNextClick();
      if (target) onDrop(target, finished, holdsCopyKey(upEvent));
    }
  };
  const followKeys = (keyEvent: KeyboardEvent) => {
    if (!dragging) return;
    if (keyEvent.type === "keydown" && keyEvent.key === "Escape") stop();
    else useDragStore.setState({ copy: holdsCopyKey(keyEvent) });
  };
  const stop = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", release);
    window.removeEventListener("pointercancel", stop);
    window.removeEventListener("keydown", followKeys, true);
    window.removeEventListener("keyup", followKeys, true);
    useDragStore.setState({ payload: null, target: null, copy: false });
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", release);
  window.addEventListener("pointercancel", stop);
  window.addEventListener("keydown", followKeys, true);
  window.addEventListener("keyup", followKeys, true);
}
