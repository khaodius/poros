import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DRAG_PREVIEW_EVENT, dragPreview } from "../lib/ipc";
import type { PreviewContent } from "../lib/types";
import { matchWindowCorners } from "../lib/windowCorners";
import { matchWindowBackground } from "../lib/windowReveal";
import { applyRememberedTheme } from "../state/themeStore";
import { WindowSketch } from "./WindowSketch";

/**
 * The page of the drag preview window: the window a tab dragged out onto the desktop will open
 * in, at its size. Its window shows once the tab is drawn, in the app's current look.
 */
export function DragPreview() {
  const [content, setContent] = useState<PreviewContent | null>(null);

  useEffect(() => {
    const listening = getCurrentWindow().listen<PreviewContent>(DRAG_PREVIEW_EVENT, (event) =>
      setContent(event.payload),
    );
    void listening
      .then(() => dragPreview.ready())
      .then((initial) => {
        if (initial) setContent(initial);
      })
      .catch(() => undefined);
    return () => void listening.then((unlisten) => unlisten());
  }, []);

  useEffect(() => {
    if (!content) return;
    applyRememberedTheme();
    matchWindowCorners();
    void matchWindowBackground()
      .then(() => dragPreview.reveal())
      .catch(() => undefined);
  }, [content]);

  if (!content) return null;
  return <WindowSketch content={content} className="drag-preview" />;
}
