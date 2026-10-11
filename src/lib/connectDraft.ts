import {
  defaultPort,
  hasFixedHost,
  isFtp,
  isWebDav,
  protocolInfo,
  signsInWithBrowser,
  usesAccessKeys,
} from "./protocols";
import type {
  AuthMethod,
  AuthType,
  ConnectProfile,
  Protocol,
  S3Addressing,
  S3Options,
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
  /** The account's email address or name for browser sign-in, the key ID for S3 and B2. */
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
  /** A saved connection to go through first; empty connects straight to the server. */
  jumpConnectionId: string;
  bypassProxy: boolean;
  /** S3 only: empty takes the region from the endpoint. */
  s3Region: string;
  s3Addressing: S3Addressing;
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
  jumpConnectionId: "",
  bypassProxy: false,
  s3Region: "",
  s3Addressing: "automatic",
};

/** The ways a protocol can sign in, the first being its default. */
export function authChoicesFor(protocol: Protocol): AuthChoice[] {
  if (signsInWithBrowser(protocol)) return ["oauth"];
  if (protocol === "sftp") return ["password", "publicKey", "agent"];
  return ["password"];
}

/** What kind of account a protocol signs in to; a username means nothing to another kind. */
function accountKind(protocol: Protocol): string {
  if (signsInWithBrowser(protocol) || usesAccessKeys(protocol)) return protocol;
  return "server";
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
  const changedService = accountKind(protocol) !== accountKind(draft.protocol);
  // An S3 endpoint is not a server to log in to, nor the other way around.
  const changedHost = hasFixedHost(protocol) || (protocol === "s3") !== (draft.protocol === "s3");
  return {
    ...draft,
    protocol,
    port,
    authChoice,
    host: changedHost ? "" : draft.host,
    username: changedService ? "" : draft.username,
    // A stored password is no use as a key passphrase, nor the other way around, and the
    // keychain keeps it for one port.
    hasSavedSecret:
      changedService || authChoice !== draft.authChoice || port !== draft.port
        ? false
        : draft.hasSavedSecret,
    signedIn: null,
  };
}

/**
 * The keychain releases a saved secret only to the server, account and sign-in method it was
 * saved for.
 */
const SECRET_SCOPE: ReadonlySet<keyof ConnectDraft> = new Set<keyof ConnectDraft>([
  "host",
  "port",
  "username",
  "authChoice",
]);

/** Changes one field of a draft, forgetting a saved secret that would no longer be used. */
export function withField<K extends keyof ConnectDraft>(
  draft: ConnectDraft,
  key: K,
  value: ConnectDraft[K],
): ConnectDraft {
  const leavesScope = SECRET_SCOPE.has(key) && draft[key] !== value;
  return { ...draft, [key]: value, hasSavedSecret: draft.hasSavedSecret && !leavesScope };
}

