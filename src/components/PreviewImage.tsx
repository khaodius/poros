import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { Minus, Plus } from "lucide-react";
import { useLatest } from "../hooks/useLatest";
import { usePreviewKeys } from "../hooks/usePreviewKeys";
import { formatPercent } from "../lib/format";
import { IMAGE_MIME_TYPES, drawingSize, type ImageFormat } from "../lib/previewSniff";
import {
  anchoredScroll,
  clampZoom,
  fitScale,
  pictureSpan,
  sameScale,
  stepZoom,
  wheelZoomFactor,
  zoomLimits,
  type PictureSize,
} from "../lib/previewZoom";
import { useT } from "../i18n";

export type { PictureSize };

/** What CSS gives a picture that has no size of its own, for the rare one that decodes so. */
const DEFAULT_OBJECT_SIZE: PictureSize = { width: 300, height: 150 };
/** A press that moves the pointer further than this drags the picture rather than clicks it. */
const DRAG_DISTANCE = 4;
const ZOOM_ICON_SIZE = 12;

interface Point {
  x: number;
  y: number;
}

/** The zoom asked for, and the scroll that keeps the point it was asked at in place. */
interface Aim {
  zoom: number | null;
  scroll: Point;
}

interface PreviewImageProps {
  bytes: Uint8Array;
  format: ImageFormat;
  /** Told the picture's own size once it is known. A drawing that gives none tells nothing. */
  onSize: (size: PictureSize) => void;
  /** The web view could not decode the picture. */
  onBroken: () => void;
}

/**
 * A picture on a checkerboard that shows where it is see-through, fitted in the pane until it is
 * zoomed: with the wheel or a pinch at the pointer, with the small buttons in its corner, or
 * with + and - while the preview has the keyboard. A click switches between fitting the pane
 * and the picture's actual size, and a zoomed picture is dragged to see the rest of it.
 *
 * The picture is only ever the source of an image element, read from memory: an image element
 * runs no scripts and loads nothing a file points at, so a server's drawing stays inert.
 */
