import type { CSSProperties } from "react";
import { Columns2, Layers, Rows2 } from "lucide-react";
import type { DropSide } from "../lib/layout";

const DROP_LABELS: Record<DropSide, string> = {
  center: "Merge as tab",
  left: "Split left",
  right: "Split right",
  top: "Split up",
  bottom: "Split down",
};

/** Where a dragged pane will land, with what dropping it there does. */
export function DropMarker({ side, style }: { side: DropSide; style: CSSProperties }) {
  const Icon = side === "center" ? Layers : side === "left" || side === "right" ? Columns2 : Rows2;
  return (
    <div className={`dock-drop is-${side === "center" ? "merge" : "split"}`} style={style}>
      {/* Pops up again as the drop changes. */}
      <span key={side} className="dock-drop-badge">
        <Icon size={15} />
        {DROP_LABELS[side]}
      </span>
    </div>
  );
}
