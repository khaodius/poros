// How a file list scrolls to the cursor's row when what it shows changes.

/** What changed in a file list since it last scrolled. */
export interface ListChange {
  /** Another folder is on show, or the first one is. */
  opened: boolean;
  /** The same folder was listed again, as after each file a transfer puts into it. */
  reloaded: boolean;
  /** The cursor is on another row, or on none. */
  cursorMoved: boolean;
  /** A date group was collapsed or expanded. */
  groupToggled: boolean;
}

/**
 * How the cursor's row is brought into view: in the middle of a folder just opened, as little as
 * it takes otherwise, or not at all. Collapsing or expanding a group, or listing the same folder
 * again with the cursor where it was, leaves the list where the user scrolled it.
 */
export function cursorScroll(change: ListChange): "center" | "auto" | null {
  if (change.opened) return "center";
  if (change.groupToggled) return null;
  if (change.reloaded && !change.cursorMoved) return null;
  return "auto";
}
