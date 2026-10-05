import { create } from "zustand";
import { toAppError } from "../lib/ipc";
import type { AppError, ConnectProfile, HostKeyApproval, HostKeyInfo } from "../lib/types";
import { useConnectionStore } from "./connectionStore";

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

/** Connects, asking the user about unknown or changed host keys. Resolves to the failure, if any. */
export async function connectWithPrompts(
  profile: ConnectProfile,
  approval?: HostKeyApproval,
): Promise<AppError | null> {
  try {
    await useConnectionStore.getState().connect(profile, approval);
    return null;
  } catch (caught) {
    const error = toAppError(caught);
    const isHostKeyQuestion = error.kind === "hostKeyUnknown" || error.kind === "hostKeyChanged";
    if (!isHostKeyQuestion || !error.hostKey) return error;
    const decision = await useHostKeyPrompt
      .getState()
      .ask(error.hostKey, error.kind === "hostKeyChanged");
    if (!decision) return { kind: error.kind, message: "The host key was not accepted." };
    return connectWithPrompts(profile, decision);
  }
}
