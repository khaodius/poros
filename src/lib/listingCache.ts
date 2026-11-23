// The listings of server folders last seen, kept in memory so that a folder opened before shows
// at once while the server is asked for it again. Listings are kept per connection: every
// session signed in to one account on one server shares them, a vault's decrypted listings are
// its own, and nothing is ever written to disk, since a listing gives file names away. The cache
// holds a bounded number of folders and of entries, and forgets the folders used longest ago
// first. Pure, so it is testable; `state/listings.ts` keeps the one a window uses.

import { isWithin, normalizedPath } from "./path";
import type { AppError, ConnectProfile, DirListing, FileEntry, SessionInfo } from "./types";

export interface ListingCacheLimits {
  /** How many folders are remembered at most. */
  folders: number;
  /** How many entries those folders hold between them at most. */
  entries: number;
}

export const LISTING_CACHE_LIMITS: ListingCacheLimits = { folders: 300, entries: 50_000 };

type ScopeKind = "server" | "vault" | "session";

function scopeOf(kind: ScopeKind, ...parts: (string | number | null)[]): string {
  return JSON.stringify([kind, ...parts]);
}

/**
 * Names the connection a session's listings belong to. Sessions of one account on one server,
 * reached the same way, share a name, so two tabs share what they have seen and a reconnect
 * finds it again; another account, port or jump host is another name. An unlocked vault has a
 * name of its own, which ends with its session: decrypted names never mix with the encrypted
 * folder underneath and never outlive the vault being open. A server whose route is not known
 * here is kept apart in the same way.
 */
export function listingScope(info: SessionInfo, profile: ConnectProfile | null): string {
  if (info.vault) return scopeOf("vault", info.id);
  if (!profile) return scopeOf("session", info.id);
  return scopeOf(
    "server",
    info.protocol,
    info.host.toLowerCase(),
    info.port,
    info.username,
    profile.jumpConnectionId ?? null,
  );
}

/**
 * Whether the listings under a name belong to a server rather than to one session: only those
 * are shared between windows and may stay once the session has closed.
 */
export function outlivesSession(scope: string): boolean {
  return scope.startsWith('["server",');
}

/** Errors that say the folder itself is gone or closed to the user. */
const UNAVAILABLE: ReadonlySet<AppError["kind"]> = new Set(["notFound", "permissionDenied"]);

/**
 * Whether listing a folder failed because the folder can no longer be listed, rather than for a
 * reason that may pass: a server out of reach, slow or busy, or a reply it could not give now.
 */
export function folderUnavailable(error: AppError): boolean {
  return UNAVAILABLE.has(error.kind);
}

const ENTRY_FIELDS = [
  "name",
  "path",
  "kind",
  "linkTarget",
  "size",
  "modified",
  "permissions",
  "owner",
  "group",
  "hidden",
] as const satisfies readonly (keyof FileEntry)[];

export function sameEntry(first: FileEntry, second: FileEntry): boolean {
  return ENTRY_FIELDS.every((field) => first[field] === second[field]);
}

/** Whether two listings show the same folder with the same entries, in whatever order. */
export function sameListing(first: DirListing, second: DirListing): boolean {
  if (first === second) return true;
  if (first.path !== second.path || first.parent !== second.parent) return false;
  if (first.entries.length !== second.entries.length) return false;
  const known = new Map(first.entries.map((entry) => [entry.path, entry]));
  return second.entries.every((entry) => {
    const match = known.get(entry.path);
    return match !== undefined && sameEntry(match, entry);
  });
}

/**
 * The listing to show once the server has answered for the folder already on show: `shown`
 * itself when nothing differs, so nothing is drawn again, and otherwise `fresh` with every entry
 * that did not change kept as the object it was, so only rows that changed are drawn again.
 */
export function freshenListing(shown: DirListing, fresh: DirListing): DirListing {
  if (sameListing(shown, fresh)) return shown;
  if (shown.path !== fresh.path) return fresh;
  const known = new Map(shown.entries.map((entry) => [entry.path, entry]));
  const entries = fresh.entries.map((entry) => {
    const before = known.get(entry.path);
    return before && sameEntry(before, entry) ? before : entry;
  });
  return { ...fresh, entries };
}

/**
 * The entries among `shown` that the user is in the middle of something with (`held`: a rename,
 * an open menu) and that `fresh` no longer lists. Their rows stay until that is over, rather
 * than vanish from under the pointer.
 */
