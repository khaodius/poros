import { create } from "zustand";
import type { ConnectDraft } from "../lib/connectDraft";

export type SettingsSection =
  "transfers" | "sync" | "connection" | "cloud" | "interface" | "appearance" | "log" | "updates";

export type AppDialog =
  | { kind: "connect"; draft: ConnectDraft; targetTabId?: string; error?: string }
  | { kind: "settings"; section: SettingsSection }
  | { kind: "saveConnection"; sessionId: string }
  /** Folders to start with; the panes looked at last fill in what is not given. */
  | { kind: "sync"; localPath?: string; sessionId?: string; remotePath?: string }
  | { kind: "closeTab"; tabId: string; sessionId: string; label: string; pendingTransfers: number }
  /** `returnTo` reopens the dialog the update was found from once this one closes. */
  | { kind: "update"; returnTo?: AppDialog };

interface UiState {
  dialog: AppDialog | null;
  open: (dialog: AppDialog) => void;
  close: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
  close: () => set({ dialog: null }),
}));
