import type { AuthMethod, ConnectProfile } from "./types";

export type AuthChoice = AuthMethod["type"];

export interface ConnectDraft {
  host: string;
  port: string;
  username: string;
  authChoice: AuthChoice;
  password: string;
  keyPath: string;
  passphrase: string;
  initialPath: string;
}

export const EMPTY_DRAFT: ConnectDraft = {
  host: "",
  port: "22",
  username: "",
  authChoice: "password",
  password: "",
  keyPath: "",
  passphrase: "",
  initialPath: "",
};

export function profileFromDraft(draft: ConnectDraft): ConnectProfile {
  const auth: AuthMethod =
    draft.authChoice === "password"
      ? { type: "password", password: draft.password }
      : draft.authChoice === "publicKey"
        ? { type: "publicKey", keyPath: draft.keyPath.trim(), passphrase: draft.passphrase || null }
        : { type: "agent" };
  return {
    host: draft.host.trim(),
    port: Number(draft.port) || 22,
    username: draft.username.trim(),
    auth,
    initialPath: draft.initialPath.trim() || null,
  };
}
