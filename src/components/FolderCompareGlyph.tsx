import {
  CircleDashed,
  ClockArrowDown,
  ClockArrowUp,
  Equal,
  EqualNot,
  Plus,
  type LucideIcon,
} from "lucide-react";
import type { MarkedState, RowCompare } from "../lib/folderCompare";

/** One glyph per state, so a row's state never depends on its color alone. */
export const COMPARE_GLYPHS: Record<MarkedState, LucideIcon> = {
  newer: ClockArrowUp,
  older: ClockArrowDown,
  onlyHere: Plus,
  different: EqualNot,
  identical: Equal,
  both: CircleDashed,
};

/** The state's name in class names: `is-compare-newer` on a row, `is-newer` on a glyph. */
export const COMPARE_CLASSES: Record<MarkedState, string> = {
  newer: "newer",
  older: "older",
  onlyHere: "only",
  different: "different",
  identical: "identical",
  both: "both",
};

/** The glyph at the end of a compared row's name, with the state in words under the pointer. */
export function FolderCompareGlyph({ mark }: { mark: RowCompare }) {
  const Glyph = COMPARE_GLYPHS[mark.state];
  return (
    <span
      className={`folder-compare-glyph is-${COMPARE_CLASSES[mark.state]}`}
      role="img"
      aria-label={mark.title}
      title={mark.title}
    >
      <Glyph size={13} aria-hidden />
    </span>
  );
}
