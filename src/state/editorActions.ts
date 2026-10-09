import { getCurrentWindow } from "@tauri-apps/api/window";
import { editor, toAppError } from "../lib/ipc";
import { allTabs, editorTab, findTab, type EditorTab, type PaneTab } from "../lib/layout";
import type { DocumentLocation } from "../lib/types";
import { getEditor, useUnsavedStore } from "./editorRegistry";
import { useLayoutStore } from "./layoutStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";

function sameFile(tab: PaneTab, location: DocumentLocation): tab is EditorTab {
  if (tab.kind !== "editor" || tab.path !== location.path) return false;
  return location.side === "remote" ? tab.sessionId === location.sessionId : !tab.sessionId;
}

/** Opens a file in an editor tab in the given group, or shows the tab already editing it. */
export async function openInEditor(location: DocumentLocation, groupId?: string): Promise<void> {
  const open = allTabs(useLayoutStore.getState().root).find((tab) => sameFile(tab, location));
  if (open) {
    useLayoutStore.getState().activateTab(open.id);
    return;
  }
  try {
    const document = await editor.open(location);
    const sessionId = location.side === "remote" ? location.sessionId : undefined;
    useLayoutStore.getState().addTab(editorTab(document, sessionId), groupId);
  } catch (caught) {
    useToastStore.getState().show("error", toAppError(caught).message);
  }
}

/** Closes an editor tab and forgets its file, saved or not. */
export function closeEditorTab(tabId: string): void {
  const tab = findTab(useLayoutStore.getState().root, tabId);
  if (tab?.kind !== "editor") return;
  useLayoutStore.getState().closeTab(tabId);
  void editor.close(tab.documentId).catch(() => undefined);
}

/** Editor tabs in this window with unsaved changes. */
export function unsavedEditorTabs(): EditorTab[] {
  const unsaved = useUnsavedStore.getState().tabs;
  return allTabs(useLayoutStore.getState().root).filter(
    (tab): tab is EditorTab => tab.kind === "editor" && Boolean(unsaved[tab.id]),
  );
}

/** Asks whether to save before tabs close, or before the window does. Tabs asked about while
 * the question is open join it. */
export function askToSave(tabIds: string[], closeWindow = false): void {
  const ui = useUiStore.getState();
  const asking = ui.dialog?.kind === "unsavedChanges" ? ui.dialog : null;
  const joined = [...new Set([...(asking?.tabIds ?? []), ...tabIds])];
  ui.open({
    kind: "unsavedChanges",
    tabIds: joined,
    closeWindow: closeWindow || !!asking?.closeWindow,
  });
}

/** Saves the tabs and closes those that saved; resolves whether all of them did. */
export async function saveAndClose(tabIds: string[]): Promise<boolean> {
  const results = await Promise.all(
    tabIds.map(async (tabId) => {
      const saved = (await getEditor(tabId)?.save()) ?? true;
      if (saved) closeEditorTab(tabId);
      return saved;
    }),
  );
  return results.every(Boolean);
}

export function closeWindowNow(): void {
  void getCurrentWindow().destroy();
}
