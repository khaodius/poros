import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Whether this window is maximized, kept current as it is resized. */
export function useWindowMaximized(): boolean {
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    const appWindow = getCurrentWindow();
    let mounted = true;
    const refresh = () =>
      void appWindow
        .isMaximized()
        .then((value) => mounted && setMaximized(value))
        .catch(() => undefined);
    refresh();
    const subscription = appWindow.onResized(refresh);
    return () => {
      mounted = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, []);
  return maximized;
}
