// How the windows of one Poros share the listings they have seen. Every window is a page of
// its own, so each keeps its own copy of the cache and tells the others what it learns: a
// listing a server just gave, a folder Poros changed, a connection whose last session closed.
// A window that opens asks for what the others already know, and the main window, which is
// there as long as Poros runs, answers. Listings kept for one session only, as a vault's
// decrypted ones are, are never told to another window. Pure, so it is testable;
// `state/listings.ts` gives it the events that carry the messages.

import { ListingCache, outlivesSession, type ListingTicket } from "./listingCache";
import type { DirListing, EntryKind, FileEntry, LinkTarget } from "./types";

/** How long a saved connection's listings stay once its last session has closed. */
export const SAVED_RETENTION_MILLIS = 10 * 60 * 1000;
/** A folder with more entries than this is not sent to the other windows, which list it anew. */
export const SHARED_LISTING_ENTRIES = 2000;
/** How many entries a window that opens is given at most, the folders used last first. */
export const WELCOME_ENTRIES = 10_000;

export interface SharedListing {
  scope: string;
  listing: DirListing;
}

export type ListingMessage =
  /** A window opened and asks what the others know. */
  | { kind: "hello"; from: string }
  /** The answer to it: everything the main window remembers, nothing from the others. */
  | { kind: "welcome"; from: string; listings: SharedListing[] }
  | { kind: "store"; from: string; scope: string; listing: DirListing }
  /** Poros changed `folder`; `entries` are the folders in it that are no longer what they were. */
  | { kind: "forget"; from: string; scope: string; folder: string; entries: string[] }
  /** Anything on the connection may have changed, as after a command run on the server. */
  | { kind: "forgetAll"; from: string; scope: string }
  /** The last session a window had on the connection closed. */
  | { kind: "closed"; from: string; scope: string; saved: boolean };

const ENTRY_KINDS: readonly EntryKind[] = ["dir", "file", "symlink", "other"];
const LINK_TARGETS: readonly LinkTarget[] = ["dir", "file", "broken"];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isOptional(value: unknown, type: "string" | "number"): boolean {
  return value === null || value === undefined || typeof value === type;
}

function isEntry(value: unknown): value is FileEntry {
  return (
    isRecord(value) &&
    typeof value.name === "string" &&
    typeof value.path === "string" &&
    ENTRY_KINDS.includes(value.kind as EntryKind) &&
    (value.linkTarget === undefined || LINK_TARGETS.includes(value.linkTarget as LinkTarget)) &&
    typeof value.size === "number" &&
    typeof value.hidden === "boolean" &&
    isOptional(value.modified, "number") &&
    isOptional(value.permissions, "number") &&
    isOptional(value.owner, "string") &&
    isOptional(value.group, "string")
  );
}

function isListing(value: unknown): value is DirListing {
  return (
    isRecord(value) &&
    typeof value.path === "string" &&
    (value.parent === null || typeof value.parent === "string") &&
    Array.isArray(value.entries) &&
    value.entries.every(isEntry)
  );
}

function isShared(value: unknown): value is SharedListing {
  return isRecord(value) && typeof value.scope === "string" && isListing(value.listing);
}

/** A message another window sent, or null for anything that is not one. */
export function parseListingMessage(value: unknown): ListingMessage | null {
  if (!isRecord(value) || typeof value.from !== "string") return null;
  const from = value.from;
  const scope = typeof value.scope === "string" ? value.scope : null;
  switch (value.kind) {
    case "hello":
      return { kind: "hello", from };
    case "welcome":
      return Array.isArray(value.listings) && value.listings.every(isShared)
        ? { kind: "welcome", from, listings: value.listings }
        : null;
    case "store":
      return scope !== null && isListing(value.listing)
        ? { kind: "store", from, scope, listing: value.listing }
        : null;
    case "forget": {
      const entries = Array.isArray(value.entries) ? value.entries : [];
      return scope !== null &&
        typeof value.folder === "string" &&
        entries.every((entry) => typeof entry === "string")
        ? { kind: "forget", from, scope, folder: value.folder, entries: entries as string[] }
        : null;
    }
    case "forgetAll":
      return scope !== null ? { kind: "forgetAll", from, scope } : null;
    case "closed":
      return scope !== null ? { kind: "closed", from, scope, saved: value.saved === true } : null;
    default:
      return null;
  }
}

/** What a window gives the sharing: its name, a way to reach the others, and its sessions. */
export interface ShareHost {
  /** The window's label. */
  label: string;
  /** The window Poros starts with, which answers the windows that open later. */
  main: boolean;
  /** Sends a message to the window `to`, or to every window when left out. */
  send: (message: ListingMessage, to?: string) => void;
  /** Whether a session of this window is on the connection `scope` names. */
  inUse: (scope: string) => boolean;
  /**
   * Another window changed `folder`, and the folders among `entries` with all inside them: panes
   * showing any of those here have something to reload.
   */
  changedElsewhere: (scope: string, folder: string, entries: readonly string[]) => void;
  later: (run: () => void, millis: number) => unknown;
  cancel: (timer: unknown) => void;
}

export class ListingShare {
  /** The windows heard from, which are the ones told. */
  private readonly peers = new Set<string>();
  /** The connections each of them has listed folders on. */
  private readonly listedBy = new Map<string, Set<string>>();
  /** Connections without a session here, and the timer that forgets each. */
  private readonly expiring = new Map<string, unknown>();

  constructor(
    readonly cache: ListingCache,
    private readonly host: ShareHost,
  ) {}

