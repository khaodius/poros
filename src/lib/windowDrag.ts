import type { MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const DRAG_THRESHOLD = 3;
const CONTROLS = "button, input, select, textarea, a, label, [role='button']";

/**
 * Lets a frameless window's top bar move it and double-click to maximize. The system drag only
 * starts once the pointer moves, because on Linux it grabs the pointer and the second click of
 * a double-click would never arrive.
 */
export function dragWindowFrom(event: ReactMouseEvent<HTMLElement>): void {
  if (event.button !== 0 || (event.target as Element).closest(CONTROLS)) return;
  if (event.detail === 2) {
    void getCurrentWindow().toggleMaximize();
    return;
  }
  moveWindowFrom(event);
}

/**
 * Lets a part of a frameless window with nothing to press on it, such as the space beside its
 * tabs, move the window as its top bar does. A double click there keeps its own meaning.
 */
export function moveWindowFrom(event: ReactMouseEvent<HTMLElement>): void {
  if (event.button !== 0) return;
  const appWindow = getCurrentWindow();
  const { clientX: startX, clientY: startY } = event;
  const move = (moveEvent: MouseEvent) => {
    if (Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) < DRAG_THRESHOLD) {
      return;
    }
    stop();
    void appWindow.startDragging();
  };
  const stop = () => {
    window.removeEventListener("mousemove", move);
    window.removeEventListener("mouseup", stop);
  };
  window.addEventListener("mousemove", move);
  window.addEventListener("mouseup", stop);
}

/** Lets the part of a modal dialog's backdrop that covers the top bar move the window too. */
export function dragWindowFromBackdrop(event: ReactMouseEvent<HTMLDialogElement>): void {
  const dialog = event.currentTarget;
  if (event.target !== dialog) return;
  const { clientX: x, clientY: y } = event;
  const bounds = dialog.getBoundingClientRect();
  if (x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom) return;
  const titleBar = document.querySelector("[data-window-drag]");
  if (titleBar && y <= titleBar.getBoundingClientRect().bottom) dragWindowFrom(event);
}