export function PreviewImage({ bytes, format, onSize, onBroken }: PreviewImageProps) {
  const t = useT();
  const frameRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const isDrawing = format === "svg";
  const declared = useMemo(() => (isDrawing ? drawingSize(bytes) : null), [isDrawing, bytes]);
  /** The size the web view decoded these bytes at. */
  const [decoded, setDecoded] = useState<{ bytes: Uint8Array; size: PictureSize } | null>(null);
  /** The space the picture is shown in. */
  const [room, setRoom] = useState<PictureSize | null>(null);
  /**
   * The scale zoomed to, null while the picture fits the pane, as it goes on doing when the pane
   * changes size. A new object for every zoom, so the scroll that goes with it is always made.
   */
  const [view, setView] = useState<{ zoom: number | null }>({ zoom: null });
  const [dragging, setDragging] = useState(false);
  /** A zoom asked for and not yet drawn: wheel events come faster than the page draws. */
  const aim = useRef<Aim | null>(null);

  useEffect(() => {
    const url = URL.createObjectURL(
      new Blob([bytes as BlobPart], { type: IMAGE_MIME_TYPES[format] }),
    );
    if (imageRef.current) imageRef.current.src = url;
    return () => URL.revokeObjectURL(url);
  }, [bytes, format]);

  // Measured rather than left to CSS: a drawing may have no size of its own for CSS to scale,
  // and a picture fitted by CSS alone keeps a box wider than itself, checkerboard and all.
  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setRoom({ width, height });
    });
    observer.observe(frame);
    return () => observer.disconnect();
  }, []);

  useLayoutEffect(() => {
    const frame = frameRef.current;
    const target = aim.current;
    aim.current = null;
    if (!frame || !target) return;
    frame.scrollLeft = target.scroll.x;
    frame.scrollTop = target.scroll.y;
  }, [view]);

  const natural = decoded?.bytes === bytes ? decoded.size : null;
  // A drawing that gives no size takes its shape from what the web view made of it.
  const own = declared ?? natural;
  const fit = own && room ? fitScale(own, room, isDrawing) : null;
  const zoom = view.zoom;
  const scale = fit === null ? null : (zoom ?? fit);
  const shown =
    own && scale !== null
      ? {
          width: Math.max(1, Math.floor(own.width * scale)),
          height: Math.max(1, Math.floor(own.height * scale)),
        }
      : null;
  const across = room && shown ? pictureSpan(room.width, shown.width) : null;
  const down = room && shown ? pictureSpan(room.height, shown.height) : null;
  const canPan =
    room !== null &&
    across !== null &&
    down !== null &&
    (across.extent > room.width || down.extent > room.height);
  const limits = fit === null ? null : zoomLimits(fit);

  /** The scale last asked for, drawn or not. */
  const aimedScale = () => {
    if (fit === null) return null;
    const aimed = aim.current ? aim.current.zoom : zoom;
    return aimed ?? fit;
  };

  /**
   * Zooms to `target`, or to fit the pane for null, keeping the point at `anchor` in the view,
   * the middle of the view by default, where it is.
   */
  const zoomTo = (target: number | null, anchor?: Point) => {
    const frame = frameRef.current;
    const before = aimedScale();
    if (!frame || !own || !room || fit === null || before === null) return;
    const next = target === null ? fit : clampZoom(target, zoomLimits(fit));
    const fitted = sameScale(next, fit);
    const wasFitted = (aim.current ? aim.current.zoom : zoom) === null;
    if (sameScale(next, before) && fitted === wasFitted) return;
    const point = anchor ?? { x: room.width / 2, y: room.height / 2 };
    const from = aim.current?.scroll ?? { x: frame.scrollLeft, y: frame.scrollTop };
    const scrollAlong = (viewLength: number, at: number, ownLength: number, scrolled: number) =>
      anchoredScroll(
        { view: viewLength, scroll: scrolled, anchor: at },
        ownLength * before,
        ownLength * next,
      );
    aim.current = {
      zoom: fitted ? null : next,
      scroll: {
        x: scrollAlong(room.width, point.x, own.width, from.x),
        y: scrollAlong(room.height, point.y, own.height, from.y),
      },
    };
    setView({ zoom: aim.current.zoom });
  };

  /** A click: the actual size at the point clicked, or back to fitting the pane. */
  const toggle = (anchor?: Point) => {
    const aimed = aim.current ? aim.current.zoom : zoom;
    zoomTo(aimed === null ? 1 : null, anchor);
  };

  const step = (direction: 1 | -1) => {
    const before = aimedScale();
    if (before !== null && fit !== null) zoomTo(stepZoom(before, direction, fit));
  };

  usePreviewKeys((event) => {
    if (fit === null) return false;
    if (event.key === "+" || event.key === "=") step(1);
    else if (event.key === "-" || event.key === "_") step(-1);
    else if (event.key === "0") zoomTo(null);
    else return false;
    return true;
  });

  const latest = useLatest({ zoomTo, toggle, aimedScale });

  // Not a React handler: those cannot keep the page from scrolling, or the window from taking a
  // pinch, which comes as a wheel with Ctrl held, as a change of text size.
  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const handleWheel = (event: WheelEvent) => {
      // Sideways, as Shift with the wheel or a touchpad gives: the view scrolls as usual.
      if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) return;
      const before = latest.current.aimedScale();
      if (before === null) return;
      event.preventDefault();
      event.stopPropagation();
      const bounds = frame.getBoundingClientRect();
      const factor = wheelZoomFactor(event.deltaY, event.deltaMode, event.ctrlKey);
      latest.current.zoomTo(before * factor, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
    };
    frame.addEventListener("wheel", handleWheel, { passive: false });
    return () => frame.removeEventListener("wheel", handleWheel);
  }, [latest]);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    const frame = event.currentTarget;
    if (event.button !== 0 || !shown) return;
    const bounds = frame.getBoundingClientRect();
    const point = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
    // On a scroll bar, which scrolls by itself.
    if (point.x >= frame.clientWidth || point.y >= frame.clientHeight) return;
    const start = { left: frame.scrollLeft, top: frame.scrollTop };
    const pans = frame.scrollWidth > frame.clientWidth || frame.scrollHeight > frame.clientHeight;
    let moved = false;
    frame.setPointerCapture(event.pointerId);

    const move = (moveEvent: PointerEvent) => {
      const right = moveEvent.clientX - event.clientX;
      const down = moveEvent.clientY - event.clientY;
      if (!moved && Math.hypot(right, down) < DRAG_DISTANCE) return;
      if (!moved && pans) setDragging(true);
      moved = true;
      if (!pans) return;
      frame.scrollLeft = start.left - right;
      frame.scrollTop = start.top - down;
    };
    const end = (endEvent: PointerEvent) => {
      frame.removeEventListener("pointermove", move);
      frame.removeEventListener("pointerup", end);
      frame.removeEventListener("pointercancel", end);
      setDragging(false);
      if (!moved && endEvent.type === "pointerup") latest.current.toggle(point);
    };
    frame.addEventListener("pointermove", move);
    frame.addEventListener("pointerup", end);
    frame.addEventListener("pointercancel", end);
  };

  let cursor = "is-fitted";
  if (dragging) cursor = "is-dragging";
  else if (canPan) cursor = "can-pan";
  else if (zoom !== null) cursor = "is-zoomed";

  return (
    <div className="preview-image">
      <div
        ref={frameRef}
        className={`preview-image-frame ${cursor}`}
        onPointerDown={handlePointerDown}
      >
        <div
          className="preview-image-canvas"
          style={across && down ? { width: across.extent, height: down.extent } : undefined}
        >
          <img
            ref={imageRef}
            alt=""
            draggable={false}
            // Hidden until its size is known, so it never shows at another size first.
            style={
              shown && across && down
                ? {
                    left: across.start,
                    top: down.start,
                    width: shown.width,
                    height: shown.height,
                  }
                : { visibility: "hidden" }
            }
            onLoad={(event) => {
              const { naturalWidth: width, naturalHeight: height } = event.currentTarget;
              const known = width > 0 && height > 0;
              setDecoded({ bytes, size: known ? { width, height } : DEFAULT_OBJECT_SIZE });
              if (declared) onSize(declared);
              else if (!isDrawing) onSize({ width, height });
            }}
            onError={(event) => {
              // Leaving one picture for the next ends its load, which is no failure.
              if (event.currentTarget.getAttribute("src")) onBroken();
            }}
          />
        </div>
      </div>
      {scale !== null && limits && (
        <div className="preview-zoom" role="group" aria-label={t("preview.zoom")}>
          <button
            type="button"
            className="preview-zoom-button"
            // Reached through the preview itself, which passes + and - on.
            tabIndex={-1}
            title={t("preview.zoomOut")}
            aria-label={t("preview.zoomOut")}
            disabled={scale <= limits.min || sameScale(scale, limits.min)}
            onClick={() => step(-1)}
          >
            <Minus size={ZOOM_ICON_SIZE} aria-hidden />
          </button>
          <button
            type="button"
            className="preview-zoom-level"
            tabIndex={-1}
            // Enter in the preview presses it, as a click on the picture does.
            data-preview-primary
            title={zoom === null ? t("preview.showAtActualSize") : t("preview.fitInThePane")}
            aria-label={zoom === null ? t("preview.showAtActualSize") : t("preview.fitInThePane")}
            onClick={() => toggle()}
          >
            {formatPercent(scale * 100)}
          </button>
          <button
            type="button"
            className="preview-zoom-button"
            tabIndex={-1}
            title={t("preview.zoomIn")}
            aria-label={t("preview.zoomIn")}
            disabled={scale >= limits.max || sameScale(scale, limits.max)}
            onClick={() => step(1)}
          >
            <Plus size={ZOOM_ICON_SIZE} aria-hidden />
          </button>
        </div>
      )}
    </div>
  );
}
