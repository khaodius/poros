import { create } from "zustand";
import { remote } from "../lib/ipc";
import type { ConnectProfile, HostKeyApproval, SessionInfo } from "../lib/types";

export type ConnectionStatus = "disconnected" | "connecting" | "connected" | "lost";

interface ConnectionState {
  status: ConnectionStatus;
  session: SessionInfo | null;
  /** Kept for reconnecting after the connection drops. */
  lastProfile: ConnectProfile | null;
  lostReason: string | null;
  connect: (profile: ConnectProfile, approval?: HostKeyApproval) => Promise<SessionInfo>;
  cancelConnect: () => void;
  disconnect: () => Promise<void>;
  markLost: (sessionId: string, reason: string) => void;
}

let currentAttempt = 0;

export const useConnectionStore = create<ConnectionState>((set, get) => ({
  status: "disconnected",
  session: null,
  lastProfile: null,
  lostReason: null,

  connect: async (profile, approval) => {
    const attempt = ++currentAttempt;
    const previous = get().session;
    if (previous) {
      await remote.disconnect(previous.id).catch(() => undefined);
    }
    set({ status: "connecting", session: null, lostReason: null });
    try {
      const session = await remote.connect(profile, approval);
      if (attempt !== currentAttempt) {
        await remote.disconnect(session.id).catch(() => undefined);
        throw { kind: "connection", message: "Connection attempt was cancelled" };
      }
      set({ status: "connected", session, lastProfile: profile });
      return session;
    } catch (error) {
      if (attempt === currentAttempt) set({ status: "disconnected" });
      throw error;
    }
  },

  cancelConnect: () => {
    currentAttempt++;
    if (get().status === "connecting") set({ status: "disconnected" });
  },

  disconnect: async () => {
    currentAttempt++;
    const session = get().session;
    set({ status: "disconnected", session: null, lostReason: null });
    if (session) await remote.disconnect(session.id).catch(() => undefined);
  },

  markLost: (sessionId, reason) => {
    if (get().session?.id !== sessionId) return;
    set({ status: "lost", lostReason: reason });
  },
}));
