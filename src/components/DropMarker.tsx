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

const DROP_ICONS: Record<DropSide, typeof Layers> = {
  center: Layers,
  left: Columns2,
  right: Columns2,
  top: Rows2,
  bottom: Rows2,
};

/**
 * Where a dragged pane will land, under the lifted pane. It says what dropping there does only
 * where no lifted pane is drawn over it; the pane's title bar says so otherwise.
 */
export function DropMarker({
  side,
  style,
  labelled = true,
}: {
  side: DropSide;
  style: CSSProperties;
  labelled?: boolean;
}) {
  const Icon = DROP_ICONS[side];
  return (
    <div className={`dock-drop is-${side === "center" ? "merge" : "split"}`} style={style}>
      {labelled && (
        // Pops up again as the drop changes.
        <span key={side} className="dock-drop-badge">
          <Icon size={15} />
          {DROP_LABELS[side]}
        </span>
      )}
    </div>
  );
}

/** What dropping a lifted pane does, in its title bar. */
export function DropHint({
  side,
  className,
  iconSize,
}: {
  side: DropSide;
  className: string;
  iconSize: number;
}) {
  const Icon = DROP_ICONS[side];
  return (
    // Pops up again as the drop changes.
    <span key={side} className={`${className} is-drop`}>
      <Icon size={iconSize} />
      {DROP_LABELS[side]}
    </span>
  );
}
