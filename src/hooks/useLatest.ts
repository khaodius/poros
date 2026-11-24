import { useLayoutEffect, useRef, type RefObject } from "react";

/**
 * The value of the latest render, for work that finishes after an await. A dialog whose
 * question changed while it worked, as when tabs join an open save question, then acts on what
 * it has become rather than on what it was when the work set out.
 */
export function useLatest<T>(value: T): RefObject<T> {
  const latest = useRef(value);
  useLayoutEffect(() => {
    latest.current = value;
  });
  return latest;
}
