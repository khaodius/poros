import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { Caps } from "../lib/previewPlan";
import { isCancelled } from "../lib/previewScheduler";
import type { FileEntry } from "../lib/types";
import {
  cachedThumbnail,
  cancelThumbnail,
  forgetThumbnail,
  requestThumbnail,
  type PreviewPlace,
  type Thumbnail,
} from "../state/previewReads";
import { FileIcon } from "./FileIcon";

/** A tile asks for its picture once it has been on show this long, not while scrolled past. */
const TILE_REST_MILLIS = 150;
/** Tiles a little outside the visible part are fetched too, so scrolling finds them ready. */
const NEAR_VIEW_MARGIN = "96px";
const PLACEHOLDER_ICON_SIZE = 28;

interface PreviewThumbnailsProps {
  place: PreviewPlace;
  /** The pictures on show, in order. */
  entries: FileEntry[];
  /** How many pictures there are, when more than are shown. */
  total: number;
  /** The longer side of a thumbnail, in pixels. */
  box: number;
  caps: Caps;
}

/**
 * Thumbnails of pictures, as placeholders at once that fill in as the pictures arrive. Only
 * tiles on show are fetched, a few at a time, and one that scrolls away before its turn is
 * dropped from the line.
 */
export function PreviewThumbnails({ place, entries, total, box, caps }: PreviewThumbnailsProps) {
  return (
    <div className="preview-grid" style={{ "--thumbnail-size": `${box}px` } as CSSProperties}>
      {entries.map((entry) => (
        <ThumbnailTile key={entry.path} place={place} entry={entry} box={box} caps={caps} />
      ))}
      {total > entries.length && (
        <p className="preview-grid-more">and {total - entries.length} more</p>
      )}
    </div>
  );
}

interface ThumbnailTileProps {
  place: PreviewPlace;
  entry: FileEntry;
  box: number;
  caps: Caps;
}

interface Drawn {
  /** The file the thumbnail is of, as its folder listed it. */
  key: string;
  /** Null when the picture could not be fetched. */
  thumbnail: Thumbnail | null;
}

function ThumbnailTile({ place, entry, box, caps }: ThumbnailTileProps) {
  const tileRef = useRef<HTMLDivElement>(null);
  const key = JSON.stringify([place.connection, entry.path, entry.size, entry.modified, box]);
  const [drawn, setDrawn] = useState<Drawn | null>(null);
  const latest = useRef({ place, entry, caps });
  useEffect(() => {
    latest.current = { place, entry, caps };
  });
  const fresh = drawn?.key === key ? drawn : null;
  const cached = cachedThumbnail(place, entry, box);
  const thumbnail = fresh ? fresh.thumbnail : cached;
  const settled = fresh !== null || cached !== undefined;

  useEffect(() => {
    const tile = tileRef.current;
    if (settled || !tile) return;
    const current = latest.current;
    let timer = 0;
    let asked = false;
    const ask = () => {
      asked = true;
      requestThumbnail(current.place, current.entry, box, current.caps).then(
        (result) => setDrawn({ key, thumbnail: result }),
        (caught: unknown) => {
          // Dropped by another pane showing the same file while this one still wants it.
          if (isCancelled(caught)) {
            if (asked) ask();
          } else {
            setDrawn({ key, thumbnail: null });
          }
        },
      );
    };
    const leave = () => {
      window.clearTimeout(timer);
      if (!asked) return;
      asked = false;
      cancelThumbnail(current.place, current.entry, box);
    };
    const observer = new IntersectionObserver(
      ([observed]) => {
        if (!observed.isIntersecting) leave();
        else if (!asked) {
          window.clearTimeout(timer);
          timer = window.setTimeout(ask, TILE_REST_MILLIS);
        }
      },
      { root: tile.closest(".preview-body"), rootMargin: NEAR_VIEW_MARGIN },
    );
    observer.observe(tile);
    return () => {
      observer.disconnect();
      leave();
    };
  }, [key, settled, box]);

  return (
    <figure className="preview-tile" title={entry.name}>
      <div ref={tileRef} className="preview-tile-picture">
        {thumbnail?.url ? (
          <img
            src={thumbnail.url}
            alt=""
            draggable={false}
            decoding="async"
            onError={() => {
              // Let go of to make room in the cache, or not a picture the web view shows.
              forgetThumbnail(place, entry, box);
              setDrawn({ key, thumbnail: null });
            }}
          />
        ) : (
          <span className={settled ? "preview-tile-icon" : "preview-tile-icon is-pending"}>
            <FileIcon entry={entry} size={PLACEHOLDER_ICON_SIZE} />
          </span>
        )}
      </div>
      <figcaption>{entry.name}</figcaption>
    </figure>
  );
}
