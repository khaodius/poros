// The searches run on each connection, newest first, for the name field's list of recent
// searches. Kept while the app runs.

import { create } from "zustand";
import { rememberSearch, type SearchForm } from "../lib/search";

interface SearchState {
  /** By the key of the file source searched: one connection, or this computer. */
  recent: Record<string, SearchForm[]>;
  remember: (sourceKey: string, form: SearchForm) => void;
}

export const useSearchStore = create<SearchState>((set) => ({
  recent: {},
  remember: (sourceKey, form) =>
    set((state) => ({
      recent: { ...state.recent, [sourceKey]: rememberSearch(state.recent[sourceKey] ?? [], form) },
    })),
}));
