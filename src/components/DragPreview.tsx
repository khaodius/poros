import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TAB_BAR_HEIGHT } from "../lib/docking";
import { DRAG_PREVIEW_EVENT, DRAG_PREVIEW_PICTURE_EVENT, dragPreview } from "../lib/ipc";
import { drawPanePicture, type PanePicture } from "../lib/panePicture";
import type { PreviewContent } from "../lib/types";
import { matchWindowCorners } from "../lib/windowCorners";
import { matchWindowBackground } from "../lib/windowReveal";
import { liftedLayers } from "../state/liftedPane";
import { applyRememberedTheme } from "../state/themeStore";
import { DropMarker } from "./DropMarker";
import { TAB_ICONS } from "./tabIcons";
import { WindowSketch } from "./WindowSketch";

/** Takes on the look of the window the pane was pictured in. */
function applyPictureTheme(picture: PanePicture): void {
  const root = document.documentElement;
  root.style.cssText = picture.rootStyle;
  if (picture.theme) root.dataset.theme = picture.theme;
  else delete root.dataset.theme;
}

/**
 * The page of the drag preview window, which draws a dragged tab where the window it is dragged
 * in cannot: the lifted pane as it reaches past that window's edge, turning into the window it
 * will open in, and that window once the pointer is off Poros. Its window shows once it has
 * drawn, in the app's current look.
 */
export function DragPreview() {
  const [content, setContent] = useState<PreviewContent | null>(null);
  const [picture, setPicture] = useState<PanePicture | null>(null);
  const pictureRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const window = getCurrentWindow();
    const listening = Promise.all([
      window.listen<PreviewContent>(DRAG_PREVIEW_EVENT, (event) => setContent(event.payload)),
      window.listen<PanePicture>(DRAG_PREVIEW_PICTURE_EVENT, (event) => setPicture(event.payload)),
    ]);
    void listening
      .then(() => dragPreview.ready())
      .then((initial) => {
        if (initial.picture) setPicture(initial.picture);
        if (initial.content) setContent(initial.content);
      })
      .catch(() => undefined);
    return () => void listening.then((unlisten) => unlisten.forEach((stop) => stop()));
  }, []);

  const drawn = content !== null && picture?.drag === content.drag ? picture : null;
  useLayoutEffect(() => {
    if (drawn) applyPictureTheme(drawn);
    else applyRememberedTheme();
    matchWindowCorners();
    const target = pictureRef.current;
    if (drawn && target) drawPanePicture(target, drawn);
  }, [drawn]);

  useEffect(() => {
    if (!content) return;
    void matchWindowBackground()
      .then(() => dragPreview.reveal())
      .catch(() => undefined);
  }, [content]);

  if (!content) return null;
  const TabIcon = TAB_ICONS[content.kind];
  // Without a picture of the pane, the drawing of its window stands in for it.
  const layers = drawn ? liftedLayers(content.toward) : { content: 0, sketch: 1 };
  const { drop } = content;
  return (
    <div className="drag-preview">
      <div className="dock-floating-title" style={{ opacity: layers.content }}>
        <TabIcon size={14} className={`tab-icon tab-icon-${content.kind}`} />
        <span className="tab-label">{content.label}</span>
      </div>
      <div
        ref={pictureRef}
        className="tab-panel drag-preview-picture"
        style={{
          top: TAB_BAR_HEIGHT - 1,
          width: drawn?.width,
          height: drawn?.height,
          opacity: layers.content,
        }}
      />
      <WindowSketch
        content={content}
        className="drag-preview-sketch"
        style={{ opacity: layers.sketch }}
      />
      {drop && (
        <DropMarker
          side={drop.side}
          style={{ left: drop.left, top: drop.top, width: drop.width, height: drop.height }}
        />
      )}
    </div>
  );
}
