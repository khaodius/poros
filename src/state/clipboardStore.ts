// What was cut or copied in the panes. Kept in local storage, so every window pastes the same
// thing; the main window starts each run of the app with an empty clipboard.

import { create } from "zustand";
import type { FileOrigin } from "../lib/fileOrigin";
import type { FileEntry, PlaceMode } from "../lib/types";
import { isMainWindow } from "./tabActions";

export interface Clip {
  /** Cut is a move, copy a copy. */
  mode: PlaceMode;
  origin: FileOrigin;
  /** The folder the entries were in. */
  folder: string;
  entries: FileEntry[];
}

const STORAGE_KEY = "poros.clipboard";

function readStored(): Clip | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored ? (JSON.parse(stored) as Clip) : null;
  } catch {
    return null;
  }
}

function store(clip: Clip | null): void {
  try {
    if (clip) localStorage.setItem(STORAGE_KEY, JSON.stringify(clip));
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Without storage the clipboard still works in this window.
  }
}

interface ClipboardState {
  clip: Clip | null;
  put: (clip: Clip) => void;
  clear: () => void;
}

export const useClipboardStore = create<ClipboardState>((set) => ({
  clip: null,
  put: (clip) => {
    store(clip);
    set({ clip });
  },
  clear: () => {
    store(null);
    set({ clip: null });
  },
}));

/** Loads the clipboard and follows what other windows put on it. */
export function startClipboard(): () => void {
  if (isMainWindow()) useClipboardStore.getState().clear();
  else useClipboardStore.setState({ clip: readStored() });
  const follow = (event: StorageEvent) => {
    if (event.key === STORAGE_KEY) useClipboardStore.setState({ clip: readStored() });
  };
  window.addEventListener("storage", follow);
  return () => window.removeEventListener("storage", follow);
}
