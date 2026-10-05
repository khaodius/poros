import { create } from "zustand";
import { forgetRemoteSource } from "../lib/fileSource";
import { remote } from "../lib/ipc";
import type { ConnectProfile, HostKeyApproval, SessionInfo } from "../lib/types";
import { useSettingsStore } from "./settingsStore";

export interface SessionEntry {
  info: SessionInfo;
  /** What the session was opened with, minus secrets once handed to another window. */
  profile: ConnectProfile | null;
  status: "connected" | "lost";
  lostReason?: string;
}

interface SessionState {
  sessions: Record<string, SessionEntry>;
  connect: (profile: ConnectProfile, approval?: HostKeyApproval) => Promise<SessionInfo>;
  /** Opens a new session for a lost or live one; the old one is closed. */
  reconnect: (sessionId: string, approval?: HostKeyApproval) => Promise<SessionInfo>;
  /** Takes over a session another window opened. */
  adopt: (sessionId: string, profile: ConnectProfile | null) => Promise<SessionInfo>;
  disconnect: (sessionId: string) => Promise<void>;
  /** Drops a session from this window without closing it, when another window takes it. */
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
  };
}

export const useSessionStore = create<SessionState>((set, get) => ({
  sessions: {},

  connect: async (profile, approval) => {
    const info = await remote.connect(withConnectionSettings(profile), approval);
    set((state) => ({
      sessions: { ...state.sessions, [info.id]: { info, profile, status: "connected" } },
    }));
    return info;
  },

  reconnect: async (sessionId, approval) => {
    const previous = get().sessions[sessionId];
    const info = await remote.reconnect(sessionId, approval);
    await remote.disconnect(sessionId).catch(() => undefined);
    forgetRemoteSource(sessionId);
    set((state) => {
      const sessions = { ...state.sessions };
      delete sessions[sessionId];
      sessions[info.id] = { info, profile: previous?.profile ?? null, status: "connected" };
      return { sessions };
    });
    return info;
  },

  adopt: async (sessionId, profile) => {
    const info = await remote.adopt(sessionId);
    set((state) => ({
      sessions: { ...state.sessions, [info.id]: { info, profile, status: "connected" } },
    }));
    return info;
  },

  disconnect: async (sessionId) => {
    get().release(sessionId);
    await remote.disconnect(sessionId).catch(() => undefined);
  },

  release: (sessionId) => {
    forgetRemoteSource(sessionId);
    set((state) => {
      const sessions = { ...state.sessions };
      delete sessions[sessionId];
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
