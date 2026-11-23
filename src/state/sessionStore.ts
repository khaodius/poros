import { create } from "zustand";
import { forgetRemoteSource } from "../lib/fileSource";
import { remote, vaults } from "../lib/ipc";
import { allTabs, isRestoring, type SavedServer } from "../lib/layout";
import type { ConnectProfile, HostKeyApproval, SessionInfo, VaultInfo } from "../lib/types";
import { serverSessionId, vaultSessions } from "../lib/vaults";
import { useLayoutStore } from "./layoutStore";
import { getPane } from "./paneRegistry";
import { useSettingsStore } from "./settingsStore";

export interface SessionEntry {
  info: SessionInfo;
  /**
   * What the session was opened with, minus secrets once handed to another window. Null for an
   * unlocked vault, which its server's session opens.
   */
  profile: ConnectProfile | null;
  /** A vault's own status stays connected: it is lost and back with its server. */
  status: "connected" | "lost";
  lostReason?: string;
}

interface SessionState {
  sessions: Record<string, SessionEntry>;
  connect: (profile: ConnectProfile, approval?: HostKeyApproval) => Promise<SessionInfo>;
  /**
   * Opens a new session for a lost or live server; the old one is closed. The tabs showing the
   * old session show the new one, and the server's unlocked vaults stay open in their sessions.
   */
  reconnect: (sessionId: string, approval?: HostKeyApproval) => Promise<SessionInfo>;
  /** Takes over a session another window opened. */
  adopt: (sessionId: string, profile: ConnectProfile | null) => Promise<SessionInfo>;
  /** Unlocks the vault in the folder `path` on a session's server, in a session of its own. */
  unlockVault: (sessionId: string, path: string, password: string) => Promise<SessionInfo>;
  /** Makes the new folder `name` in `parent` on a session's server a vault, and unlocks it. */
  createVault: (
    sessionId: string,
    parent: string,
    name: string,
    password: string,
  ) => Promise<SessionInfo>;
  /** Closes a session. A server's unlocked vaults close with it; a vault closes alone. */
  disconnect: (sessionId: string) => Promise<void>;
  /**
   * Drops a session from this window without closing it, when another window takes it: a server
   * with its unlocked vaults.
   */
  release: (sessionId: string) => void;
  markLost: (sessionId: string, reason: string) => void;
  /** Records that the session's server is now a saved connection. */
  linkSaved: (sessionId: string, savedConnectionId: string) => void;
}

function withConnectionSettings(profile: ConnectProfile): ConnectProfile {
  const { connection } = useSettingsStore.getState().settings;
  return {
    ...profile,
    timeoutSecs: profile.timeoutSecs ?? connection.timeoutSecs,
    keepaliveSecs: profile.keepaliveSecs ?? connection.keepaliveSecs,
    compression: profile.compression ?? connection.compression,
    receiveBufferKib: connection.autoTuneReceiveBuffer ? null : connection.receiveBufferKib,
    sendBufferKib: connection.autoTuneSendBuffer ? null : connection.sendBufferKib,
  };
}

type Sessions = SessionState["sessions"];

function withSession(sessions: Sessions, info: SessionInfo, profile: ConnectProfile | null) {
  return { sessions: { ...sessions, [info.id]: { info, profile, status: "connected" as const } } };
}

/** The file tabs of this window that show one of `sessionIds`. */
function tabsShowing(sessionIds: readonly string[]) {
  return allTabs(useLayoutStore.getState().root).flatMap((tab) =>
    tab.kind === "remote" && !isRestoring(tab) && sessionIds.includes(tab.sessionId) ? [tab] : [],
  );
}