export function heldEntries(
  shown: readonly FileEntry[],
  fresh: DirListing,
  held: ReadonlySet<string>,
): FileEntry[] {
  if (held.size === 0) return [];
  const listed = new Set(fresh.entries.map((entry) => entry.path));
  const kept = new Map<string, FileEntry>();
  for (const entry of shown) {
    if (held.has(entry.path) && !listed.has(entry.path)) kept.set(entry.path, entry);
  }
  return [...kept.values()];
}

/** `listing` with the rows kept for the user added, or `listing` itself when there are none. */
export function withHeldEntries(
  listing: DirListing | null,
  kept: readonly FileEntry[],
  held: ReadonlySet<string>,
): DirListing | null {
  if (!listing || kept.length === 0) return listing;
  const listed = new Set(listing.entries.map((entry) => entry.path));
  const added = kept.filter((entry) => held.has(entry.path) && !listed.has(entry.path));
  return added.length === 0 ? listing : { ...listing, entries: [...listing.entries, ...added] };
}

/** A folder asked for, to tell on its answer whether the folder changed in the meantime. */
export interface ListingTicket {
  readonly scope: string;
  readonly path: string;
  /** Counts the folders asked for, so an answer can tell whether a later one was kept already. */
  readonly serial: number;
  /** What changed on the connection since, to match against the folder the answer names. */
  changes: { path: string; subtree: boolean }[];
  outdated: boolean;
}

/** More changes than this while one listing is on its way, and it is simply not kept. */
const MAX_TICKET_CHANGES = 64;

/** A listing the cache took in, told to those who watch it. */
export interface StoredListing {
  scope: string;
  listing: DirListing;
  /** Whoever stored it, so they can tell their own listing from another's. */
  by: unknown;
}

interface Slot {
  scope: string;
  listing: DirListing;
  /** The serial of the ticket the listing was asked for with; 0 for one taken in otherwise. */
  serial: number;
}

function slotKey(scope: string, path: string): string {
  return `${scope}\n${normalizedPath(path, "posix")}`;
}

export class ListingCache {
  /** Oldest use first, as a Map keeps the order its keys were set in. */
  private readonly slots = new Map<string, Slot>();
  /**
   * Paths that were asked for and answered under another path, as a link to a folder is: the
   * key asked for, and the key its listing is kept under.
   */
  private readonly aliases = new Map<string, string>();
  private readonly pending = new Set<ListingTicket>();
  /**
   * Folders the server listed since they last changed that were too large to keep, by key, so
   * that they are not listed once more only because they are not remembered.
   */
  private readonly oversized = new Set<string>();
  private readonly watchers = new Set<(stored: StoredListing) => void>();
  private entryTotal = 0;
  private lastSerial = 0;

  constructor(private readonly limits: ListingCacheLimits = LISTING_CACHE_LIMITS) {}

  get folderCount(): number {
    return this.slots.size;
  }

  get entryCount(): number {
    return this.entryTotal;
  }

  private resolve(scope: string, path: string): string {
    const key = slotKey(scope, path);
    return this.slots.has(key) ? key : (this.aliases.get(key) ?? key);
  }

  /** Whether the folder's listing is remembered; does not count as using it. */
  has(scope: string, path: string): boolean {
    return this.slots.has(this.resolve(scope, path));
  }

  /**
   * Whether the server listed the folder since it last changed: its listing is remembered, or
   * was too large to be. Does not count as using it.
   */
  listedSinceChange(scope: string, path: string): boolean {
    return this.has(scope, path) || this.oversized.has(slotKey(scope, path));
  }

  /** The folder's last known listing, which counts as using it. */
  get(scope: string, path: string): DirListing | undefined {
    const key = this.resolve(scope, path);
    const slot = this.slots.get(key);
    if (!slot) return undefined;
    this.slots.delete(key);
    this.slots.set(key, slot);
    return slot.listing;
  }

  /** Notes that the folder is being asked for; its answer goes to `put` or to `cancel`. */
  begin(scope: string, path: string): ListingTicket {
    this.lastSerial += 1;
    const ticket: ListingTicket = {
      scope,
      path,
      serial: this.lastSerial,
      changes: [],
      outdated: false,
    };
    this.pending.add(ticket);
    return ticket;
  }

  /** Forgets a ticket whose folder could not be listed. */
  cancel(ticket: ListingTicket): void {
    this.pending.delete(ticket);
  }

