import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { rgbChannels } from "./theme";

let windowBackground = "";
let backgroundSet: Promise<void> = Promise.resolve();

/**
 * Paints the window behind the page in the page's background color, so the moment before the
 * first frame, and the edge uncovered while resizing, show the theme instead of white. Resolves
 * once the window has the color, also when an earlier call is still setting it.
 */
export function matchWindowBackground(): Promise<void> {
  const color = rgbChannels(getComputedStyle(document.body).backgroundColor);
  if (color && color.join() !== windowBackground) {
    windowBackground = color.join();
    backgroundSet = getCurrentWebviewWindow()
      .setBackgroundColor(color)
      .catch(() => undefined);
  }
  return backgroundSet;
}

/** Shows this window, which opens hidden so nothing appears before the first render. */
export async function revealWindow(): Promise<void> {
  await matchWindowBackground();
  await getCurrentWindow()
    .show()
    .catch(() => undefined);
}
