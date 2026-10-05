import { create } from "zustand";
import {
  activateTab,
  addTab,
  allGroups,
  defaultLayout,
  dockTab,
  findGroup,
  group,
  groupOfTab,
  moveTab,
  parseLayout,
  persistableLayout,
  removeTab,
  setSizes,
  updateTab,
  welcomeTab,
  type DropSide,
  type LayoutNode,
  type PaneTab,
} from "../lib/layout";

const STORAGE_KEY = "poros.layout";
const SAVE_DELAY_MILLIS = 400;

interface LayoutState {
  root: LayoutNode;
  /** The group new tabs open in. */
  activeGroupId: string;
  setRoot: (root: LayoutNode) => void;
  addTab: (tab: PaneTab, groupId?: string, index?: number) => void;
  activateTab: (tabId: string) => void;
  replaceTab: (tabId: string, tab: PaneTab) => void;
  closeTab: (tabId: string) => void;
  moveTab: (tabId: string, groupId: string, index?: number) => void;
  dockTab: (tabId: string, groupId: string, side: DropSide) => void;
  setSizes: (splitId: string, sizes: number[]) => void;
  focusGroup: (groupId: string) => void;
}

function firstGroupId(root: LayoutNode): string {
  return allGroups(root)[0].id;
}

/** Keeps the active group valid after a change, and never leaves the window empty. */
function settle(root: LayoutNode | null, activeGroupId: string, preferTabId?: string) {
  const next = root ?? group([welcomeTab()]);
  const preferred = preferTabId ? groupOfTab(next, preferTabId)?.id : undefined;
  const active = preferred ?? (findGroup(next, activeGroupId) ? activeGroupId : firstGroupId(next));
  return { root: next, activeGroupId: active };
}

export const useLayoutStore = create<LayoutState>((set, get) => {
  const initial = defaultLayout();
  const update = (root: LayoutNode | null, preferTabId?: string) =>
    set(settle(root, get().activeGroupId, preferTabId));
  return {
    root: initial,
    activeGroupId: firstGroupId(initial),
    setRoot: (root) => set(settle(root, "")),
    addTab: (tab, groupId, index) => {
      const target = groupId ?? get().activeGroupId;
      update(addTab(get().root, target, tab, index), tab.id);
    },
    activateTab: (tabId) => update(activateTab(get().root, tabId), tabId),
    replaceTab: (tabId, tab) => update(updateTab(get().root, tabId, tab)),
    closeTab: (tabId) => update(removeTab(get().root, tabId)),
    moveTab: (tabId, groupId, index) => update(moveTab(get().root, tabId, groupId, index), tabId),
    dockTab: (tabId, groupId, side) => update(dockTab(get().root, tabId, groupId, side), tabId),
    setSizes: (splitId, sizes) => set({ root: setSizes(get().root, splitId, sizes) }),
    focusGroup: (groupId) => {
      if (get().activeGroupId !== groupId) set({ activeGroupId: groupId });
    },
  };
});

export function restoreLayout(): LayoutNode | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? parseLayout(JSON.parse(stored)) : null;
  } catch {
    return null;
  }
}

/** Saves the main window's layout as it changes; returns the unsubscribe function. */
export function persistLayout(enabled: () => boolean): () => void {
  let timer = 0;
  return useLayoutStore.subscribe((state, previous) => {
    if (state.root === previous.root) return;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      try {
        const layout = enabled() ? persistableLayout(state.root) : null;
        if (layout) localStorage.setItem(STORAGE_KEY, JSON.stringify(layout));
        else localStorage.removeItem(STORAGE_KEY);
      } catch {
        // Storage can be unavailable; the default layout then opens next time.
      }
    }, SAVE_DELAY_MILLIS);
  });
}
