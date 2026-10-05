import type { AuthMethod, AuthType, ConnectProfile, SavedConnection } from "./types";

export type AuthChoice = AuthType;

export interface ConnectDraft {
  /** The saved connection being edited, if any. */
  savedId: string | null;
  name: string;
  host: string;
  port: string;
  username: string;
  authChoice: AuthChoice;
  password: string;
  keyPath: string;
  passphrase: string;
  initialPath: string;
  saveConnection: boolean;
  /** Keeps the password or passphrase in the system keychain. */
  saveSecret: boolean;
  /** The keychain already holds a secret for the saved connection. */
  hasSavedSecret: boolean;
}

export const EMPTY_DRAFT: ConnectDraft = {
  savedId: null,
  name: "",
  host: "",
  port: "22",
  username: "",
  authChoice: "password",
  password: "",
  keyPath: "",
  passphrase: "",
  initialPath: "",
  saveConnection: true,
  saveSecret: false,
  hasSavedSecret: false,
};

export function profileFromDraft(draft: ConnectDraft): ConnectProfile {
  const auth: AuthMethod =
    draft.authChoice === "password"
      ? { type: "password", password: draft.password }
      : draft.authChoice === "publicKey"
        ? { type: "publicKey", keyPath: draft.keyPath.trim(), passphrase: draft.passphrase || null }
        : { type: "agent" };
  const profile: ConnectProfile = {
    host: draft.host.trim(),
    port: Number(draft.port) || 22,
    username: draft.username.trim(),
    auth,
    initialPath: draft.initialPath.trim() || null,
  };
  if (draft.savedId) profile.savedConnectionId = draft.savedId;
  return profile;
}

export function draftFromSaved(connection: SavedConnection): ConnectDraft {
  return {
    ...EMPTY_DRAFT,
    savedId: connection.id,
    name: connection.name,
    host: connection.host,
    port: String(connection.port),
    username: connection.username,
    authChoice: connection.authType,
    keyPath: connection.keyPath ?? "",
    initialPath: connection.remotePath ?? "",
    saveSecret: connection.saveSecret,
    hasSavedSecret: connection.saveSecret,
  };
}

/** The connection to store for a draft; secrets travel separately. */
export function savedFromDraft(draft: ConnectDraft): SavedConnection {
  const host = draft.host.trim();
  return {
    id: draft.savedId ?? "",
    name: draft.name.trim() || host,
    host,
    port: Number(draft.port) || 22,
    username: draft.username.trim(),
    authType: draft.authChoice,
    keyPath: draft.authChoice === "publicKey" ? draft.keyPath.trim() : null,
    remotePath: draft.initialPath.trim() || null,
    saveSecret: draft.authChoice !== "agent" && draft.saveSecret,
  };
}

/** The password or passphrase typed into a draft, if any. */
export function draftSecret(draft: ConnectDraft): string {
  if (draft.authChoice === "password") return draft.password;
  if (draft.authChoice === "publicKey") return draft.passphrase;
  return "";
}

export function savedFromProfile(profile: ConnectProfile, name: string, saveSecret: boolean) {
  const connection: SavedConnection = {
    id: "",
    name: name.trim() || profile.host,
    host: profile.host,
    port: profile.port,
    username: profile.username,
    authType: profile.auth.type,
    keyPath: profile.auth.type === "publicKey" ? profile.auth.keyPath : null,
    remotePath: profile.initialPath ?? null,
    saveSecret: profile.auth.type !== "agent" && saveSecret,
  };
  return connection;
}

export function profileSecret(profile: ConnectProfile): string {
  if (profile.auth.type === "password") return profile.auth.password;
  if (profile.auth.type === "publicKey") return profile.auth.passphrase ?? "";
  return "";
}

/** A saved connection to the same account on the same server. */
export function findSameAccount(
  connections: SavedConnection[],
  profile: Pick<ConnectProfile, "host" | "port" | "username">,
): SavedConnection | undefined {
  const host = profile.host.toLowerCase();
  return connections.find(
    (connection) =>
      connection.host.toLowerCase() === host &&
      connection.port === profile.port &&
      connection.username === profile.username,
  );
}
