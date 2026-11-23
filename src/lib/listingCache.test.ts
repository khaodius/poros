import { describe, expect, it } from "vitest";
import {
  ListingCache,
  freshenListing,
  heldEntries,
  listingScope,
  outlivesSession,
  sameListing,
  serverUnreachable,
  withHeldEntries,
  type StoredListing,
} from "./listingCache";
import type { ConnectProfile, DirListing, FileEntry, SessionInfo } from "./types";

function file(path: string, change: Partial<FileEntry> = {}): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size: 10,
    modified: 1_700_000_000,
    permissions: 0o644,
    owner: "deploy",
    group: "www",
    hidden: false,
    ...change,
  };
}

function listing(path: string, names: string[] = []): DirListing {
  const parent = path === "/" ? null : path.slice(0, path.lastIndexOf("/")) || "/";
  const prefix = path === "/" ? "/" : `${path}/`;
  return { path, parent, entries: names.map((name) => file(prefix + name)) };
}

const profile: ConnectProfile = {
  protocol: "sftp",
  host: "Web.Example.com",
  port: 22,
  username: "deploy",
  auth: { type: "agent" },
};

function session(id: string, change: Partial<SessionInfo> = {}): SessionInfo {
  return {
    id,
    label: "deploy@web.example.com",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/home/deploy",
    initialPath: "/home/deploy",
    protocol: "sftp",
    ...change,
  };
}

const SERVER = listingScope(session("one"), profile);

describe("listingScope", () => {
  it("is shared by the sessions of one account on one server, and by a reconnect", () => {
    expect(listingScope(session("two"), profile)).toBe(SERVER);
    const saved = { ...profile, savedConnectionId: "saved-1" };
    expect(listingScope(session("three", { savedConnectionId: "saved-1" }), saved)).toBe(SERVER);
    expect(outlivesSession(SERVER)).toBe(true);
  });

  it("keeps another account, port, protocol or jump host apart", () => {
    const others = [
      listingScope(session("two", { username: "root" }), profile),
      listingScope(session("two", { port: 2222 }), profile),
      listingScope(session("two", { protocol: "ftp" }), profile),
      listingScope(session("two", { host: "db.example.com" }), profile),
      listingScope(session("two"), { ...profile, jumpConnectionId: "bastion" }),
    ];
    expect(new Set([SERVER, ...others]).size).toBe(others.length + 1);
  });

  it("gives an unlocked vault a name that ends with its session", () => {
    const vault = { parentSessionId: "one", root: "/srv/vault", name: "vault" };
    const first = listingScope(session("vault-1", { vault }), null);
    const second = listingScope(session("vault-2", { vault }), null);
    expect(first).not.toBe(second);
    expect(first).not.toBe(SERVER);
    expect(outlivesSession(first)).toBe(false);
  });

  it("keeps a server whose route is unknown to itself", () => {
    const unknown = listingScope(session("one"), null);
    expect(unknown).not.toBe(SERVER);
    expect(outlivesSession(unknown)).toBe(false);
  });
});

