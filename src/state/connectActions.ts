import {
  draftFromSaved,
  findSameAccount,
  profileFromDraft,
  savedFromProfile,
} from "../lib/connectDraft";
import type { AppError, ConnectProfile, SavedConnection } from "../lib/types";
import { connectWithPrompts, type ConnectResult } from "./connectFlow";
import { useSavedConnections } from "./savedConnectionsStore";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { showSession } from "./tabActions";
import { useUiStore } from "./uiStore";

export async function connectInTab(
  profile: ConnectProfile,
  targetTabId?: string,
): Promise<ConnectResult> {
  const result = await connectWithPrompts(profile);
  if ("session" in result) showSession(result.session, targetTabId);
  return result;
}

/** Connects from the quick connect bar, saving the server when the user asked for that. */
export async function quickConnect(profile: ConnectProfile): Promise<AppError | null> {
  const result = await connectInTab(profile);
  if ("error" in result) return result.error;
  if (useSettingsStore.getState().settings.interface.saveQuickConnections) {
    const saved = useSavedConnections.getState();
    if (!findSameAccount(saved.connections, profile)) {
      const connection = await saved
        .save(savedFromProfile(profile, result.session.label, false))
        .catch(() => null);
      if (connection) useSessionStore.getState().linkSaved(result.session.id, connection.id);
    }
  }
  return null;
}

const NEEDS_CREDENTIALS = new Set(["passphraseRequired", "authFailed"]);

/** Connects to a saved server. When a password is needed, opens the connect dialog. */
export async function connectSaved(
  connection: SavedConnection,
  targetTabId?: string,
): Promise<AppError | null> {
  const draft = draftFromSaved(connection);
  const openDialog = (error?: string) =>
    useUiStore.getState().open({ kind: "connect", draft, targetTabId, error });
  if (connection.authType === "password" && !connection.saveSecret) {
    openDialog();
    return null;
  }
  const result = await connectInTab(profileFromDraft(draft), targetTabId);
  if (!("error" in result)) return null;
  if (NEEDS_CREDENTIALS.has(result.error.kind)) {
    openDialog(result.error.message);
    return null;
  }
  return result.error;
}
