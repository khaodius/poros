// Keyboard focus around dialogs and menus.

/** The tab group the user works in. */
const ACTIVE_GROUP = ".tab-group.is-focused";
/** The file list of the pane the user works in. */
const ACTIVE_FILE_LIST = ".tab-panel:not([hidden]) .pane.is-active .file-list-body";

export interface FocusReturn<Target> {
  /** What has focus as the dialog or menu closes. */
  active: Target | null;
  /** What had focus before it opened. */
  previous: Target | null;
  /** The page body, where focus falls when the focused element leaves the page. */
  body: Target | null;
  /** Whether the element is in the closing dialog or menu. */
  inside: (element: Target) => boolean;
  /** The user went to another tab group meanwhile, so focus must not pull them back. */
  moved: boolean;
  /** Takes focus when what had it before cannot. */
  fallback: Target | null;
}

/**
 * Where focus may go as a dialog or menu closes, best first: back to what had it before, else
 * the fallback. None when something outside the dialog or menu took focus on purpose.
 */
export function focusReturnCandidates<Target extends { readonly isConnected: boolean }>({
  active,
  previous,
  body,
  inside,
  moved,
  fallback,
}: FocusReturn<Target>): Target[] {
  if (active !== null && active !== body && !inside(active)) return [];
  const candidates: Target[] = [];
  if (
    !moved &&
    previous !== null &&
    previous !== body &&
    previous.isConnected &&
    !inside(previous)
  ) {
    candidates.push(previous);
  }
  if (fallback !== null && fallback !== previous) candidates.push(fallback);
  return candidates;
}

function focusedElement(): HTMLElement | null {
  return document.activeElement instanceof HTMLElement ? document.activeElement : null;
}

/**
 * Notes what has focus as a dialog or menu opens, and returns what to call as it closes, with
 * its element, to give focus back. Otherwise focus falls to the page body when the element
 * leaves the page, and keys stop reaching the file list.
 */
export function rememberFocus(): (closing: Element | null) => void {
  const previous = focusedElement();
  const group = document.querySelector(ACTIVE_GROUP);
  return (closing) => {
    const candidates = focusReturnCandidates<HTMLElement>({
      active: focusedElement(),
      previous,
      body: document.body,
      inside: (element) => closing?.contains(element) ?? false,
      moved: document.querySelector(ACTIVE_GROUP) !== group,
      fallback: document.querySelector<HTMLElement>(ACTIVE_FILE_LIST),
    });
    // A hidden or inert element does not take focus, so the next one gets it.
    for (const candidate of candidates) {
      candidate.focus({ preventScroll: true });
      if (document.activeElement === candidate) return;
    }
  };
}
