import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { flushSync } from "react-dom";
import type { Point, Rect } from "../lib/docking";
import { boxBetween, edgeScrollSpeed, rowsInBox } from "../lib/selectionBox";
import { holdsOtherButton, swallowNextClick, swallowNextContextMenu } from "../state/dragStore";

/** How far the pointer moves before a press draws a box rather than clicking. */
const BOX_THRESHOLD = 5;

export interface RowRange {
  first: number;
  last: number;
}

export interface BoxGesture {
  /** The rows the box touches, each time they change; null while it touches none. */
  onRows: (rows: RowRange | null) => void;
  /** On release, with whether the box was drawn downward from where it started. */
  onFinish: (rows: RowRange | null, downward: boolean) => void;
  /** Escape, or another mouse button, before release. */
  onCancel: () => void;
}

export interface BoxLayout {
  rowHeight: number;
  rowCount: number;
  /** Width at the right of the content that belongs to no row. */
  rowsInset: number;
}

/**
 * Draws a box from a press in a scrolling list of rows of one height to wherever the pointer
 * goes, scrolling the list while the pointer is near or past its top or bottom edge.
 * `contentRef` holds the rows and moves as the list scrolls; the box is in its coordinates, in
 * the element `boxRef` is given to.
 *
 * The box is drawn straight onto that element as the pointer moves, rather than through a
 * render, so it keeps up with the pointer, and the rows it touches are rendered before the
 * same frame is drawn.
 */
export function useSelectionBox(
  scrollRef: RefObject<HTMLElement | null>,
  contentRef: RefObject<HTMLElement | null>,
  layout: BoxLayout,
): {
  boxRef: RefObject<HTMLDivElement | null>;
  begin: (event: ReactPointerEvent, gesture: BoxGesture) => void;
} {
  const boxRef = useRef<HTMLDivElement>(null);
  const layoutRef = useRef(layout);
  useLayoutEffect(() => {
    layoutRef.current = layout;
  });
  const stopRef = useRef<(() => void) | null>(null);
  useEffect(() => () => stopRef.current?.(), []);

  const begin = useCallback(
    (event: ReactPointerEvent, gesture: BoxGesture) => {
      const scroll = scrollRef.current;
      const content = contentRef.current;
      if (event.button !== 0 || event.pointerType === "touch" || !scroll || !content) return;
      stopRef.current?.();
      const pointerId = event.pointerId;
      const toContent = ({ x, y }: Point): Point => {
        const bounds = content.getBoundingClientRect();
        return { x: x - bounds.left, y: y - bounds.top };
      };
      const pressedAt = { x: event.clientX, y: event.clientY };
      const start = toContent(pressedAt);
      let pointer = pressedAt;
      let drawing = false;
      let cancelled = false;
      let downward = true;
      let rows: RowRange | null = null;
      // A box that never touches a row still clears the selection it replaces.
      let reported = false;
      let frame = 0;
      let lastFrameAt = 0;
      // Scrolling moves whole pixels, so slow speeds build up over several frames.
      let unscrolled = 0;

      const draw = (box: Rect | null) => {
        const element = boxRef.current;
        if (!element) return;
        if (!box) {
          delete element.dataset.drawn;
          return;
        }
        element.style.left = `${box.left}px`;
        element.style.top = `${box.top}px`;
        element.style.width = `${box.width}px`;
        element.style.height = `${box.height}px`;
        element.dataset.drawn = "";
      };

      const update = () => {
        if (cancelled) return;
        const { rowHeight, rowCount, rowsInset } = layoutRef.current;
        const end = toContent(pointer);
        const width = content.clientWidth;
        const next = boxBetween(start, end, {
          width,
          height: Math.max(content.offsetHeight, scroll.clientHeight),
        });
        downward = end.y >= start.y;
        draw(next);
        const touched = rowsInBox(next, rowHeight, rowCount, width - rowsInset);
        if (!reported || touched?.first !== rows?.first || touched?.last !== rows?.last) {
          reported = true;
          rows = touched;
          flushSync(() => gesture.onRows(rows));
        }
      };

      const scrollAtEdges = (time: number) => {
        const elapsed = lastFrameAt ? Math.min(time - lastFrameAt, 50) : 0;
        lastFrameAt = time;
        const bounds = scroll.getBoundingClientRect();
        unscrolled += edgeScrollSpeed(pointer.y, bounds.top, bounds.bottom) * elapsed;
        const step = Math.trunc(unscrolled);
        if (step !== 0) {
          scroll.scrollTop += step;
          unscrolled -= step;
        }
        frame = requestAnimationFrame(scrollAtEdges);
      };

      const move = (moved: PointerEvent) => {
        if (moved.pointerId !== pointerId) return;
        // Released where this window could not see it.
        if (!(moved.buttons & 1)) {
          finish();
          return;
        }
        if (cancelled) return;
        // Another button, such as a right click, cancels the box as Escape does.
        if (holdsOtherButton(moved)) {
          if (drawing) {
            cancel();
            swallowNextContextMenu();
          }
          return;
        }
        pointer = { x: moved.clientX, y: moved.clientY };
        if (!drawing) {
          if (Math.hypot(pointer.x - pressedAt.x, pointer.y - pressedAt.y) < BOX_THRESHOLD) return;
          drawing = true;
          frame = requestAnimationFrame(scrollAtEdges);
          scroll.addEventListener("scroll", update);
        }
        update();
      };
      const stop = () => {
        cancelAnimationFrame(frame);
        scroll.removeEventListener("scroll", update);
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", release);
        window.removeEventListener("pointercancel", finish);
        window.removeEventListener("blur", finish);
        window.removeEventListener("keydown", cancelOnEscape, true);
        stopRef.current = null;
        draw(null);
      };
      // Released elsewhere, such as to a screenshot tool that took the pointer: the rows
      // selected so far stay selected.
      const finish = () => {
        if (drawing && !cancelled) gesture.onFinish(rows, downward);
        stop();
      };
      const release = (released: PointerEvent) => {
        if (released.pointerId !== pointerId) return;
        // The click that follows would select the row under the pointer, or clear the selection.
        if (drawing) swallowNextClick();
        finish();
      };
      // The release is still to come, and its click is swallowed as after any box.
      const cancel = () => {
        cancelled = true;
        cancelAnimationFrame(frame);
        draw(null);
        gesture.onCancel();
      };
      const cancelOnEscape = (key: KeyboardEvent) => {
        if (key.key !== "Escape" || !drawing || cancelled) return;
        key.preventDefault();
        key.stopPropagation();
        cancel();
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", release);
      window.addEventListener("pointercancel", finish);
      window.addEventListener("blur", finish);
      window.addEventListener("keydown", cancelOnEscape, true);
      stopRef.current = stop;
    },
    [scrollRef, contentRef],
  );

  return { boxRef, begin };
}
