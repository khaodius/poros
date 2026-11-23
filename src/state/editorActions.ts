import { getCurrentWindow } from "@tauri-apps/api/window";
import { editor, toAppError } from "../lib/ipc";
import { allTabs, editorTab, findTab, type EditorTab, type PaneTab } from "../lib/layout";
import type { DocumentLocation } from "../lib/types";
import { untilAnswered } from "./answers";
import { getEditor, useUnsavedStore } from "./editorRegistry";
import { useLayoutStore } from "./layoutStore";
import { disconnectUnused, useSessionStore } from "./sessionStore";
import { useToastStore } from "./toastStore";
import { useUiStore } from "./uiStore";
import { stopSavingWorkspace } from "./workspace";

function sameFile(tab: PaneTab, location: DocumentLocation): tab is EditorTab {
  if (tab.kind !== "editor" || tab.path !== location.path) return false;
  return location.side === "remote" ? tab.sessionId === location.sessionId : !tab.sessionId;
}

/**
 * Opens a file in the editor beside the pane it was opened from, or shows the tab already
 * editing it.
 */
export async function openInEditor(
  location: DocumentLocation,
  nearGroupId?: string,
): Promise<void> {
  const open = allTabs(useLayoutStore.getState().root).find((tab) => sameFile(tab, location));
  if (open) {
    useLayoutStore.getState().activateTab(open.id);
    return;
  }
  try {
    const document = await editor.open(location);
    const sessionId = location.side === "remote" ? location.sessionId : undefined;
    useLayoutStore.getState().openTab(editorTab(document, sessionId), nearGroupId);
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
  if (tab.sessionId) disconnectUnused(tab.sessionId);
}

/** Editor tabs in this window with unsaved changes. */
export function unsavedEditorTabs(): EditorTab[] {
  const unsaved = useUnsavedStore.getState().tabs;
  return allTabs(useLayoutStore.getState().root).filter(
    (tab): tab is EditorTab => tab.kind === "editor" && Boolean(unsaved[tab.id]),
  );
}

/**
 * Editor tabs with unsaved changes to files that only these sessions can save: those in a vault,
 * and those on a server other than over SFTP, where the editor connects again on its own.
 */
export function unsavedOnlyThrough(sessionIds: readonly string[]): EditorTab[] {
  const { sessions } = useSessionStore.getState();
  return unsavedEditorTabs().filter(
    (tab) =>
      tab.sessionId !== undefined &&
      sessionIds.includes(tab.sessionId) &&
      sessions[tab.sessionId]?.info.protocol !== "sftp",
  );
}

/** Asks whether to save before tabs close, or before the window does. Tabs asked about while
 * the question is open join it. Resolves once no question about these tabs is open. */
export function askToSave(tabIds: string[], closeWindow = false): Promise<void> {
  const ui = useUiStore.getState();
  const asking = ui.dialog?.kind === "unsavedChanges" ? ui.dialog : null;
  const joined = [...new Set([...(asking?.tabIds ?? []), ...tabIds])];
  ui.open({
    kind: "unsavedChanges",
    tabIds: joined,
    closeWindow: closeWindow || !!asking?.closeWindow,
  });
  return untilAnswered(
    (dialog) =>
      dialog?.kind === "unsavedChanges" && tabIds.every((tabId) => dialog.tabIds.includes(tabId)),
  );
}

/** Asks about the tabs' unsaved changes; resolves true once each was saved or discarded. */
export async function savedOrDiscarded(tabs: readonly EditorTab[]): Promise<boolean> {
  await askToSave(tabs.map((tab) => tab.id));
  const unsaved = new Set(unsavedEditorTabs().map((tab) => tab.id));
  return !tabs.some((tab) => unsaved.has(tab.id));
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

/** Closes the window once its unsaved files are dealt with, keeping the tabs saved as it was
 * asked to close rather than those left after saving closed some. */
export function closeWindowNow(): void {
  stopSavingWorkspace();
  void getCurrentWindow().destroy();
}
