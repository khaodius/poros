import { describe, expect, it } from "vitest";
import { ListingCache, listingScope } from "./listingCache";
import {
  ListingShare,
  SAVED_RETENTION_MILLIS,
  SHARED_LISTING_ENTRIES,
  parseListingMessage,
  type ListingMessage,
} from "./listingShare";
import type { ConnectProfile, DirListing, FileEntry, SessionInfo } from "./types";

function file(path: string): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function listing(path: string, names: string[] = []): DirListing {
  return { path, parent: "/", entries: names.map((name) => file(`${path}/${name}`)) };
}

const profile: ConnectProfile = {
  protocol: "sftp",
  host: "web.example.com",
  port: 22,
  username: "deploy",
  auth: { type: "agent" },
};

function session(id: string, change: Partial<SessionInfo> = {}): SessionInfo {
  return {
    id,
    label: "web",
    host: "web.example.com",
    port: 22,
    username: "deploy",
    home: "/",
    initialPath: "/",
    protocol: "sftp",
    ...change,
  };
}

const SERVER = listingScope(session("one"), profile);
const VAULT = listingScope(
  session("vault", { vault: { parentSessionId: "one", root: "/vault", name: "vault" } }),
  null,
);

interface Timer {
  run: () => void;
  millis: number;
}

/** A few windows of one Poros, whose messages reach each other as soon as they are sent. */
class Windows {
  readonly shares = new Map<string, ListingShare>();
  readonly inUse = new Map<string, Set<string>>();
  readonly reloads: { window: string; scope: string; folder: string; entries: string[] }[] = [];
  readonly sent: { from: string; to?: string; message: ListingMessage }[] = [];
  readonly timers: Timer[] = [];

  open(label: string): ListingShare {
    const share = new ListingShare(new ListingCache(), {
      label,
      main: label === "main",
      send: (message, to) => {
        this.sent.push({ from: label, to, message });
        // As a window gets them: copied, and checked like anything a window is sent.
        const received = parseListingMessage(JSON.parse(JSON.stringify(message)));
        if (!received) throw new Error("A message sent would not be taken in");
        for (const [other, target] of this.shares) {
          if (other !== label && (to === undefined || to === other)) target.receive(received);
        }
      },
      inUse: (scope) => this.inUse.get(label)?.has(scope) ?? false,
      changedElsewhere: (scope, folder, entries) =>
        this.reloads.push({ window: label, scope, folder, entries: [...entries] }),
      later: (run, millis) => {
        const timer = { run, millis };
        this.timers.push(timer);
        return timer;
      },
      cancel: (timer) => {
        const index = this.timers.indexOf(timer as Timer);
        if (index >= 0) this.timers.splice(index, 1);
      },
    });
    this.shares.set(label, share);
    this.inUse.set(label, new Set());
    share.join();
    return share;
  }

  use(label: string, scope: string): void {
    this.inUse.get(label)?.add(scope);
  }

  leave(label: string, scope: string): void {
    this.inUse.get(label)?.delete(scope);
  }

  elapse(): void {
    for (const timer of this.timers.splice(0)) timer.run();
  }
}

