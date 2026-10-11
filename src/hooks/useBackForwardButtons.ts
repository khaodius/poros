import { useEffect, useLayoutEffect, useRef, type RefObject } from "react";

// The mouse's side buttons, and the Browser Back and Forward keys some mice send in their
// place, go back and forward in the pane under the pointer, as in a file manager.

const BACK_BUTTON = 3;
const FORWARD_BUTTON = 4;

/** Whether a press is a side button, which the web view would otherwise take as browser
 * navigation. */
export function isSideButton(event: MouseEvent): boolean {
  return event.button === BACK_BUTTON || event.button === FORWARD_BUTTON;
}

export function useBackForwardButtons(
  elementRef: RefObject<HTMLElement | null>,
  back: () => void,
  forward: () => void,
): void {
  const steps = useRef({ back, forward });
  useLayoutEffect(() => {
    steps.current = { back, forward };
  });

  useEffect(() => {
    const element = elementRef.current;
    if (!element) return;
    const handleMouseUp = (event: MouseEvent) => {
      if (!isSideButton(event)) return;
      event.preventDefault();
      // Dialogs and menus a pane opens sit inside it on the page.
      if ((event.target as Element).closest("dialog, .context-menu")) return;
      if (event.button === BACK_BUTTON) steps.current.back();
      else steps.current.forward();
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "BrowserBack" && event.key !== "BrowserForward") return;
      if (!element.matches(":hover") || document.querySelector("dialog[open]")) return;
      event.preventDefault();
      if (event.key === "BrowserBack") steps.current.back();
      else steps.current.forward();
    };
    element.addEventListener("mouseup", handleMouseUp);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      element.removeEventListener("mouseup", handleMouseUp);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [elementRef]);
}
