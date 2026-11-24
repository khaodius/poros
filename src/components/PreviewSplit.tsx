import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import {
  DEFAULT_PREVIEW_SHARE,
  previewSide,
  shareAtPointer,
  storePreviewShare,
  storedPreviewShare,
  type PreviewSide,
} from "../lib/previewLayout";
import { useT } from "../i18n";

interface PreviewSplitProps {
  /** The preview pane, or null while it is closed. */
  preview: ReactNode | null;
  /** The file list. */
  children: ReactNode;
}

/**
 * A pane's file list with its preview beside it, split by a draggable divider. In a pane too
 * narrow for the two side by side, the preview goes below the list.
 */
export function PreviewSplit({ preview, children }: PreviewSplitProps) {
  const t = useT();
  const hostRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const [side, setSide] = useState<PreviewSide>("side");
  const [share, setShare] = useState(storedPreviewShare);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const observer = new ResizeObserver(([entry]) => {
      setSide(previewSide(entry.contentRect.width));
    });
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  const commit = (next: number) => {
    setShare(next);
    storePreviewShare(next);
  };

  // Like the dock splits, dragging resizes the preview directly and commits on release.
  const startResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    const host = hostRef.current;
    const pane = previewRef.current;
    if (!host || !pane || event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const bounds = host.getBoundingClientRect();
    let next = share;
    let frame = 0;

    const move = (moveEvent: PointerEvent) => {
      // Released where this window could not see it: the resize ends where it is.
      if (!(moveEvent.buttons & 1)) {
        stop();
        return;
      }
      next =
        side === "side"
          ? shareAtPointer(moveEvent.clientX, bounds.left, bounds.width)
          : shareAtPointer(moveEvent.clientY, bounds.top, bounds.height);
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        pane.style.flexBasis = `${next * 100}%`;
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
    <div ref={hostRef} className={`preview-split is-${side}`}>
      <div className="preview-split-list">{children}</div>
      {preview !== null && (
        <>
          <div
            className="preview-resizer"
            role="separator"
            aria-orientation={side === "side" ? "vertical" : "horizontal"}
            aria-label={t("preview.resizePreview")}
            title={t("preview.dragToResizeDoubleClick")}
            onPointerDown={startResize}
            onDoubleClick={() => commit(DEFAULT_PREVIEW_SHARE)}
          />
          <div
            ref={previewRef}
            className="preview-split-preview"
            style={{ flexBasis: `${share * 100}%` }}
          >
            {preview}
          </div>
        </>
      )}
    </div>
  );
}
