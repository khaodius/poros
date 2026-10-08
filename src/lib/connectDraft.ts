import { defaultPort, isCloud, isFtp, protocolInfo } from "./protocols";
import type {
  AuthMethod,
  AuthType,
  ConnectProfile,
  Protocol,
  SavedConnection,
  SignedIn,
} from "./types";

export type AuthChoice = AuthType;

export interface ConnectDraft {
  /** The saved connection being edited, if any. */
  savedId: string | null;
  protocol: Protocol;
  name: string;
  host: string;
  port: string;
  /** For cloud storage, the account's email address or name. */
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
  /** FTP data connections come from the server. */
  ftpActive: boolean;
  /** A cloud account just signed in to, not yet stored with a connection. */
  signedIn: SignedIn | null;
}

export const EMPTY_DRAFT: ConnectDraft = {
  savedId: null,
  protocol: "sftp",
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
  ftpActive: false,
  signedIn: null,
};

/** The ways a protocol can sign in, the first being its default. */
export function authChoicesFor(protocol: Protocol): AuthChoice[] {
  if (isCloud(protocol)) return ["oauth"];
  if (isFtp(protocol)) return ["password"];
  return ["password", "publicKey", "agent"];
}

/**
 * Switches a draft to another protocol, moving the port along when it was the old protocol's
 * default and keeping the sign-in method when the new protocol supports it.
 */
export function withProtocol(draft: ConnectDraft, protocol: Protocol): ConnectDraft {
  const port =
    draft.port === "" || Number(draft.port) === defaultPort(draft.protocol)
      ? String(defaultPort(protocol))
      : draft.port;
  const choices = authChoicesFor(protocol);
  const authChoice = choices.includes(draft.authChoice) ? draft.authChoice : choices[0];
  const changedService = isCloud(protocol) || isCloud(draft.protocol);
  return {
    ...draft,
    protocol,
    port,
    authChoice,
    host: isCloud(protocol) ? "" : draft.host,
    username: changedService ? "" : draft.username,
    // A stored password is no use as a key passphrase, nor the other way around.
    hasSavedSecret:
      changedService || authChoice !== draft.authChoice ? false : draft.hasSavedSecret,
    signedIn: null,
  };
}

/** Whether a draft has everything a connection attempt needs. */
export function canConnect(draft: ConnectDraft): boolean {
  if (isCloud(draft.protocol)) return draft.signedIn !== null || draft.hasSavedSecret;
  if (draft.host.trim() === "") return false;
  // FTP servers take `anonymous` when no username is given.
  if (!isFtp(draft.protocol) && draft.username.trim() === "") return false;
  return draft.authChoice !== "publicKey" || draft.keyPath.trim() !== "";
}

function authFromDraft(draft: ConnectDraft): AuthMethod {
  switch (draft.authChoice) {
    case "publicKey":
      return {
        type: "publicKey",
        keyPath: draft.keyPath.trim(),
        passphrase: draft.passphrase || null,
      };
    case "agent":
      return { type: "agent" };
    case "oauth":
      return { type: "oauth", grantId: draft.signedIn?.grantId ?? null };
    case "password":
      return { type: "password", password: draft.password };
  }
}

export function profileFromDraft(draft: ConnectDraft): ConnectProfile {
  const cloud = isCloud(draft.protocol);
  const profile: ConnectProfile = {
    protocol: draft.protocol,
    host: cloud ? "" : draft.host.trim(),
    port: cloud ? defaultPort(draft.protocol) : Number(draft.port) || defaultPort(draft.protocol),
    username: (draft.signedIn?.account ?? draft.username).trim(),
    auth: authFromDraft(draft),
    initialPath: draft.initialPath.trim() || null,
  };
  if (isFtp(draft.protocol)) profile.ftpActive = draft.ftpActive;
  if (draft.savedId) profile.savedConnectionId = draft.savedId;
  return profile;
}

