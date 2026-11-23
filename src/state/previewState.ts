// Whether a file pane's preview is open. The tab keeps it, so it comes back with the workspace
// and moves with the tab to another window.

import { findTab, isFileTab } from "../lib/layout";
import { useLayoutStore } from "./layoutStore";

export function usePreviewOpen(tabId: string): boolean {
  return useLayoutStore((state) => {
    const tab = findTab(state.root, tabId);
    return tab !== null && isFileTab(tab) && tab.preview === true;
  });
}

export function togglePreview(tabId: string): void {
  const layout = useLayoutStore.getState();
  const tab = findTab(layout.root, tabId);
  if (!tab || !isFileTab(tab)) return;
  layout.replaceTab(tabId, { ...tab, preview: !tab.preview });
}
