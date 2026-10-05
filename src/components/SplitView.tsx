import { useCallback, useRef, useState, type PointerEvent, type ReactNode } from "react";

interface SplitViewProps {
  direction: "horizontal" | "vertical";
  storageKey: string;
  /** Share of the first panel, 0 to 1. */
  defaultRatio: number;
  minRatio?: number;
  maxRatio?: number;
  first: ReactNode;
  second: ReactNode;
}

function readRatio(storageKey: string, fallback: number): number {
  try {
    const stored = Number(localStorage.getItem(storageKey));
    return stored > 0 && stored < 1 ? stored : fallback;
  } catch {
    return fallback;
  }
}

export function SplitView({
  direction,
  storageKey,
  defaultRatio,
  minRatio = 0.15,
  maxRatio = 0.85,
  first,
  second,
}: SplitViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [ratio, setRatio] = useState(() => readRatio(storageKey, defaultRatio));
  const horizontal = direction === "horizontal";

  const persist = useCallback(
    (value: number) => {
      try {
        localStorage.setItem(storageKey, String(value));
      } catch {
        // Storage can be unavailable; the split then resets on restart.
      }
    },
    [storageKey],
  );

  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const bounds = container.getBoundingClientRect();
    let latest = ratio;
    const move = (moveEvent: globalThis.PointerEvent) => {
      const offset = horizontal ? moveEvent.clientX - bounds.left : moveEvent.clientY - bounds.top;
      const size = horizontal ? bounds.width : bounds.height;
      latest = Math.min(maxRatio, Math.max(minRatio, offset / size));
      setRatio(latest);
    };
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      persist(latest);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  const resetRatio = () => {
    setRatio(defaultRatio);
    persist(defaultRatio);
  };

  return (
    <div ref={containerRef} className={`split split-${direction}`}>
      <div className="split-panel" style={{ flexBasis: `${ratio * 100}%` }}>
        {first}
      </div>
      <div
        className="split-handle"
        role="separator"
        aria-orientation={horizontal ? "vertical" : "horizontal"}
        onPointerDown={startDrag}
        onDoubleClick={resetRatio}
      />
      <div className="split-panel" style={{ flexBasis: `${(1 - ratio) * 100}%` }}>
        {second}
      </div>
    </div>
  );
}