  /**
   * Remembers a listing the server gave, and tells the watchers. With the `ticket` taken when
   * it was asked for, a listing is not kept when its folder changed while it was on its way, as
   * it may be from before the change, nor when the folder was asked for again later and that
   * answer came first. A listing larger than the whole cache is not kept either. Returns whether
   * it was.
   */
  put(scope: string, listing: DirListing, ticket?: ListingTicket, by?: unknown): boolean {
    const key = slotKey(scope, listing.path);
    const replaced = this.slots.get(key);
    if (ticket) {
      this.pending.delete(ticket);
      if (ticket.outdated || ticket.scope !== scope) return false;
      if (replaced && replaced.serial > ticket.serial) return false;
      const changed = ticket.changes.some(({ path, subtree }) =>
        [ticket.path, listing.path].some((folder) =>
          subtree
            ? isWithin(folder, path, "posix")
            : normalizedPath(folder, "posix") === normalizedPath(path, "posix"),
        ),
      );
      if (changed) return false;
    }
    if (listing.entries.length > this.limits.entries) {
      this.remove(key);
      this.oversized.delete(key);
      this.oversized.add(key);
      // Only ever a few, but bounded all the same, the oldest out first.
      if (this.oversized.size > this.limits.folders) {
        this.oversized.delete(this.oversized.values().next().value!);
      }
      return false;
    }
    this.oversized.delete(key);
    if (replaced) {
      this.slots.delete(key);
      this.entryTotal -= replaced.listing.entries.length;
    }
    this.slots.set(key, { scope, listing, serial: ticket?.serial ?? 0 });
    this.entryTotal += listing.entries.length;
    if (ticket) {
      const asked = slotKey(scope, ticket.path);
      if (asked !== key) this.aliases.set(asked, key);
    }
    this.trim(key);
    for (const watcher of [...this.watchers]) watcher({ scope, listing, by });
    return true;
  }

  /** Calls `watcher` with each listing taken in from now on; the function returned stops it. */
  watch(watcher: (stored: StoredListing) => void): () => void {
    this.watchers.add(watcher);
    return () => {
      this.watchers.delete(watcher);
    };
  }

  /** Forgets one folder's listing: something in the folder changed. */
  invalidate(scope: string, path: string): void {
    this.note(scope, { path, subtree: false });
    this.remove(this.resolve(scope, path));
    this.aliases.delete(slotKey(scope, path));
    this.oversized.delete(slotKey(scope, path));
  }

  /** Forgets the listing of `path` and of every folder inside it, as after it is deleted. */
  invalidateSubtree(scope: string, path: string): void {
    this.note(scope, { path, subtree: true });
    for (const [key, slot] of [...this.slots]) {
      if (slot.scope === scope && isWithin(slot.listing.path, path, "posix")) this.remove(key);
    }
    const prefix = `${scope}\n`;
    const inside = (key: string) =>
      key.startsWith(prefix) && isWithin(key.slice(prefix.length), path, "posix");
    for (const asked of [...this.aliases.keys()]) {
      if (inside(asked)) this.aliases.delete(asked);
    }
    for (const key of [...this.oversized]) {
      if (inside(key)) this.oversized.delete(key);
    }
  }

  /** Forgets everything kept under one name: a vault locked, a session gone for good. */
  dropScope(scope: string): void {
    for (const ticket of this.pending) {
      if (ticket.scope === scope) ticket.outdated = true;
    }
    for (const [key, slot] of [...this.slots]) {
      if (slot.scope === scope) this.remove(key);
    }
    for (const key of [...this.oversized]) {
      if (key.startsWith(`${scope}\n`)) this.oversized.delete(key);
    }
  }

  clear(): void {
    for (const ticket of this.pending) ticket.outdated = true;
    this.slots.clear();
    this.aliases.clear();
    this.oversized.clear();
    this.entryTotal = 0;
  }

  /** What is remembered under the names `wanted` picks, used longest ago first. */
  snapshot(wanted: (scope: string) => boolean): { scope: string; listing: DirListing }[] {
    return [...this.slots.values()]
      .filter((slot) => wanted(slot.scope))
      .map(({ scope, listing }) => ({ scope, listing }));
  }

  private note(scope: string, change: { path: string; subtree: boolean }): void {
    for (const ticket of this.pending) {
      if (ticket.scope !== scope) continue;
      if (ticket.changes.length >= MAX_TICKET_CHANGES) ticket.outdated = true;
      else ticket.changes.push(change);
    }
  }

  private remove(key: string): void {
    const slot = this.slots.get(key);
    if (!slot) return;
    this.slots.delete(key);
    this.entryTotal -= slot.listing.entries.length;
    for (const [asked, target] of [...this.aliases]) {
      if (target === key) this.aliases.delete(asked);
    }
  }

  /** Forgets the folders used longest ago until the cache is within its limits again. */
  private trim(keep: string): void {
    for (const key of [...this.slots.keys()]) {
      const within =
        this.slots.size <= this.limits.folders && this.entryTotal <= this.limits.entries;
      if (within) return;
      if (key !== keep) this.remove(key);
    }
  }
}