/** Whether a draft has everything a connection attempt needs. */
export function canConnect(draft: ConnectDraft): boolean {
  if (signsInWithBrowser(draft.protocol)) return draft.signedIn !== null || draft.hasSavedSecret;
  // S3 without an endpoint is Amazon's.
  if (usesAccessKeys(draft.protocol)) return draft.username.trim() !== "";
  if (draft.host.trim() === "") return false;
  // FTP servers take `anonymous` when no username is given, and WebDAV may need no sign-in.
  if (!isFtp(draft.protocol) && !isWebDav(draft.protocol) && draft.username.trim() === "") {
    return false;
  }
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

function s3FromDraft(draft: ConnectDraft): S3Options {
  return { region: draft.s3Region.trim(), addressing: draft.s3Addressing };
}

function portFromDraft(draft: ConnectDraft): number {
  return hasFixedHost(draft.protocol)
    ? defaultPort(draft.protocol)
    : Number(draft.port) || defaultPort(draft.protocol);
}

export function profileFromDraft(draft: ConnectDraft): ConnectProfile {
  const profile: ConnectProfile = {
    protocol: draft.protocol,
    host: hasFixedHost(draft.protocol) ? "" : draft.host.trim(),
    port: portFromDraft(draft),
    username: (draft.signedIn?.account ?? draft.username).trim(),
    auth: authFromDraft(draft),
    initialPath: draft.initialPath.trim() || null,
  };
  if (isFtp(draft.protocol)) profile.ftpActive = draft.ftpActive;
  if (draft.protocol === "s3") profile.s3 = s3FromDraft(draft);
  if (draft.savedId) profile.savedConnectionId = draft.savedId;
  // The proxy and jump hosts carry SSH connections only.
  if (draft.protocol === "sftp") {
    if (draft.jumpConnectionId) profile.jumpConnectionId = draft.jumpConnectionId;
    if (draft.bypassProxy) profile.bypassProxy = true;
  }
  return profile;
}

export function draftFromSaved(connection: SavedConnection): ConnectDraft {
  return {
    ...EMPTY_DRAFT,
    savedId: connection.id,
    protocol: connection.protocol,
    name: connection.name,
    host: hasFixedHost(connection.protocol) ? "" : connection.host,
    port: String(connection.port),
    username: connection.username,
    authChoice: connection.authType,
    keyPath: connection.keyPath ?? "",
    initialPath: connection.remotePath ?? "",
    saveSecret: connection.saveSecret,
    hasSavedSecret: connection.saveSecret,
    ftpActive: connection.ftpActive ?? false,
    jumpConnectionId: connection.jumpConnectionId ?? "",
    bypassProxy: connection.bypassProxy ?? false,
    s3Region: connection.s3?.region ?? "",
    s3Addressing: connection.s3?.addressing ?? "automatic",
  };
}

/** A name for a connection that has none: the server, or the cloud account. */
function fallbackName(protocol: Protocol, host: string, username: string): string {
  if (hasFixedHost(protocol)) return `${protocolInfo(protocol).label} (${username})`;
  if (protocol === "s3") return host || "Amazon S3";
  return host;
}

/** The connection to store for a draft; secrets travel separately. */
export function savedFromDraft(draft: ConnectDraft): SavedConnection {
  const host = draft.host.trim();
  const username = (draft.signedIn?.account ?? draft.username).trim();
  const connection: SavedConnection = {
    id: draft.savedId ?? "",
    protocol: draft.protocol,
    name: draft.name.trim() || fallbackName(draft.protocol, host, username),
    host,
    port: portFromDraft(draft),
    username,
    authType: draft.authChoice,
    keyPath: draft.authChoice === "publicKey" ? draft.keyPath.trim() : null,
    remotePath: draft.initialPath.trim() || null,
    // A browser sign-in cannot be used again unless it is stored.
    saveSecret:
      signsInWithBrowser(draft.protocol) || (draft.authChoice !== "agent" && draft.saveSecret),
    ftpActive: isFtp(draft.protocol) && draft.ftpActive,
    bypassProxy: draft.protocol === "sftp" && draft.bypassProxy,
    jumpConnectionId: (draft.protocol === "sftp" && draft.jumpConnectionId) || null,
  };
  if (draft.protocol === "s3") connection.s3 = s3FromDraft(draft);
  return connection;
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
    protocol: profile.protocol,
    name: name.trim() || fallbackName(profile.protocol, profile.host, profile.username),
    host: profile.host,
    port: profile.port,
    username: profile.username,
    authType: profile.auth.type,
    keyPath: profile.auth.type === "publicKey" ? profile.auth.keyPath : null,
    remotePath: profile.initialPath ?? null,
    saveSecret:
      signsInWithBrowser(profile.protocol) || (profile.auth.type !== "agent" && saveSecret),
    ftpActive: profile.ftpActive ?? false,
    bypassProxy: profile.bypassProxy ?? false,
    jumpConnectionId: profile.jumpConnectionId ?? null,
  };
  if (profile.s3) connection.s3 = profile.s3;
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
  const fixedHost = hasFixedHost(profile.protocol);
  return connections.find(
    (connection) =>
      connection.protocol === profile.protocol &&
      connection.username === profile.username &&
      (fixedHost || (connection.host.toLowerCase() === host && connection.port === profile.port)),
  );
}

/** How a saved connection is described under its name. */
export function connectionDetail(connection: SavedConnection): string {
  const info = protocolInfo(connection.protocol);
  if (hasFixedHost(connection.protocol)) return `${info.label}, ${connection.username}`;
  const address =
    connection.port === info.defaultPort
      ? connection.host
      : `${connection.host}:${connection.port}`;
  // An access key ID says nothing to a reader.
  if (connection.protocol === "s3") return `${info.shortLabel}, ${address}`;
  const account = connection.username || (isFtp(connection.protocol) ? "anonymous" : "");
  const target = account ? `${account}@${address}` : address;
  return connection.protocol === "sftp" ? target : `${info.shortLabel}, ${target}`;
}
