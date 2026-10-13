import { useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MAIN_WINDOW, WINDOW_USED_EVENT } from "../lib/ipc";

/**
 * Whether this is the Poros window the user turned to last, which asks what every window could:
 * the main window until another is used. A press or key in the window counts as well as its
 * focus, as WebView2 can leave a window the user is dragging in without the page's focus until
 * it is clicked again. Only another Poros window takes it over, so while the user is in another
 * program a question waits in the window they come back to.
 */
export function useLastUsedWindow(): boolean {
  const label = getCurrentWindow().label;
  const main = label === MAIN_WINDOW;
  const lastUsed = useRef(main);
  const [used, setUsed] = useState(main);

  useEffect(() => {
    const appWindow = getCurrentWindow();
    const take = () => {
      if (lastUsed.current) return;
      lastUsed.current = true;
      setUsed(true);
      void emit(WINDOW_USED_EVENT, label).catch(() => undefined);
    };
    // Every window hears the same announcements in the same order, its own among them, so all
    // agree on the last one.
    const listening = listen<string>(WINDOW_USED_EVENT, (event) => {
      lastUsed.current = event.payload === label;
      setUsed(lastUsed.current);
    });
    const focusing = appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) take();
    });
    if (document.hasFocus()) take();
    window.addEventListener("pointerdown", take, true);
    window.addEventListener("keydown", take, true);
    window.addEventListener("focus", take);
    return () => {
      void listening.then((unlisten) => unlisten());
      void focusing.then((unlisten) => unlisten());
      window.removeEventListener("pointerdown", take, true);
      window.removeEventListener("keydown", take, true);
      window.removeEventListener("focus", take);
    };
  }, [label]);

  return used;
}
