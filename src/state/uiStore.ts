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
  | "archives"
  | "interface"
  | "appearance"
  | "log"
  | "updates";

export type AppDialog =
  | { kind: "connect"; draft: ConnectDraft; targetTabId?: string; error?: string }
  | { kind: "settings"; section: SettingsSection }
  | { kind: "saveConnection"; sessionId: string }
  /** `returnTo` reopens the dialog the import was started from once this one closes. */
  | { kind: "importConnections"; returnTo?: AppDialog }
  /** Folders to start with; the panes looked at last fill in what is not given. */
  | { kind: "sync"; localPath?: string; sessionId?: string; remotePath?: string }
  | {
      kind: "closeTab";
      tabId: string;
      sessionId: string;
      label: string;
      /** Jobs in the queue for the tab's server and for its unlocked vaults. */
      pendingTransfers: number;
      /** How many of those are for its unlocked vaults, which close with the tab. */
      vaultTransfers: number;
      /** Where the server and its vaults are mounted as drives, which go with the tab. */
      drives?: string[];
    }
  | {
      kind: "lockVault";
      sessionId: string;
      name: string;
      pendingTransfers: number;
      /** Where the vault is mounted as a drive, which goes when it locks. */
      drives?: string[];
    }
  /** Mounts a session's server or vault, starting from the folder `path` its pane shows. */
  | { kind: "mount"; sessionId: string; path: string | null }
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