describe("ListingShare", () => {
  it("tells the other windows each listing a server gives", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    main.store(SERVER, listing("/srv", ["a"]));
    expect(second.cache.get(SERVER, "/srv")?.entries).toHaveLength(1);
    second.store(SERVER, listing("/srv", ["a", "b"]));
    expect(main.cache.get(SERVER, "/srv")?.entries).toHaveLength(2);
  });

  it("gives a window that opens what the main window remembers", () => {
    const windows = new Windows();
    const main = windows.open("main");
    main.store(SERVER, listing("/srv", ["a"]));
    main.store(SERVER, listing("/var", ["log"]));
    const second = windows.open("window-2");
    expect(second.cache.has(SERVER, "/srv")).toBe(true);
    expect(second.cache.has(SERVER, "/var")).toBe(true);
    // Only the main window answers with listings, so they are not sent once per window.
    const third = windows.open("window-3");
    expect(third.cache.folderCount).toBe(2);
    const welcomes = windows.sent.filter(
      ({ to, message }) => to === "window-3" && message.kind === "welcome",
    );
    expect(welcomes.map(({ from }) => from).sort()).toEqual(["main", "window-2"]);
    const fromSecond = welcomes.find(({ from }) => from === "window-2")?.message;
    expect(fromSecond?.kind === "welcome" && fromSecond.listings).toEqual([]);
  });

  it("keeps what a window listed itself over what it is told on opening", () => {
    const windows = new Windows();
    const main = windows.open("main");
    main.store(SERVER, listing("/srv", ["old"]));
    const second = new ListingShare(new ListingCache(), {
      label: "window-2",
      main: false,
      send: () => undefined,
      inUse: () => true,
      changedElsewhere: () => undefined,
      later: () => undefined,
      cancel: () => undefined,
    });
    second.cache.put(SERVER, listing("/srv", ["new"]));
    second.receive({
      kind: "welcome",
      from: "main",
      listings: [{ scope: SERVER, listing: listing("/srv", ["old"]) }],
    });
    expect(second.cache.get(SERVER, "/srv")?.entries[0].name).toBe("new");
  });

  it("never tells another window a vault's decrypted listings", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    main.store(VAULT, listing("/vault", ["diary.txt"]));
    main.forget(VAULT, "/vault");
    main.forgetAll(VAULT);
    main.closed(VAULT, true);
    const third = windows.open("window-3");
    const about = windows.sent.filter(({ message }) => JSON.stringify(message).includes("vault"));
    expect(about).toEqual([]);
    expect(second.cache.folderCount).toBe(0);
    expect(third.cache.folderCount).toBe(0);
    // And one sent all the same is not taken in.
    second.receive({ kind: "store", from: "main", scope: VAULT, listing: listing("/vault") });
    expect(second.cache.folderCount).toBe(0);
  });

  it("drops a vault's listings in its own window the moment it closes", () => {
    const windows = new Windows();
    const main = windows.open("main");
    main.store(VAULT, listing("/vault", ["diary.txt"]));
    main.closed(VAULT, true);
    expect(main.cache.folderCount).toBe(0);
    expect(windows.timers).toEqual([]);
  });

  it("forgets a changed folder in every window, and has the others reload it", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    for (const path of ["/srv", "/srv/old", "/srv/old/deep", "/srv/kept"]) {
      main.store(SERVER, listing(path, ["x"]));
    }
    main.forget(SERVER, "/srv", ["/srv/old"]);
    for (const share of [main, second]) {
      expect(share.cache.has(SERVER, "/srv")).toBe(false);
      expect(share.cache.has(SERVER, "/srv/old")).toBe(false);
      expect(share.cache.has(SERVER, "/srv/old/deep")).toBe(false);
      expect(share.cache.has(SERVER, "/srv/kept")).toBe(true);
    }
    expect(windows.reloads).toEqual([
      { window: "window-2", scope: SERVER, folder: "/srv", entries: ["/srv/old"] },
    ]);
  });

  it("forgets a whole connection in every window", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    main.store(SERVER, listing("/srv", ["x"]));
    second.forgetAll(SERVER);
    expect(main.cache.folderCount).toBe(0);
    expect(second.cache.folderCount).toBe(0);
  });

  it("drops an unsaved connection's listings when its last session closes", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    windows.use("main", SERVER);
    main.store(SERVER, listing("/srv", ["x"]));
    windows.leave("main", SERVER);
    main.closed(SERVER, false);
    expect(main.cache.folderCount).toBe(0);
    expect(second.cache.folderCount).toBe(0);
  });

  it("keeps them while another tab or window is still on the connection", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    windows.use("main", SERVER);
    windows.use("window-2", SERVER);
    main.store(SERVER, listing("/srv", ["x"]));
    // Another session of this window is still on it.
    main.closed(SERVER, false);
    expect(main.cache.has(SERVER, "/srv")).toBe(true);
    // The last one here closed, but the other window has one.
    windows.leave("main", SERVER);
    main.closed(SERVER, false);
    expect(main.cache.has(SERVER, "/srv")).toBe(false);
    expect(second.cache.has(SERVER, "/srv")).toBe(true);
    // What the other window lists from here on is remembered here again, until it closes too.
    second.store(SERVER, listing("/var", ["log"]));
    expect(main.cache.has(SERVER, "/var")).toBe(true);
    windows.leave("window-2", SERVER);
    second.closed(SERVER, false);
    expect(main.cache.folderCount).toBe(0);
    expect(second.cache.folderCount).toBe(0);
  });

  it("keeps a saved connection's listings for a while, in case it is opened again", () => {
    const windows = new Windows();
    const main = windows.open("main");
    main.store(SERVER, listing("/srv", ["x"]));
    main.closed(SERVER, true);
    expect(main.cache.has(SERVER, "/srv")).toBe(true);
    expect(windows.timers.map((timer) => timer.millis)).toEqual([SAVED_RETENTION_MILLIS]);
    windows.elapse();
    expect(main.cache.has(SERVER, "/srv")).toBe(false);
  });

  it("keeps them for good once the connection is in use again", () => {
    const windows = new Windows();
    const main = windows.open("main");
    main.store(SERVER, listing("/srv", ["x"]));
    main.closed(SERVER, true);
    // Reconnected: the first folder listed says so.
    windows.use("main", SERVER);
    main.store(SERVER, listing("/srv/www", ["y"]));
    expect(windows.timers).toEqual([]);
    expect(main.cache.has(SERVER, "/srv")).toBe(true);

    // Or reconnected without a folder listed yet when the time is up.
    windows.leave("main", SERVER);
    main.closed(SERVER, true);
    windows.use("main", SERVER);
    windows.elapse();
    expect(main.cache.has(SERVER, "/srv")).toBe(true);
  });

  it("stops telling a window that closed, and drops what only that window was on", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    windows.use("window-2", SERVER);
    second.store(SERVER, listing("/srv", ["x"]));
    expect(main.audience).toEqual(["window-2"]);
    windows.shares.delete("window-2");
    main.prune(["main"]);
    expect(main.audience).toEqual([]);
    expect(main.cache.folderCount).toBe(0);
    const before = windows.sent.length;
    main.store(SERVER, listing("/srv", ["x"]));
    expect(windows.sent.length).toBe(before);
  });

  it("keeps what a closed window was on when a session here is on it too", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    windows.use("main", SERVER);
    second.store(SERVER, listing("/srv", ["x"]));
    windows.shares.delete("window-2");
    main.prune(["main"]);
    expect(main.cache.has(SERVER, "/srv")).toBe(true);
  });

  it("lists a very large folder anew in each window rather than sending it", () => {
    const windows = new Windows();
    const main = windows.open("main");
    const second = windows.open("window-2");
    const names = Array.from({ length: SHARED_LISTING_ENTRIES + 1 }, (_, index) => `f${index}`);
    expect(main.store(SERVER, listing("/big", names))).toBe(true);
    expect(main.cache.has(SERVER, "/big")).toBe(true);
    expect(second.cache.has(SERVER, "/big")).toBe(false);
    const third = windows.open("window-3");
    expect(third.cache.has(SERVER, "/big")).toBe(false);
  });
});

