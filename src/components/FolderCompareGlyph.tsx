import type { RowCompare } from "../lib/folderCompare";
import { COMPARE_CLASSES, COMPARE_GLYPHS } from "./folderCompareMarks";

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