/** Shows the session that replaces `previousId` in the tabs that showed it, where they were. */
function moveTabs(previousId: string, sessionId: string): void {
  for (const tab of tabsShowing([previousId])) {
    const path = getPane(tab.id)?.path() ?? tab.path;
    useLayoutStore.getState().replaceTab(tab.id, { id: tab.id, kind: "remote", sessionId, path });
  }
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: {},

  connect: async (profile, approval) => {
    const info = await remote.connect(withConnectionSettings(profile), approval);
    set((state) => withSession(state.sessions, info, profile));
    return info;
  },

  reconnect: async (sessionId, approval) => {
    const previous = get().sessions[sessionId];
    const info = await remote.reconnect(sessionId, approval);
    await remote.disconnect(sessionId).catch(() => undefined);
    forgetRemoteSource(sessionId);
    // In one step with the sessions, so no tab is left showing a session that is gone.
    moveTabs(sessionId, info.id);
    const reattached = vaultSessions(get().sessions, sessionId);
    set((state) => {
      const sessions = { ...state.sessions };
      delete sessions[sessionId];
      sessions[info.id] = { info, profile: previous?.profile ?? null, status: "connected" };
      // The backend moved the server's unlocked vaults to the new session, keys and all.
      for (const vault of reattached) {
        const moved = { ...vault, vault: { ...vault.vault, parentSessionId: info.id } };
        sessions[vault.id] = { ...state.sessions[vault.id], info: moved };
      }
      return { sessions };
    });
    // Their sessions stay, so their panes would not reload on their own.
    for (const tab of tabsShowing(reattached.map((vault) => vault.id))) {
      getPane(tab.id)?.refresh();
    }
    return info;
  },

  adopt: async (sessionId, profile) => {
    const info = await remote.adopt(sessionId);
    set((state) => withSession(state.sessions, info, profile));
    return info;
  },

  unlockVault: async (sessionId, path, password) => {
    const info = await vaults.unlock(sessionId, path, password);
    set((state) => withSession(state.sessions, info, null));
    return info;
  },

  createVault: async (sessionId, parent, name, password) => {
    const info = await vaults.create(sessionId, parent, name, password);
    set((state) => withSession(state.sessions, info, null));
    return info;
  },

  disconnect: async (sessionId) => {
    get().release(sessionId);
    await remote.disconnect(sessionId).catch(() => undefined);
  },

  release: (sessionId) => {
    const vaultIds = vaultSessions(get().sessions, sessionId).map((vault) => vault.id);
    const leaving = [sessionId, ...vaultIds];
    leaving.forEach(forgetRemoteSource);
    set((state) => {
      const sessions = { ...state.sessions };
      for (const id of leaving) delete sessions[id];
      return { sessions };
    });
  },

  linkSaved: (sessionId, savedConnectionId) =>
    set((state) => {
      const entry = state.sessions[sessionId];
      if (!entry) return state;
      const info = { ...entry.info, savedConnectionId };
      return { sessions: { ...state.sessions, [sessionId]: { ...entry, info } } };
    }),

  markLost: (sessionId, reason) =>
    set((state) => {
      const entry = state.sessions[sessionId];
      if (!entry) return state;
      return {
        sessions: {
          ...state.sessions,
          [sessionId]: { ...entry, status: "lost", lostReason: reason },
        },
      };
    }),
}));

/**
 * The entry of the server a session's files are on: its own, or for an unlocked vault that of
 * the server the vault is on, whose connection the vault uses.
 */
export function serverEntry(sessions: Sessions, sessionId: string): SessionEntry | undefined {
  const entry = sessions[sessionId] as SessionEntry | undefined;
  return entry && sessions[serverSessionId(entry.info)];
}

/** A profile safe to hand to another window: the backend still holds the secrets. */
export function withoutSecrets(profile: ConnectProfile | null): ConnectProfile | null {
  if (!profile) return null;
  const auth =
    profile.auth.type === "password"
      ? { type: "password" as const, password: "" }
      : profile.auth.type === "publicKey"
        ? { type: "publicKey" as const, keyPath: profile.auth.keyPath, passphrase: null }
        : profile.auth;
  return { ...profile, auth };
}

/** A profile to keep on disk: without its password, passphrase or browser sign-in. */
export function withoutStoredSecrets(profile: ConnectProfile): ConnectProfile {
  const kept = withoutSecrets(profile)!;
  return kept.auth.type === "oauth" ? { ...kept, auth: { type: "oauth", grantId: null } } : kept;
}

/** The server a session in this window is connected to, to connect to again after a restart. */
export function savedServerOf(sessionId: string): SavedServer | null {
  const entry = useSessionStore.getState().sessions[sessionId];
  // A vault is no server to connect to: after a restart it is a locked folder on its server.
  if (!entry || entry.info.vault) return null;
  const { label, protocol } = entry.info;
  const profile = entry.profile && withoutStoredSecrets(entry.profile);
  const savedConnectionId = entry.info.savedConnectionId ?? profile?.savedConnectionId ?? undefined;
  return { label, protocol, savedConnectionId, profile };
}

/** The vault a session in this window has unlocked, or null for a server's own session. */
export function vaultOf(sessionId: string): VaultInfo | null {
  return useSessionStore.getState().sessions[sessionId]?.info.vault ?? null;
}
