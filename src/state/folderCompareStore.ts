// This window's part in Compare folders: the comparison state of its panes, joined to the other
// windows, the pane registry, the settings and the backend of the running app.

import { emit, emitTo } from "@tauri-apps/api/event";
import { getAllWindows, getCurrentWindow } from "@tauri-apps/api/window";
import { useStore } from "zustand";
import { DRAG_PREVIEW_WINDOW, FOLDER_COMPARE_EVENT, onSyncProgress, sync } from "../lib/ipc";
import { createFolderCompare, type FolderCompareState } from "./folderCompare";
import { listPanes } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";

/** How long other windows get to name the pane they would offer to compare with. */
const SEEK_MILLIS = 250;

export const WINDOW_LABEL = getCurrentWindow().label;

export const folderCompare = createFolderCompare({
  window: WINDOW_LABEL,
  send: (to, message) => {
    const sending = to
      ? emitTo(to, FOLDER_COMPARE_EVENT, message)
      : emit(FOLDER_COMPARE_EVENT, message);
    void sending.catch(() => undefined);
  },
  otherWindows: async () =>
    (await getAllWindows())
      .map((entry) => entry.label)
      .filter((label) => label !== WINDOW_LABEL && label !== DRAG_PREVIEW_WINDOW),
  panes: () =>
    listPanes().map((pane) => ({
      tabId: pane.tabId,
      label: pane.label,
      visible: pane.visible,
      activatedAt: pane.activatedAt,
      ready: pane.path() !== null && !pane.locked,
    })),
  options: () => useSettingsStore.getState().settings.interface.folderCompare,
  syncDefaults: () => useSettingsStore.getState().settings.sync.options,
  compareFolders: sync.compareFolders,
  cancel: sync.cancel,
  newId: () => crypto.randomUUID(),
  seekMillis: SEEK_MILLIS,
});

export function useFolderCompareState<T>(selector: (state: FolderCompareState) => T): T {
  return useStore(folderCompare.store, selector);
}

let connected = false;

/**
 * Starts listening to the other windows, to progress from the backend and to the settings.
 * Called by the first file pane; it lasts as long as the window.
 */
export function connectFolderCompare(): void {
  if (connected) return;
  connected = true;
  void getCurrentWindow()
    .listen<unknown>(FOLDER_COMPARE_EVENT, (event) => folderCompare.receive(event.payload))
    .catch(() => undefined);
  void onSyncProgress(folderCompare.progress).catch(() => undefined);
  useSettingsStore.subscribe(() => folderCompare.rulesChanged());
  // A window that closes says so on its way out; one that could not is noticed on coming back
  // to this one.
  window.addEventListener("pagehide", () => folderCompare.leaveAll());
  window.addEventListener("focus", () => void folderCompare.dropClosedWindows());
}
