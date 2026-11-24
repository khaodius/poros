import { useEffect, useMemo, useRef, useState } from "react";
import { fitInside } from "../lib/previewLayout";
import { IMAGE_MIME_TYPES, drawingSize, type ImageFormat } from "../lib/previewSniff";
import { useT } from "../i18n";

export interface PictureSize {
  width: number;
  height: number;
}

/** What CSS gives a picture that has no size of its own, for the rare one that decodes so. */
const DEFAULT_OBJECT_SIZE: PictureSize = { width: 300, height: 150 };

interface PreviewImageProps {
  bytes: Uint8Array;
  format: ImageFormat;
  /** Told the picture's own size once it is known. A drawing that gives none tells nothing. */
  onSize: (size: PictureSize) => void;
  /** The web view could not decode the picture. */
  onBroken: () => void;
}

/**
 * A picture scaled to fit, on a checkerboard that shows where it is see-through. A click
 * switches between fitting it in the pane and its actual size.
 *
 * The picture is only ever the source of an image element, read from memory: an image element
 * runs no scripts and loads nothing a file points at, so a server's drawing stays inert.
 */
export function PreviewImage({ bytes, format, onSize, onBroken }: PreviewImageProps) {
  const t = useT();
  const frameRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const [actualSize, setActualSize] = useState(false);
  const isDrawing = format === "svg";
  const declared = useMemo(() => (isDrawing ? drawingSize(bytes) : null), [isDrawing, bytes]);
  /** The size the web view decoded these bytes at. */
  const [decoded, setDecoded] = useState<{ bytes: Uint8Array; size: PictureSize } | null>(null);
  /** The space the picture is fitted in. */
  const [room, setRoom] = useState<PictureSize | null>(null);

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

  const natural = decoded?.bytes === bytes ? decoded.size : null;
  // A drawing that gives no size takes its shape from what the web view made of it.
  const own = declared ?? natural;
  let shown: PictureSize | null = null;
  if (own) shown = actualSize ? own : fitInside(own.width, own.height, room ?? own, isDrawing);

  return (
    <div ref={frameRef} className={`preview-image ${actualSize ? "is-actual-size" : "is-fitted"}`}>
      <button
        type="button"
        className="preview-image-toggle"
        // Reached through the preview itself, which Enter passes on to this.
        tabIndex={-1}
        data-preview-primary
        title={actualSize ? t("preview.fitInThePane") : t("preview.showAtActualSize")}
        aria-label={actualSize ? t("preview.fitInThePane") : t("preview.showAtActualSize")}
        aria-pressed={actualSize}
        onClick={() => setActualSize((actual) => !actual)}
      >
        <img
          ref={imageRef}
          alt=""
          draggable={false}
          // Hidden until its size is known, so it never shows at another size first.
          style={shown ? { width: shown.width, height: shown.height } : { visibility: "hidden" }}
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
      </button>
    </div>
  );
}
