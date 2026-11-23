// Where the keyboard goes between the panes on screen and their previews.

/** A file list Tab can move to: the pane it is in, and where it is on screen. */
export interface PaneStop<Pane> {
  pane: Pane;
  left: number;
  top: number;
}

/**
 * The pane Tab moves to from `from`: the next one on screen, left to right, then top to bottom,
 * and the first after the last. Only file lists are stops, never a preview, so Tab goes from
 * list to list whether previews are open or not. From a list it moves only when there is
 * another pane; from a preview, which `leavingPreview` says, it may come back to the pane's own
 * list. Null when it stays where it is.
 */
export function nextPane<Pane>(
  stops: readonly PaneStop<Pane>[],
  from: Pane | null,
  leavingPreview = false,
): Pane | null {
  if (stops.length === 0 || (stops.length < 2 && !leavingPreview)) return null;
  const ordered = [...stops].sort(
    (first, second) => first.left - second.left || first.top - second.top,
  );
  const current = ordered.findIndex((stop) => stop.pane === from);
  return ordered[(current + 1) % ordered.length].pane;
}

/** What a key press says about the keys held with it. */
interface PressedKey {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** Shown in the Preview button's tooltip. */
export const FOCUS_PREVIEW_SHORTCUT = "Ctrl+Shift+P";

/**
 * Ctrl+Shift+P, Cmd+Shift+P on a Mac: from a file list into its preview. Poros gives it no other
 * meaning. WebView2 on Windows binds it to printing, but the file list's handler takes the key
 * and prevents that default whenever it is pressed inside a pane. Tab keeps going from list to
 * list.
 */
export function isFocusPreviewKey(event: PressedKey): boolean {
  const primary = event.ctrlKey || event.metaKey;
  return primary && event.shiftKey && !event.altKey && event.key.toLowerCase() === "p";
}