describe("ListingCache", () => {
  it("remembers a folder per connection and path, however the path is spelled", () => {
    const cache = new ListingCache();
    cache.put(SERVER, listing("/srv/www", ["index.html"]));
    expect(cache.get(SERVER, "/srv/www")?.entries).toHaveLength(1);
    expect(cache.get(SERVER, "/srv/www/")?.path).toBe("/srv/www");
    expect(cache.get(SERVER, "/srv")).toBeUndefined();
    expect(cache.get(listingScope(session("x", { username: "root" }), profile), "/srv/www")).toBe(
      undefined,
    );
  });

  it("replaces a folder's listing and counts its entries once", () => {
    const cache = new ListingCache();
    cache.put(SERVER, listing("/srv", ["a", "b", "c"]));
    cache.put(SERVER, listing("/srv", ["a"]));
    expect(cache.folderCount).toBe(1);
    expect(cache.entryCount).toBe(1);
  });

  it("forgets the folder used longest ago once there are too many", () => {
    const cache = new ListingCache({ folders: 2, entries: 100 });
    cache.put(SERVER, listing("/a", ["1"]));
    cache.put(SERVER, listing("/b", ["1"]));
    // Looking a folder up counts as using it.
    cache.get(SERVER, "/a");
    cache.put(SERVER, listing("/c", ["1"]));
    expect(cache.has(SERVER, "/a")).toBe(true);
    expect(cache.has(SERVER, "/b")).toBe(false);
    expect(cache.has(SERVER, "/c")).toBe(true);
  });

  it("forgets old folders once their entries are too many, and never the newest", () => {
    const cache = new ListingCache({ folders: 10, entries: 5 });
    cache.put(SERVER, listing("/a", ["1", "2"]));
    cache.put(SERVER, listing("/b", ["1", "2"]));
    cache.put(SERVER, listing("/c", ["1", "2", "3"]));
    expect(cache.has(SERVER, "/a")).toBe(false);
    expect(cache.has(SERVER, "/b")).toBe(true);
    expect(cache.has(SERVER, "/c")).toBe(true);
    expect(cache.entryCount).toBe(5);
  });

  it("does not keep a folder larger than everything it may hold", () => {
    const cache = new ListingCache({ folders: 10, entries: 2 });
    cache.put(SERVER, listing("/big", ["1"]));
    expect(cache.put(SERVER, listing("/big", ["1", "2", "3"]))).toBe(false);
    expect(cache.has(SERVER, "/big")).toBe(false);
    expect(cache.entryCount).toBe(0);
  });

  it("forgets one folder, or a folder with everything inside it", () => {
    const cache = new ListingCache();
    for (const path of ["/srv", "/srv/www", "/srv/www/css", "/srv/www2", "/var"]) {
      cache.put(SERVER, listing(path, ["x"]));
    }
    cache.invalidate(SERVER, "/srv");
    expect(cache.has(SERVER, "/srv")).toBe(false);
    expect(cache.has(SERVER, "/srv/www")).toBe(true);
    cache.invalidateSubtree(SERVER, "/srv/www");
    expect(cache.has(SERVER, "/srv/www")).toBe(false);
    expect(cache.has(SERVER, "/srv/www/css")).toBe(false);
    // A name that only starts the same is another folder.
    expect(cache.has(SERVER, "/srv/www2")).toBe(true);
    expect(cache.has(SERVER, "/var")).toBe(true);
    expect(cache.entryCount).toBe(2);
  });

  it("drops a vault's decrypted listings when it locks, and nothing else", () => {
    const cache = new ListingCache();
    const vault = { parentSessionId: "one", root: "/srv/vault", name: "vault" };
    const unlocked = listingScope(session("vault-1", { vault }), null);
    cache.put(SERVER, listing("/srv/vault", ["vault.cryptomator", "masterkey.cryptomator"]));
    cache.put(unlocked, listing("/srv/vault", ["plans.txt"]));
    cache.put(unlocked, listing("/srv/vault/private", ["diary.txt"]));
    // The same path holds the encrypted folder for the server and the decrypted one for the vault.
    expect(cache.get(unlocked, "/srv/vault")?.entries[0].name).toBe("plans.txt");
    expect(cache.get(SERVER, "/srv/vault")?.entries[0].name).toBe("vault.cryptomator");
    cache.dropScope(unlocked);
    expect(cache.has(unlocked, "/srv/vault")).toBe(false);
    expect(cache.has(unlocked, "/srv/vault/private")).toBe(false);
    expect(cache.has(SERVER, "/srv/vault")).toBe(true);
    expect(cache.snapshot(() => true).map((kept) => kept.scope)).toEqual([SERVER]);
  });

  it("does not keep a listing whose folder changed while it was on its way", () => {
    const cache = new ListingCache();
    const before = cache.begin(SERVER, "/srv");
    cache.invalidate(SERVER, "/srv");
    expect(cache.put(SERVER, listing("/srv", ["old"]), before)).toBe(false);
    expect(cache.has(SERVER, "/srv")).toBe(false);

    const inside = cache.begin(SERVER, "/srv/www/css");
    cache.invalidateSubtree(SERVER, "/srv/www");
    expect(cache.put(SERVER, listing("/srv/www/css", ["old"]), inside)).toBe(false);

    const elsewhere = cache.begin(SERVER, "/var");
    cache.invalidate(SERVER, "/srv");
    expect(cache.put(SERVER, listing("/var", ["log"]), elsewhere)).toBe(true);

    const after = cache.begin(SERVER, "/srv");
    expect(cache.put(SERVER, listing("/srv", ["new"]), after)).toBe(true);
  });

  it("keeps the folder's later answer when an earlier one arrives after it", () => {
    const cache = new ListingCache();
    const seen: string[] = [];
    cache.watch((stored) => seen.push(stored.listing.entries[0].name));
    const earlier = cache.begin(SERVER, "/srv");
    const later = cache.begin(SERVER, "/srv");
    expect(cache.put(SERVER, listing("/srv", ["new"]), later)).toBe(true);
    expect(cache.put(SERVER, listing("/srv", ["old"]), earlier)).toBe(false);
    expect(cache.get(SERVER, "/srv")?.entries[0].name).toBe("new");
    expect(seen).toEqual(["new"]);
    // Another folder asked for earlier is not held back by it.
    const elsewhere = cache.begin(SERVER, "/var");
    cache.begin(SERVER, "/srv");
    expect(cache.put(SERVER, listing("/var", ["log"]), elsewhere)).toBe(true);
  });

  it("does not keep a listing asked for before its vault locked", () => {
    const cache = new ListingCache();
    const vault = listingScope(
      session("vault-1", { vault: { parentSessionId: "one", root: "/v", name: "v" } }),
      null,
    );
    const asked = cache.begin(vault, "/v");
    cache.dropScope(vault);
    expect(cache.put(vault, listing("/v", ["secret.txt"]), asked)).toBe(false);
    expect(cache.folderCount).toBe(0);
  });

  it("finds a folder again under the path it was asked for, as a link to it is", () => {
    const cache = new ListingCache();
    const asked = cache.begin(SERVER, "/home/deploy/site");
    cache.put(SERVER, listing("/srv/www", ["index.html"]), asked);
    expect(cache.get(SERVER, "/home/deploy/site")?.path).toBe("/srv/www");
    // A reload of the folder itself keeps the link's path pointing at it.
    cache.put(SERVER, listing("/srv/www", ["index.html", "new.html"]));
    expect(cache.get(SERVER, "/home/deploy/site")?.entries).toHaveLength(2);
    cache.invalidate(SERVER, "/srv/www");
    expect(cache.has(SERVER, "/home/deploy/site")).toBe(false);
  });

  it("tells those who watch which listing was taken in, and by whom", () => {
    const cache = new ListingCache();
    const seen: StoredListing[] = [];
    const stop = cache.watch((stored) => seen.push(stored));
    const pane = {};
    cache.put(SERVER, listing("/srv", ["a"]), undefined, pane);
    const rejected = cache.begin(SERVER, "/srv");
    cache.invalidate(SERVER, "/srv");
    cache.put(SERVER, listing("/srv", ["b"]), rejected, pane);
    stop();
    cache.put(SERVER, listing("/srv", ["c"]));
    expect(seen).toHaveLength(1);
    expect(seen[0].by).toBe(pane);
    expect(seen[0].listing.entries[0].name).toBe("a");
  });

  it("forgets everything at once", () => {
    const cache = new ListingCache();
    const asked = cache.begin(SERVER, "/b");
    cache.put(SERVER, listing("/a", ["1"]));
    cache.clear();
    expect(cache.folderCount).toBe(0);
    expect(cache.entryCount).toBe(0);
    expect(cache.put(SERVER, listing("/b", ["1"]), asked)).toBe(false);
  });
});

