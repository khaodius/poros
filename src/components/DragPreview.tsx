import { useEffect, useState } from "react";
import { SquareArrowOutUpRight } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DRAG_PREVIEW_EVENT, dragPreview } from "../lib/ipc";
import type { PreviewContent } from "../lib/types";
import { matchWindowCorners } from "../lib/windowCorners";
import { matchWindowBackground } from "../lib/windowReveal";
import { applyRememberedTheme } from "../state/themeStore";
import { PorosLogo } from "./PorosLogo";
import { TAB_ICONS } from "./tabIcons";

/** Widths of the placeholder lines, in percent of the pane. */
const LINES = [58, 44, 72, 38, 64, 50, 80, 34, 60, 46, 70, 40];

/**
 * The page of the drag preview window: a small copy of the window a tab dragged out onto the
 * desktop will open in. Its window shows once the tab is drawn, in the app's current look.
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
  const TabIcon = TAB_ICONS[content.kind];
  return (
    <div className="drag-preview">
      <div className="drag-preview-titlebar">
        <PorosLogo size={12} />
        <span>Poros</span>
        <span className="drag-preview-hint">
          <SquareArrowOutUpRight size={11} />
          New window
        </span>
      </div>
      <div className="drag-preview-tabs">
        <span className="drag-preview-tab">
          <TabIcon size={12} className="tab-icon" />
          <span className="tab-label">{content.label}</span>
        </span>
      </div>
      <div className={`drag-preview-pane is-${content.kind}`}>
        {content.kind === "welcome" ? (
          <PorosLogo size={36} className="drag-preview-mark" />
        ) : (
          LINES.map((width, index) => (
            <span key={index} className="drag-preview-line">
              <span className="drag-preview-icon" />
              <span className="drag-preview-text" style={{ width: `${width}%` }} />
            </span>
          ))
        )}
      </div>
    </div>
  );
}