export function draftFromSaved(connection: SavedConnection): ConnectDraft {
  return {
    ...EMPTY_DRAFT,
    savedId: connection.id,
    protocol: connection.protocol,
    name: connection.name,
    host: isCloud(connection.protocol) ? "" : connection.host,
    port: String(connection.port),
    username: connection.username,
    authChoice: connection.authType,
    keyPath: connection.keyPath ?? "",
    initialPath: connection.remotePath ?? "",
    saveSecret: connection.saveSecret,
    hasSavedSecret: connection.saveSecret,
    ftpActive: connection.ftpActive ?? false,
  };
}

/** A name for a connection that has none: the server, or the cloud account. */
function fallbackName(protocol: Protocol, host: string, username: string): string {
  return isCloud(protocol) ? `${protocolInfo(protocol).label} (${username})` : host;
}

/** The connection to store for a draft; secrets travel separately. */
export function savedFromDraft(draft: ConnectDraft): SavedConnection {
  const host = draft.host.trim();
  const username = (draft.signedIn?.account ?? draft.username).trim();
  const cloud = isCloud(draft.protocol);
  return {
    id: draft.savedId ?? "",
    protocol: draft.protocol,
    name: draft.name.trim() || fallbackName(draft.protocol, host, username),
    host,
    port: cloud ? defaultPort(draft.protocol) : Number(draft.port) || defaultPort(draft.protocol),
    username,
    authType: draft.authChoice,
    keyPath: draft.authChoice === "publicKey" ? draft.keyPath.trim() : null,
    remotePath: draft.initialPath.trim() || null,
    // A cloud connection cannot open without its stored sign-in.
    saveSecret: cloud || (draft.authChoice !== "agent" && draft.saveSecret),
    ftpActive: isFtp(draft.protocol) && draft.ftpActive,
  };
}

/** The password or passphrase typed into a draft, if any. */
export function draftSecret(draft: ConnectDraft): string {
  if (draft.authChoice === "password") return draft.password;
  if (draft.authChoice === "publicKey") return draft.passphrase;
  return "";
}

export function savedFromProfile(profile: ConnectProfile, name: string, saveSecret: boolean) {
  const cloud = isCloud(profile.protocol);
  const connection: SavedConnection = {
    id: "",
    protocol: profile.protocol,
    name: name.trim() || fallbackName(profile.protocol, profile.host, profile.username),
    host: profile.host,
    port: profile.port,
    username: profile.username,
    authType: profile.auth.type,
    keyPath: profile.auth.type === "publicKey" ? profile.auth.keyPath : null,
    remotePath: profile.initialPath ?? null,
    saveSecret: cloud || (profile.auth.type !== "agent" && saveSecret),
    ftpActive: profile.ftpActive ?? false,
  };
  return connection;
}

export function profileSecret(profile: ConnectProfile): string {
  if (profile.auth.type === "password") return profile.auth.password;
  if (profile.auth.type === "publicKey") return profile.auth.passphrase ?? "";
  return "";
}

/** The browser sign-in a profile carries, for saving it with the connection. */
export function profileGrant(profile: ConnectProfile): string | null {
  return profile.auth.type === "oauth" ? (profile.auth.grantId ?? null) : null;
}

/** A saved connection to the same account on the same server or cloud service. */
export function findSameAccount(
  connections: SavedConnection[],
  profile: Pick<ConnectProfile, "protocol" | "host" | "port" | "username">,
): SavedConnection | undefined {
  const host = profile.host.toLowerCase();
  const cloud = isCloud(profile.protocol);
  return connections.find(
    (connection) =>
      connection.protocol === profile.protocol &&
      connection.username === profile.username &&
      (cloud || (connection.host.toLowerCase() === host && connection.port === profile.port)),
  );
}

/** How a saved connection is described under its name. */
export function connectionDetail(connection: SavedConnection): string {
  const info = protocolInfo(connection.protocol);
  if (isCloud(connection.protocol)) return `${info.label}, ${connection.username}`;
  const address =
    connection.port === info.defaultPort
      ? connection.host
      : `${connection.host}:${connection.port}`;
  const account = connection.username || (isFtp(connection.protocol) ? "anonymous" : "");
  const target = account ? `${account}@${address}` : address;
  return connection.protocol === "sftp" ? target : `${info.shortLabel}, ${target}`;
}