describe("comparing listings", () => {
  it("tells the same folder with the same entries, in whatever order", () => {
    const first = listing("/srv", ["a", "b"]);
    const second = { ...first, entries: [...listing("/srv", ["a", "b"]).entries].reverse() };
    expect(sameListing(first, second)).toBe(true);
    expect(sameListing(first, listing("/srv", ["a"]))).toBe(false);
    expect(sameListing(first, listing("/var", ["a", "b"]))).toBe(false);
    const grown = listing("/srv", ["a", "b"]);
    grown.entries[1] = { ...grown.entries[1], size: 11 };
    expect(sameListing(first, grown)).toBe(false);
  });

  it("changes nothing when the server lists what is on show", () => {
    const shown = listing("/srv", ["a", "b"]);
    expect(freshenListing(shown, listing("/srv", ["a", "b"]))).toBe(shown);
  });

  it("keeps the entries that did not change as the objects they were", () => {
    const shown = listing("/srv", ["a", "b"]);
    const fresh = listing("/srv", ["a", "b", "c"]);
    fresh.entries[1] = { ...fresh.entries[1], size: 99 };
    const next = freshenListing(shown, fresh);
    expect(next.entries.map((entry) => entry.name)).toEqual(["a", "b", "c"]);
    expect(next.entries[0]).toBe(shown.entries[0]);
    expect(next.entries[1]).not.toBe(shown.entries[1]);
    expect(next.entries[1].size).toBe(99);
  });

  it("keeps a row the user is busy with when the server no longer lists it", () => {
    const shown = listing("/srv", ["a", "b"]);
    const fresh = listing("/srv", ["a"]);
    const held = new Set(["/srv/b"]);
    const kept = heldEntries(shown.entries, fresh, held);
    expect(kept.map((entry) => entry.path)).toEqual(["/srv/b"]);
    expect(withHeldEntries(fresh, kept, held)?.entries.map((entry) => entry.name)).toEqual([
      "a",
      "b",
    ]);
    // Once the rename or the menu is over, the row goes.
    expect(withHeldEntries(fresh, kept, new Set())).toBe(fresh);
    expect(heldEntries(shown.entries, fresh, new Set(["/srv/a"]))).toEqual([]);
  });

  it("tells a server that cannot be reached from a folder that cannot be listed", () => {
    expect(serverUnreachable({ kind: "disconnected", message: "gone" })).toBe(true);
    expect(serverUnreachable({ kind: "timeout", message: "slow" })).toBe(true);
    expect(serverUnreachable({ kind: "notFound", message: "no such folder" })).toBe(false);
    expect(serverUnreachable({ kind: "permissionDenied", message: "no" })).toBe(false);
  });
});
