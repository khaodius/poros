import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { BottomPanel } from "./BottomPanel";
import { DockArea } from "./DockArea";

const STORAGE_KEY = "poros.bottomPanelShare";
const DEFAULT_SHARE = 0.3;
const SHARE_LIMITS = { min: 0.12, max: 0.75 };

function clampShare(share: number): number {
  return Math.min(SHARE_LIMITS.max, Math.max(SHARE_LIMITS.min, share));
}

function readShare(): number {
  try {
    const stored = Number(localStorage.getItem(STORAGE_KEY));
    return stored > 0 ? clampShare(stored) : DEFAULT_SHARE;
  } catch {
    return DEFAULT_SHARE;
  }
}

/** The docked panes above the transfer queue and log, split by a draggable bar. */
export function Workspace() {
  const [share, setShare] = useState(readShare);
  const containerRef = useRef<HTMLElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const commit = (next: number) => {
    setShare(next);
    try {
      localStorage.setItem(STORAGE_KEY, String(next));
    } catch {
      // The panel then opens at its default height next time.
    }
  };

  // Like the dock splits, dragging resizes the panel directly and commits on release.
  const startResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    const panel = panelRef.current;
    if (!container || !panel || event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const bounds = container.getBoundingClientRect();
    let next = share;
    let frame = 0;

    const move = (moveEvent: PointerEvent) => {
      next = clampShare((bounds.bottom - moveEvent.clientY) / Math.max(1, bounds.height));
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        panel.style.flexBasis = `${next * 100}%`;
      });
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      commit(next);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  };

  return (
    <main ref={containerRef} className="workspace">
      <div className="workspace-main">
        <DockArea />
      </div>
      <div
        className="workspace-resizer"
        role="separator"
        aria-orientation="horizontal"
        title="Drag to resize, double-click to reset"
        onPointerDown={startResize}
        onDoubleClick={() => commit(DEFAULT_SHARE)}
      />
      <div ref={panelRef} className="workspace-bottom" style={{ flexBasis: `${share * 100}%` }}>
        <BottomPanel />
      </div>
    </main>
  );
}
