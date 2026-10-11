import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  DEFAULT_PANEL_SHARE,
  clampPanelShare,
  storePanelShare,
  storedPanelShare,
} from "../lib/panelShare";
import { BottomPanel } from "./BottomPanel";
import { DockArea } from "./DockArea";

/** The docked panes above the transfer queue and log, split by a draggable bar. */
export function Workspace() {
  const [share, setShare] = useState(storedPanelShare);
  const containerRef = useRef<HTMLElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const commit = (next: number) => {
    setShare(next);
    storePanelShare(next);
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
      // Released where this window could not see it: the resize ends where it is.
      if (!(moveEvent.buttons & 1)) {
        stop();
        return;
      }
      next = clampPanelShare((bounds.bottom - moveEvent.clientY) / Math.max(1, bounds.height));
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
      window.removeEventListener("blur", stop);
      commit(next);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
    // Another program took the pointer, such as a screenshot tool, and gets the release.
    window.addEventListener("blur", stop);
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
        onDoubleClick={() => commit(DEFAULT_PANEL_SHARE)}
      />
      <div ref={panelRef} className="workspace-bottom" style={{ flexBasis: `${share * 100}%` }}>
        <BottomPanel />
      </div>
    </main>
  );
}
