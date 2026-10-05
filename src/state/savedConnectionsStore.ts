import { create } from "zustand";
import { savedConnections } from "../lib/ipc";
import type { SavedConnection } from "../lib/types";

interface SavedConnectionsState {
  connections: SavedConnection[];
  load: () => Promise<void>;
  save: (connection: SavedConnection, secret?: string | null) => Promise<SavedConnection>;
  remove: (id: string) => Promise<void>;
}

export const useSavedConnections = create<SavedConnectionsState>((set, get) => ({
  connections: [],
  load: async () => {
    set({ connections: await savedConnections.list() });
  },
  save: async (connection, secret) => {
    const saved = await savedConnections.save(connection, secret);
    await get().load();
    return saved;
  },
  remove: async (id) => {
    await savedConnections.remove(id);
    await get().load();
  },
}));

/** Saved connections, most recently used first, then by name. */
export function byRecentUse(connections: SavedConnection[]): SavedConnection[] {
  return [...connections].sort(
    (left, right) =>
      (right.lastUsed ?? 0) - (left.lastUsed ?? 0) || left.name.localeCompare(right.name),
  );
}
