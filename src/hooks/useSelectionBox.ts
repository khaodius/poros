import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import type { Point, Rect } from "../lib/docking";
import { boxBetween, edgeScrollSpeed, rowsInBox } from "../lib/selectionBox";
import { swallowNextClick } from "../state/dragStore";

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
  /** Escape before release. */
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
 * `contentRef` holds the rows and moves as the list scrolls; the box is in its coordinates.
 */
export function useSelectionBox(
  scrollRef: RefObject<HTMLElement | null>,
  contentRef: RefObject<HTMLElement | null>,
  layout: BoxLayout,
): { box: Rect | null; begin: (event: ReactPointerEvent, gesture: BoxGesture) => void } {
  const [box, setBox] = useState<Rect | null>(null);
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

      const update = () => {
        if (cancelled) return;
        const { rowHeight, rowCount, rowsInset } = layoutRef.current;
        const end = toContent(pointer);
        const next = boxBetween(start, end, {
          width: content.clientWidth,
          height: Math.max(content.offsetHeight, scroll.clientHeight),
        });
        downward = end.y >= start.y;
        setBox(next);
        const touched = rowsInBox(next, rowHeight, rowCount, content.clientWidth - rowsInset);
        if (!reported || touched?.first !== rows?.first || touched?.last !== rows?.last) {
          reported = true;
          rows = touched;
          gesture.onRows(rows);
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
        if (moved.pointerId !== pointerId || cancelled) return;
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
        setBox(null);
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
      const cancelOnEscape = (key: KeyboardEvent) => {
        if (key.key !== "Escape" || !drawing || cancelled) return;
        key.preventDefault();
        key.stopPropagation();
        cancelled = true;
        cancelAnimationFrame(frame);
        setBox(null);
        gesture.onCancel();
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

  return { box, begin };
}
