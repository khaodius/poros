// Files dragged from a pane out of the window, onto the desktop or into a file manager. The
// system's own drag and drop carries them from the window's edge.

import { dragOut, toAppError } from "../lib/ipc";
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
  const polling =
    sessionId === null
      ? undefined
      : window.setInterval(() => void useTransferStore.getState().load(), PROGRESS_POLL_MILLIS);
  try {
    const outcome = await dragOut({ sessionId, items });
    if (outcome.kind === "returned" && outcome.at) {
      const position = { clientX: outcome.at.x, clientY: outcome.at.y, screen: { x: 0, y: 0 } };
      const target = hitTest(position, payload);
      if (target) onDrop(target, payload, outcome.copy);
    }
  } catch (caught) {
    useToastStore.getState().show("error", toAppError(caught).message);
  } finally {
    window.clearInterval(polling);
    useDragStore.setState({ outgoing: null, payload: null, target: null });
  }
}