  /** Asks the windows already open for what they know. */
  join(): void {
    this.host.send({ kind: "hello", from: this.host.label });
  }

  /** The windows that are told what this one learns. */
  get audience(): readonly string[] {
    return [...this.peers];
  }

  /**
   * Stops telling the windows that are no longer open. A window's sessions close with it, so
   * the listings of the connections it was on go as well, unless a window still open is on them.
   */
  prune(open: readonly string[]): void {
    const gone = [...this.peers].filter((peer) => !open.includes(peer));
    for (const peer of gone) this.peers.delete(peer);
    for (const peer of gone) {
      const scopes = this.listedBy.get(peer) ?? [];
      this.listedBy.delete(peer);
      for (const scope of scopes) {
        const listedElsewhere = [...this.listedBy.values()].some((other) => other.has(scope));
        if (!listedElsewhere && !this.host.inUse(scope)) this.release(scope, false);
      }
    }
  }

  /** Remembers a listing the server gave this window, and tells the others. */
  store(scope: string, listing: DirListing, ticket?: ListingTicket, by?: unknown): boolean {
    this.keep(scope);
    const kept = this.cache.put(scope, listing, ticket, by);
    if (kept && listing.entries.length <= SHARED_LISTING_ENTRIES) {
      this.tell(scope, { kind: "store", from: this.host.label, scope, listing });
    }
    return kept;
  }

  /**
   * Forgets the listing of a folder Poros changed, and of the folders among `entries`, which
   * were moved, deleted or written over, with everything inside them. The other windows too.
   */
  forget(scope: string, folder: string, entries: readonly string[] = []): void {
    this.drop(scope, folder, entries);
    this.tell(scope, {
      kind: "forget",
      from: this.host.label,
      scope,
      folder,
      entries: [...entries],
    });
  }

  /** Forgets every listing of a connection on which anything may have changed. */
  forgetAll(scope: string): void {
    this.cache.dropScope(scope);
    this.tell(scope, { kind: "forgetAll", from: this.host.label, scope });
  }

  /**
   * A session on the connection closed for good. Unless another one here is still on it, its
   * listings go: at once, or for a saved connection, which is likely to be opened again, after
   * a while. The other windows do the same unless they have a session on it themselves.
   */
  closed(scope: string, saved: boolean): void {
    if (!outlivesSession(scope)) {
      this.cache.dropScope(scope);
      return;
    }
    if (this.host.inUse(scope)) return;
    this.tell(scope, { kind: "closed", from: this.host.label, scope, saved });
    this.release(scope, saved);
  }

  /** Takes in what another window sent. */
  receive(message: ListingMessage): void {
    if (message.from === this.host.label) return;
    this.peers.add(message.from);
    switch (message.kind) {
      case "hello":
        this.host.send(
          { kind: "welcome", from: this.host.label, listings: this.host.main ? this.known() : [] },
          message.from,
        );
        return;
      case "welcome":
        // What this window has listed itself since it opened is newer than what it is told.
        for (const { scope, listing } of message.listings) {
          if (outlivesSession(scope) && !this.cache.has(scope, listing.path)) {
            this.cache.put(scope, listing, undefined, message.from);
          }
        }
        return;
      case "store":
        if (!outlivesSession(message.scope)) return;
        this.noteListedBy(message.from, message.scope);
        this.keep(message.scope);
        this.cache.put(message.scope, message.listing, undefined, message.from);
        return;
      case "forget":
        this.drop(message.scope, message.folder, message.entries);
        this.host.changedElsewhere(message.scope, message.folder, message.entries);
        return;
      case "forgetAll":
        this.cache.dropScope(message.scope);
        return;
      case "closed":
        this.listedBy.get(message.from)?.delete(message.scope);
        if (!this.host.inUse(message.scope)) this.release(message.scope, message.saved);
        return;
    }
  }

  /** What a window that opens is told: the folders used last, as far as they fit one message. */
  private known(): SharedListing[] {
    const told: SharedListing[] = [];
    let entries = 0;
    for (const shared of this.cache.snapshot(outlivesSession).reverse()) {
      const size = shared.listing.entries.length;
      if (size > SHARED_LISTING_ENTRIES || entries + size > WELCOME_ENTRIES) continue;
      entries += size;
      told.push(shared);
    }
    // Oldest first, so that the window taking them in ends with the same order of use.
    return told.reverse();
  }

  private noteListedBy(peer: string, scope: string): void {
    const scopes = this.listedBy.get(peer) ?? new Set<string>();
    scopes.add(scope);
    this.listedBy.set(peer, scopes);
  }

  private drop(scope: string, folder: string, entries: readonly string[]): void {
    this.cache.invalidate(scope, folder);
    for (const entry of entries) this.cache.invalidateSubtree(scope, entry);
  }

  private tell(scope: string, message: ListingMessage): void {
    if (!outlivesSession(scope)) return;
    for (const peer of this.peers) this.host.send(message, peer);
  }

  /** The connection is in use again, here or in another window: its listings stay. */
  private keep(scope: string): void {
    if (!this.expiring.has(scope)) return;
    this.host.cancel(this.expiring.get(scope));
    this.expiring.delete(scope);
  }

  private release(scope: string, saved: boolean): void {
    this.keep(scope);
    if (!saved) {
      this.cache.dropScope(scope);
      return;
    }
    const timer = this.host.later(() => {
      this.expiring.delete(scope);
      if (!this.host.inUse(scope)) this.cache.dropScope(scope);
    }, SAVED_RETENTION_MILLIS);
    this.expiring.set(scope, timer);
  }
}
