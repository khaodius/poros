import { useEffect, useRef, useState } from "react";
import { IMAGE_MIME_TYPES, type ImageFormat } from "../lib/previewSniff";

export interface PictureSize {
  width: number;
  height: number;
}

interface PreviewImageProps {
  bytes: Uint8Array;
  format: ImageFormat;
  /** Told the picture's own size once it has been decoded. */
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
  const imageRef = useRef<HTMLImageElement>(null);
  const [actualSize, setActualSize] = useState(false);

  useEffect(() => {
    const url = URL.createObjectURL(
      new Blob([bytes as BlobPart], { type: IMAGE_MIME_TYPES[format] }),
    );
    if (imageRef.current) imageRef.current.src = url;
    return () => URL.revokeObjectURL(url);
  }, [bytes, format]);

  return (
    <div className={`preview-image ${actualSize ? "is-actual-size" : "is-fitted"}`}>
      <button
        type="button"
        className="preview-image-toggle"
        // Reached through the preview itself, which Enter passes on to this.
        tabIndex={-1}
        data-preview-primary
        title={actualSize ? "Fit in the pane" : "Show at actual size"}
        aria-label={actualSize ? "Fit in the pane" : "Show at actual size"}
        aria-pressed={actualSize}
        onClick={() => setActualSize((actual) => !actual)}
      >
        <img
          ref={imageRef}
          alt=""
          draggable={false}
          onLoad={(event) => {
            const { naturalWidth: width, naturalHeight: height } = event.currentTarget;
            onSize({ width, height });
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
