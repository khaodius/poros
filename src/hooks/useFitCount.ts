import { useLayoutEffect, useState, type RefObject } from "react";

/**
 * How many of `thresholds` (ascending widths) the element is at least as wide as. Re-renders
 * only when that count changes, not on every pixel of a resize.
 */
export function useFitCount(ref: RefObject<HTMLElement | null>, thresholds: readonly number[]) {
  const [count, setCount] = useState(thresholds.length);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([observed]) => {
      const width = observed.contentRect.width;
      setCount(thresholds.filter((threshold) => threshold <= width).length);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, thresholds]);
  return Math.min(count, thresholds.length);
}
