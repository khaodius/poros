import { useEffect } from "react";
import { WHEEL_STEP_MIN, glideAt, nextGlide, wheelDistance, type Glide } from "../lib/smoothScroll";

const SCROLLING_OVERFLOW = new Set(["auto", "scroll", "overlay"]);

interface Gliding extends Glide {
  /** Where the glide last put the list, to notice when something else scrolls it. */
  placed: number;
}

/**
 * The innermost list under `target` that can still scroll that way: one at its end hands the
 * wheel to the list around it, as scrolling does without gliding.
 */
function scrollerFor(
  target: Element,
  direction: number,
  glides: Map<HTMLElement, Gliding>,
): HTMLElement | null {
  for (let element: Element | null = target; element; element = element.parentElement) {
    if (!(element instanceof HTMLElement)) continue;
    if (!SCROLLING_OVERFLOW.has(getComputedStyle(element).overflowY)) continue;
    const limit = element.scrollHeight - element.clientHeight;
    if (limit <= 0) continue;
    const position = glides.get(element)?.to ?? element.scrollTop;
    if (direction < 0 ? position > 0 : position < limit - 0.5) return element;
  }
  return null;
}

/** While `enabled`, steps of the mouse wheel glide whatever list or panel they scroll. */
export function useSmoothScrolling(enabled: boolean) {
  useEffect(() => {
    if (!enabled) return;
    const glides = new Map<HTMLElement, Gliding>();
    let frame = 0;

    const step = (now: number) => {
      for (const [element, glide] of glides) {
        // Something else scrolled it, such as its scrollbar, a key or a jump to a row.
        if (Math.abs(element.scrollTop - glide.placed) > 1 || !element.isConnected) {
          glides.delete(element);
          continue;
        }
        const { position, done } = glideAt(glide, now);
        element.scrollTop = position;
        glide.placed = element.scrollTop;
        if (done) glides.delete(element);
      }
      frame = glides.size > 0 ? requestAnimationFrame(step) : 0;
    };

    const handleWheel = (event: WheelEvent) => {
      if (event.defaultPrevented || event.ctrlKey || event.shiftKey || event.altKey) return;
      if (event.deltaY === 0 || Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
      if (!(event.target instanceof Element)) return;
      // Fields and the terminal scroll as they always do.
      if (event.target.closest("input, textarea, select, .xterm")) return;
      const scroller = scrollerFor(event.target, Math.sign(event.deltaY), glides);
      if (!scroller) return;
      const distance = wheelDistance(event, scroller.clientHeight);
      if (Math.abs(distance) < WHEEL_STEP_MIN) {
        glides.delete(scroller);
        return;
      }
      event.preventDefault();
      const position = scroller.scrollTop;
      const limit = scroller.scrollHeight - scroller.clientHeight;
      const glide = nextGlide(glides.get(scroller), position, distance, limit, performance.now());
      glides.set(scroller, { ...glide, placed: position });
      if (!frame) frame = requestAnimationFrame(step);
    };

    window.addEventListener("wheel", handleWheel, { passive: false });
    return () => {
      window.removeEventListener("wheel", handleWheel);
      cancelAnimationFrame(frame);
    };
  }, [enabled]);
}