describe("parseListingMessage", () => {
  it("takes in every kind of message", () => {
    const messages: ListingMessage[] = [
      { kind: "hello", from: "a" },
      { kind: "welcome", from: "a", listings: [{ scope: SERVER, listing: listing("/s", ["x"]) }] },
      { kind: "store", from: "a", scope: SERVER, listing: listing("/s", ["x"]) },
      { kind: "forget", from: "a", scope: SERVER, folder: "/s", entries: ["/s/x"] },
      { kind: "forgetAll", from: "a", scope: SERVER },
      { kind: "closed", from: "a", scope: SERVER, saved: true },
    ];
    for (const message of messages) expect(parseListingMessage(message)).toEqual(message);
  });

  it("refuses anything that is not one", () => {
    const entry = file("/s/x");
    const refused: unknown[] = [
      null,
      "hello",
      { kind: "hello" },
      { kind: "store", from: "a", scope: SERVER },
      { kind: "store", from: "a", scope: 5, listing: listing("/s") },
      { kind: "store", from: "a", scope: SERVER, listing: { path: "/s", entries: [] } },
      {
        kind: "store",
        from: "a",
        scope: SERVER,
        listing: { path: "/s", parent: null, entries: [{ ...entry, kind: "socket" }] },
      },
      {
        kind: "store",
        from: "a",
        scope: SERVER,
        listing: { path: "/s", parent: null, entries: [{ ...entry, size: "big" }] },
      },
      { kind: "welcome", from: "a", listings: [{ scope: SERVER }] },
      { kind: "forget", from: "a", scope: SERVER, folder: "/s", entries: [5] },
      { kind: "forget", from: "a", scope: SERVER },
      { kind: "forgetAll", from: "a" },
      { kind: "wipe", from: "a", scope: SERVER },
    ];
    for (const value of refused) expect(parseListingMessage(value)).toBeNull();
  });
});
