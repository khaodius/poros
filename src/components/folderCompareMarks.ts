import {
  CircleDashed,
  ClockArrowDown,
  ClockArrowUp,
  Diff,
  Equal,
  EqualNot,
  Plus,
  type LucideIcon,
} from "lucide-react";
import type { MarkedState } from "../lib/folderCompare";

/** One glyph per state, so a row's state never depends on its color alone. */
export const COMPARE_GLYPHS: Record<MarkedState, LucideIcon> = {
  newer: ClockArrowUp,
  older: ClockArrowDown,
  onlyHere: Plus,
  different: EqualNot,
  identical: Equal,
  both: CircleDashed,
  differsInside: Diff,
};

/**
 * The state's name in class names: `is-compare-newer` on a row, `is-newer` on a glyph. A folder
 * that differs inside takes Different's color with a glyph of its own.
 */
export const COMPARE_CLASSES: Record<MarkedState, string> = {
  newer: "newer",
  older: "older",
  onlyHere: "only",
  different: "different",
  identical: "identical",
  both: "both",
  differsInside: "differs-inside",
};
