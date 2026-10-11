import {
  between,
  floatingRect,
  smoothstep,
  tearingOut,
  type Point,
  type Rect,
} from "../lib/docking";
import type { DropTarget, TabDrag } from "./dragStore";
import { tornOutFrame } from "./tabActions";

/** A lifted pane as drawn: where it floats in the dock, and how far it has turned into a window. */
export interface LiftedPane {
  rect: Rect;
  /** From 0, the pane, to 1, the window it opens as when dropped off Poros. */
  toward: number;
}

/**
 * Where a lifted pane floats in the dock, and how far it has turned into the window it would
 * open as. Where no preview can follow it off the window, it is held inside the dock instead
 * while a drop would leave, so it can say where the tab will go.
 */
export function liftedPane(
  payload: TabDrag,
  pointer: Point,
  target: DropTarget | null,
  desktopPreview: boolean,
): LiftedPane {
  const { dock } = payload;
  const rect = floatingRect(payload, { x: pointer.x - dock.left, y: pointer.y - dock.top });
  const leaving = target?.kind === "outside" || target?.kind === "window";
  if (!desktopPreview) {
    if (!leaving) return { rect, toward: 0 };
    const clamp = (value: number, room: number) => Math.min(Math.max(0, value), Math.max(0, room));
    const held = {
      ...rect,
      left: clamp(rect.left, dock.width - rect.width),
      top: clamp(rect.top, dock.height - rect.height),
    };
    return { rect: held, toward: 0 };
  }
  const toward = tearingOut(pointer, dock, {
    width: window.innerWidth,
    height: window.innerHeight,
  });
  if (toward <= 0) return { rect, toward };
  const opened = tornOutFrame(pointer);
  const inDock = { ...opened, left: opened.left - dock.left, top: opened.top - dock.top };
  return { rect: between(rect, inDock, toward), toward };
}

/** How much of the pane's own content and of the drawing of its window show, `toward` along. */
export function liftedLayers(toward: number): { content: number; sketch: number } {
  // The content gives way to the drawing in the first half of the way.
  const sketch = smoothstep(toward * 2);
  return { content: 1 - sketch, sketch };
}
