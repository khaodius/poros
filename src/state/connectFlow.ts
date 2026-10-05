import { create } from "zustand";
import { toAppError } from "../lib/ipc";
import type {
  AppError,
  ConnectProfile,
  HostKeyApproval,
  HostKeyInfo,
  SessionInfo,
} from "../lib/types";
import { useSessionStore } from "./sessionStore";

export interface HostKeyQuestion {
  hostKey: HostKeyInfo;
  changed: boolean;
  answer: (approval: HostKeyApproval | null) => void;
}

interface HostKeyPromptState {
  question: HostKeyQuestion | null;
  ask: (hostKey: HostKeyInfo, changed: boolean) => Promise<HostKeyApproval | null>;
}

export const useHostKeyPrompt = create<HostKeyPromptState>((set) => ({
  question: null,
  ask: (hostKey, changed) =>
    new Promise((resolve) =>
      set({
        question: {
          hostKey,
          changed,
          answer: (approval) => {
            set({ question: null });
            resolve(approval);
          },
        },
      }),
    ),
}));

export type ConnectResult = { session: SessionInfo } | { error: AppError };

/** Runs a connection attempt, asking the user about unknown or changed host keys. */
async function withHostKeyPrompts(
  attempt: (approval?: HostKeyApproval) => Promise<SessionInfo>,
  approval?: HostKeyApproval,
): Promise<ConnectResult> {
  try {
    return { session: await attempt(approval) };
  } catch (caught) {
    const error = toAppError(caught);
    const isHostKeyQuestion = error.kind === "hostKeyUnknown" || error.kind === "hostKeyChanged";
    if (!isHostKeyQuestion || !error.hostKey) return { error };
    const decision = await useHostKeyPrompt
      .getState()
      .ask(error.hostKey, error.kind === "hostKeyChanged");
    if (!decision)
      return { error: { kind: error.kind, message: "The host key was not accepted." } };
    return withHostKeyPrompts(attempt, decision);
  }
}

export function connectWithPrompts(profile: ConnectProfile): Promise<ConnectResult> {
  return withHostKeyPrompts((approval) => useSessionStore.getState().connect(profile, approval));
}

export function reconnectWithPrompts(sessionId: string): Promise<ConnectResult> {
  return withHostKeyPrompts((approval) =>
    useSessionStore.getState().reconnect(sessionId, approval),
  );
}
