// Files dragged from a pane out of the window, onto the desktop or into a file manager. The
// system's own drag and drop carries them from the window's edge.

import { dragOut, onDragOutHover, toAppError } from "../lib/ipc";
import type { DragPointer } from "../lib/windowAreas";
import { hitTest, useDragStore, type DropHandler, type FilesDrag } from "./dragStore";
import { getPane } from "./paneRegistry";
import { useToastStore } from "./toastStore";
import { transferable } from "./transferActions";
import { useTransferStore } from "./transferStore";

/**
 * While a file manager waits for dragged files to download, Linux holds back the queue's progress
 * updates, so the queue is read this often instead.
 */
const PROGRESS_POLL_MILLIS = 500;

/** A point in this window's page. Files dragged out drop only on this window from here. */
function pagePointer({ x, y }: { x: number; y: number }): DragPointer {
  return { clientX: x, clientY: y, screen: { x: 0, y: 0 } };
}

/**
 * Hands a drag of files that reached the window's edge to the system. Files on a server download
 * as they go, ahead of the rest of the queue. Let go back over this window, they drop as if they
 * had never left it.
 */
export async function dragFilesOut(payload: FilesDrag, onDrop: DropHandler): Promise<void> {
  const source = getPane(payload.sourceTabId);
  const items = transferable(payload.entries);
  if (!source || items.length === 0) return;
  const sessionId = source.kind === "remote" ? source.sessionId : null;
  if (sessionId === undefined) return;
  useDragStore.setState({ outgoing: { payload, onDrop } });
  // Back over this window, the pane under the pointer lights up and the dragged files follow it,
  // as before they left. Windows tells the page nothing of a drag of files from a server, so
  // Poros says where the pointer is.
  const unlistenHovers = await onDragOutHover(({ at, copy }) => {
    if (!at) {
      useDragStore.setState({ payload: null, target: null, copy: false });
      return;
    }
    const target = hitTest(pagePointer(at), payload);
    useDragStore.setState({ payload, target, pointer: at, copy });
  }).catch(() => undefined);
  const polling =
    sessionId === null
      ? undefined
      : window.setInterval(() => void useTransferStore.getState().load(), PROGRESS_POLL_MILLIS);
  try {
    const outcome = await dragOut({ sessionId, items });
    if (outcome.kind === "returned" && outcome.at) {
      const target = hitTest(pagePointer(outcome.at), payload);
      // The files fly from where they were let go of.
      useDragStore.setState({ pointer: outcome.at });
      if (target) onDrop(target, payload, outcome.copy);
    }
  } catch (caught) {
    useToastStore.getState().show("error", toAppError(caught).message);
  } finally {
    void unlistenHovers?.();
    window.clearInterval(polling);
    useDragStore.setState({ outgoing: null, payload: null, target: null, copy: false });
  }
}
