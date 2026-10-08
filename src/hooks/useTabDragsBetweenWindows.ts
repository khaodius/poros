import { useEffect } from "react";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TAB_DRAG_EVENT } from "../lib/ipc";
import { findTab } from "../lib/layout";
import { incomingTarget, useDragStore } from "../state/dragStore";
import { useLayoutStore } from "../state/layoutStore";
import { receiveTabs, type TabDragMessage } from "../state/tabActions";

/**
 * Tells the window under a tab dragged out of this one where the pointer is, once a frame, and
 * marks where a tab dragged over this window from another would land.
 */
export function useTabDragsBetweenWindows(): void {
  useEffect(() => {
    let hovered: string | null = null;
    let latest: TabDragMessage | null = null;
    let frame = 0;
    const send = (label: string, message: TabDragMessage) =>
      void emitTo(label, TAB_DRAG_EVENT, message).catch(() => undefined);

    const unsubscribe = useDragStore.subscribe(({ payload, target }) => {
      const over = payload?.kind === "tab" && target?.kind === "window" ? target : null;
      if (hovered && hovered !== over?.label) {
        send(hovered, { phase: "leave" });
        hovered = null;
      }
      if (!over || payload?.kind !== "tab") return;
      hovered = over.label;
      const kind = findTab(useLayoutStore.getState().root, payload.tabId)?.kind ?? "welcome";
      latest = { phase: "over", x: over.x, y: over.y, label: payload.label, kind };
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (hovered && latest) send(hovered, latest);
      });
    });

    const listening = getCurrentWindow().listen<TabDragMessage>(TAB_DRAG_EVENT, (event) => {
      const message = event.payload;
      if (message.phase === "leave") {
        useDragStore.setState({ target: null, incoming: null });
        return;
      }
      const target = incomingTarget(message.x, message.y);
      if (message.phase === "over") {
        useDragStore.setState({
          target,
          pointer: { x: message.x, y: message.y },
          incoming: { label: message.label, kind: message.kind },
        });
        return;
      }
      useDragStore.setState({ target: null, incoming: null });
      void receiveTabs(message.handoff, target);
    });

    return () => {
      unsubscribe();
      cancelAnimationFrame(frame);
      void listening.then((unlisten) => unlisten());
    };
  }, []);
}
