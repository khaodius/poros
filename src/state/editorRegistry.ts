// The editors open in this window, so closing a tab or the window can ask about unsaved changes
// and an editor tab moving to another window can take its unsaved text along.

import { create } from "zustand";
import type { EditorDraft } from "../lib/layout";

export interface EditorHandle {
  /** Resolves false when the file was not saved: it failed or the file changed meanwhile. */
  save: () => Promise<boolean>;
  /** The text with its unsaved changes; null when everything is saved. */
  draft: () => EditorDraft | null;
}

const editors = new Map<string, EditorHandle>();

export function registerEditor(tabId: string, handle: EditorHandle): () => void {
  editors.set(tabId, handle);
  return () => {
    if (editors.get(tabId) === handle) editors.delete(tabId);
    useUnsavedStore.getState().mark(tabId, false);
  };
}

export function getEditor(tabId: string): EditorHandle | undefined {
  return editors.get(tabId);
}

interface UnsavedState {
  /** Editor tabs with unsaved changes. */
  tabs: Record<string, true>;
  mark: (tabId: string, unsaved: boolean) => void;
}

export const useUnsavedStore = create<UnsavedState>((set, get) => ({
  tabs: {},
  mark: (tabId, unsaved) => {
    if (Boolean(get().tabs[tabId]) === unsaved) return;
    set((state) => {
      const tabs = { ...state.tabs };
      if (unsaved) tabs[tabId] = true;
      else delete tabs[tabId];
      return { tabs };
    });
  },
}));
