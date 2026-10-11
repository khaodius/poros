import type { CSSProperties } from "react";
import { SquareArrowOutUpRight } from "lucide-react";
import { storedPanelShare } from "../lib/panelShare";
import type { PreviewContent } from "../lib/types";
import { PorosLogo } from "./PorosLogo";
import { TAB_ICONS } from "./tabIcons";

/** Widths of the placeholder lines, in percent of the pane. */
const LINES = [58, 44, 72, 38, 64, 50, 80, 34, 60, 46, 70, 40, 54, 66, 42, 76, 36, 62, 48, 68];

interface WindowSketchProps {
  content: Pick<PreviewContent, "label" | "kind">;
  className?: string;
  style?: CSSProperties;
}

/**
 * A drawing of the window a tab dragged out of Poros opens in, laid out like it at its real
 * size: its title bar, the tab, lines standing in for the pane, and the queue below. The lifted
 * pane turns into it near the window's edge, and the preview on the desktop shows it, so the
 * two match where one takes over from the other.
 */
export function WindowSketch({ content, className, style }: WindowSketchProps) {
  const TabIcon = TAB_ICONS[content.kind];
  const files = content.kind === "local" || content.kind === "remote";
  return (
    <div className={`window-sketch ${className ?? ""}`} style={style}>
      <div className="window-sketch-titlebar">
        <PorosLogo size={20} />
        <span>Poros</span>
        <span className="window-sketch-hint">
          <SquareArrowOutUpRight size={13} />
          New window
        </span>
      </div>
      <div className="window-sketch-workspace">
        <div className="window-sketch-tabs">
          <span className="window-sketch-tab">
            <TabIcon size={14} className="tab-icon" />
            <span className="tab-label">{content.label}</span>
          </span>
        </div>
        <div className={`window-sketch-pane is-${content.kind}`}>
          {files && <span className="window-sketch-bar" />}
          {content.kind === "welcome" ? (
            <PorosLogo size={56} className="window-sketch-mark" />
          ) : (
            <span className="window-sketch-lines">
              {LINES.map((width, index) => (
                <span key={index} className="window-sketch-line">
                  <span className="window-sketch-icon" />
                  <span className="window-sketch-text" style={{ width: `${width}%` }} />
                </span>
              ))}
            </span>
          )}
        </div>
        <div className="window-sketch-panel" style={{ flexBasis: `${storedPanelShare() * 100}%` }}>
          <span className="window-sketch-panel-header">
            <span>Queue</span>
            <span>Failed</span>
            <span>Completed</span>
            <span>Log</span>
          </span>
        </div>
      </div>
      <div className="window-sketch-statusbar" />
    </div>
  );
}
