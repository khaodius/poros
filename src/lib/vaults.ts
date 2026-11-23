// Cryptomator vaults kept on a server: telling a vault's folder by its listing, and which
// session, the server's or an unlocked vault's, shows a path.

import { isWithin } from "./path";
import type { FileEntry, SessionInfo, VaultInfo } from "./types";

/** The file that makes a folder a vault, since vault format 8 (Cryptomator 1.6). */
export const VAULT_CONFIG_NAME = "vault.cryptomator";
/** The key file, which with the data folder and no configuration marks an older vault. */
export const VAULT_MASTERKEY_NAME = "masterkey.cryptomator";
export const VAULT_DATA_FOLDER = "d";

export const MIN_VAULT_PASSWORD_LENGTH = 8;

/** What a listing says of its folder: a vault to unlock, one too old to open, or neither. */
export type VaultSign = "none" | "locked" | "outdated";

export function vaultSign(entries: readonly FileEntry[]): VaultSign {
  const holds = (name: string, kind: FileEntry["kind"]) =>
    entries.some((entry) => entry.name === name && entry.kind === kind);
  if (holds(VAULT_CONFIG_NAME, "file")) return "locked";
  return holds(VAULT_MASTERKEY_NAME, "file") && holds(VAULT_DATA_FOLDER, "dir")
    ? "outdated"
    : "none";
}

/** An absolute path with its `.` and `..` parts worked out, as a server reads it. */
function withoutDotParts(path: string): string {
  if (!path.startsWith("/")) return path;
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== "" && part !== ".") parts.push(part);
  }
  return `/${parts.join("/")}`;
}

/** Whether `path` is the vault's folder `root` or below it, where the vault's session lists. */
export function isInVault(root: string, path: string): boolean {
  return isWithin(withoutDotParts(path), root, "posix");
}

/** The sessions a window knows, by id, as far as vaults need them. */
export type KnownSessions = Readonly<Record<string, { readonly info: SessionInfo }>>;

function infoOf(sessions: KnownSessions, sessionId: string): SessionInfo | undefined {
  return (sessions[sessionId] as KnownSessions[string] | undefined)?.info;
}

/** The session of the server a session's files are on: its own, or that of a vault's server. */
export function serverSessionId(info: SessionInfo): string {
  return info.vault?.parentSessionId ?? info.id;
}

/** The session of an unlocked vault. */
export type VaultSession = SessionInfo & { vault: VaultInfo };

export function isVaultSession(info: SessionInfo): info is VaultSession {
  return info.vault !== undefined;
}

/** The sessions of the vaults unlocked on a server. */
export function vaultSessions(sessions: KnownSessions, serverId: string): VaultSession[] {
  return Object.values(sessions)
    .map((entry) => entry.info)
    .filter(isVaultSession)
    .filter((info) => info.vault.parentSessionId === serverId);
}

/**
 * A server's session and those of its unlocked vaults, the server's first, given any one of
 * them. They connect, move between windows and close together.
 */
export function sessionFamily(sessions: KnownSessions, sessionId: string): string[] {
  const info = infoOf(sessions, sessionId);
  if (!info) return [sessionId];
  const serverId = serverSessionId(info);
  return [serverId, ...vaultSessions(sessions, serverId).map((vault) => vault.id)];
}

interface WithStatus {
  readonly info: SessionInfo;
  readonly status: "connected" | "lost";
}

/**
 * Whether a session cannot be used now: it is lost or gone from this window, or it is a vault's
 * and its server is. A vault's connection is its server's.
 */
export function isLost(sessions: Readonly<Record<string, WithStatus>>, sessionId: string): boolean {
  const entry = sessions[sessionId] as WithStatus | undefined;
  if (!entry) return true;
  const server = sessions[serverSessionId(entry.info)] as WithStatus | undefined;
  return entry.status !== "connected" || server?.status !== "connected";
}

/** The vault among `vaults` that `path` is in; the innermost, were one to hold another. */
function vaultHolding(vaults: VaultSession[], path: string): VaultSession | undefined {
  return vaults
    .filter((info) => isInVault(info.vault.root, path))
    .sort((first, second) => second.vault.root.length - first.vault.root.length)[0];
}

/**
 * The session that shows `path` to a pane on `sessionId`: the unlocked vault of that pane's
 * server the path is in, or else the server itself. A vault and its server share their paths,
 * so this is what moves a pane into a vault and out of it again, whichever way it gets there.
 */
export function sessionShowing(sessions: KnownSessions, sessionId: string, path: string): string {
  const info = infoOf(sessions, sessionId);
  if (!info) return sessionId;
  const serverId = serverSessionId(info);
  const vault = vaultHolding(vaultSessions(sessions, serverId), path);
  if (vault) return vault.id;
  // Without its server in this window a vault keeps the pane, and refuses the path itself.
  return infoOf(sessions, serverId) ? serverId : sessionId;
}
