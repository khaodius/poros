import { create } from "zustand";
import type { CommandTarget } from "../lib/commands";
import type { ConnectDraft } from "../lib/connectDraft";
import type { ScheduledTask } from "../lib/types";

export type SettingsSection =
  | "transfers"
  | "sync"
  | "connection"
  | "cloud"
  | "automation"
  | "interface"
  | "appearance"
  | "log"
  | "updates";

export type AppDialog =
  | { kind: "connect"; draft: ConnectDraft; targetTabId?: string; error?: string }
  | { kind: "settings"; section: SettingsSection }
  | { kind: "saveConnection"; sessionId: string }
  /** Folders to start with; the panes looked at last fill in what is not given. */
  | { kind: "sync"; localPath?: string; sessionId?: string; remotePath?: string }
  | { kind: "closeTab"; tabId: string; sessionId: string; label: string; pendingTransfers: number }
  /** Editor tabs with unsaved changes that are about to close, or whose window is. */
  | { kind: "unsavedChanges"; tabIds: string[]; closeWindow: boolean }
  /** `returnTo` reopens the dialog the update was found from once this one closes. */
  | { kind: "update"; returnTo?: AppDialog }
  | { kind: "runCommand"; sessionId: string; target: CommandTarget }
  /** A task to start editing, such as one prepared from the synchronization dialog. */
  | { kind: "schedules"; draft?: ScheduledTask };

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
