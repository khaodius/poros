import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { remoteSource } from "../lib/fileSource";
import { LISTING_CACHE_LIMITS, listingScope } from "../lib/listingCache";
import { SAVED_RETENTION_MILLIS } from "../lib/listingShare";
import { DEFAULT_SETTINGS } from "../lib/settings";
import type { ConnectProfile, DirListing, FileEntry, SessionInfo } from "../lib/types";
import {
  forgetListing,
  forgetListingOf,
  forgetListingsWithin,
  forgetServerListings,
  listingShare,
  paneListings,
  startSharingListings,
} from "./listings";
import { registerPane, type PaneHandle } from "./paneRegistry";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const cache = listingShare.cache;

const profile: ConnectProfile = {
  protocol: "sftp",
  host: "web.example.com",
  port: 22,
  username: "deploy",
  auth: { type: "agent" },
};

function server(id: string, change: Partial<SessionInfo> = {}): SessionInfo {
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

function vault(id: string, parentSessionId: string, root: string): SessionInfo {
  return {
    ...server(id),
    protocol: "cryptomator",
    vault: { parentSessionId, root, name: root.slice(root.lastIndexOf("/") + 1) },
  };
}

function know(...infos: SessionInfo[]) {
  useSessionStore.setState({
    sessions: Object.fromEntries(
      infos.map((info) => [
        info.id,
        { info, profile: info.vault ? null : profile, status: "connected" as const },
      ]),
    ),
  });
}

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

function listing(path: string, names: string[] = ["x"]): DirListing {
  return { path, parent: "/", entries: names.map((name) => file(`${path}/${name}`)) };
}

const SERVER = listingScope(server("s1"), profile);
const VAULT = listingScope(vault("v1", "s1", "/srv/vault"), null);

function remember(scope: string, ...paths: string[]) {
  for (const path of paths) cache.put(scope, listing(path));
}

function setCachedListings(cachedListings: boolean) {
  useSettingsStore.setState({
    settings: {
      ...DEFAULT_SETTINGS,
      interface: { ...DEFAULT_SETTINGS.interface, cachedListings },
    },
  });
}

const cleanups: (() => void)[] = [];

/** A pane of this window on a session, which notes when it is told to reload. */
function pane(tabId: string, sessionId: string, path: string | null, loading = false) {
  const handle: PaneHandle & { reloads: number } = {
    tabId,
    kind: "remote",
    sessionId,
    label: tabId,
    path: () => path,
    refresh: () => {
      handle.reloads += 1;
    },
    loading: () => loading,
    selected: () => [],
    visible: true,
    activatedAt: 0,
    reloads: 0,
  };
  cleanups.push(registerPane(handle));
  return handle;
}

beforeEach(() => {
  vi.useFakeTimers();
  invoke.mockReset();
  invoke.mockResolvedValue(undefined);
  cache.clear();
  useSessionStore.setState({ sessions: {} });
  setCachedListings(true);
});

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
});

describe("what Poros changes", () => {
  it("forgets the changed folder, and the folders changed in it with all inside", () => {
    know(server("s1"));
    remember(SERVER, "/srv", "/srv/old", "/srv/old/deep", "/srv/kept", "/var");
    forgetListing("s1", "/srv", ["/srv/old"]);
    expect(cache.has(SERVER, "/srv")).toBe(false);
    expect(cache.has(SERVER, "/srv/old")).toBe(false);
    expect(cache.has(SERVER, "/srv/old/deep")).toBe(false);
    expect(cache.has(SERVER, "/srv/kept")).toBe(true);
    expect(cache.has(SERVER, "/var")).toBe(true);
  });

  it("forgets through any session on the connection, and not through another account", () => {
    know(server("s1"), server("s2"), server("root", { username: "root" }));
    remember(SERVER, "/srv");
    forgetListing("root", "/srv");
    forgetListing("closed-long-ago", "/srv");
    expect(cache.has(SERVER, "/srv")).toBe(true);
    forgetListing("s2", "/srv");
    expect(cache.has(SERVER, "/srv")).toBe(false);
  });

  it("forgets the folder a saved file is in, and a whole tree after a synchronization", () => {
    know(server("s1"));
    remember(SERVER, "/srv", "/srv/www", "/srv/www/css", "/var");
    forgetListingOf("s1", "/srv/www/index.html");
    expect(cache.has(SERVER, "/srv/www")).toBe(false);
    expect(cache.has(SERVER, "/srv/www/css")).toBe(true);
    forgetListingsWithin("s1", "/srv");
    expect(cache.folderCount).toBe(1);
    expect(cache.has(SERVER, "/var")).toBe(true);
  });

  it("forgets the encrypted files of a vault that something changed in", () => {
    know(server("s1"), vault("v1", "s1", "/srv/vault"));
    remember(SERVER, "/srv/vault", "/srv/vault/d", "/srv/vault/d/AB", "/srv");
    remember(VAULT, "/srv/vault", "/srv/vault/letters");
    forgetListing("v1", "/srv/vault/letters");
    expect(cache.has(VAULT, "/srv/vault/letters")).toBe(false);
    expect(cache.has(VAULT, "/srv/vault")).toBe(true);
    // The server's own listing of the vault's folder still holds what marks it a vault.
    expect(cache.has(SERVER, "/srv/vault")).toBe(true);
    expect(cache.has(SERVER, "/srv/vault/d")).toBe(false);
    expect(cache.has(SERVER, "/srv/vault/d/AB")).toBe(false);
    expect(cache.has(SERVER, "/srv")).toBe(true);
  });

  it("forgets everything on a server after a command ran there, its vaults included", () => {
    know(server("s1"), vault("v1", "s1", "/srv/vault"), server("root", { username: "root" }));
    const other = listingScope(server("root", { username: "root" }), profile);
    remember(SERVER, "/srv", "/var");
    remember(VAULT, "/srv/vault");
    remember(other, "/root");
    forgetServerListings("v1");
    expect(cache.snapshot(() => true).map((kept) => kept.listing.path)).toEqual(["/root"]);
  });

  it("forgets what a pane's source creates, renames or deletes", async () => {
    know(server("s1"));
    const source = remoteSource(server("s1"), profile);
    remember(SERVER, "/srv", "/srv/old", "/srv/old/deep", "/var");
    invoke.mockResolvedValue("/srv/new");
    await source.mkdir("/srv", "new");
    expect(cache.has(SERVER, "/srv")).toBe(false);
    expect(cache.has(SERVER, "/srv/old")).toBe(true);

    remember(SERVER, "/srv");
    invoke.mockResolvedValue("/srv/renamed");
    await source.rename("/srv/old", "renamed");
    expect(cache.has(SERVER, "/srv")).toBe(false);
    expect(cache.has(SERVER, "/srv/old")).toBe(false);
    expect(cache.has(SERVER, "/srv/old/deep")).toBe(false);

    remember(SERVER, "/var", "/var/log");
    invoke.mockRejectedValue({ kind: "permissionDenied", message: "no" });
    // Whatever went before a delete failed is gone all the same.
    await expect(source.remove(["/var/log"])).rejects.toMatchObject({ kind: "permissionDenied" });
    expect(cache.has(SERVER, "/var")).toBe(false);
    expect(cache.has(SERVER, "/var/log")).toBe(false);
  });
});

describe("panes left showing a folder that changed", () => {
  it("reload through one of them when nobody else reloads the folder", () => {
    know(server("s1"), server("s2"));
    const first = pane("a", "s1", "/srv");
    const second = pane("b", "s2", "/srv");
    const elsewhere = pane("c", "s1", "/var");
    forgetListing("s1", "/srv");
    vi.advanceTimersByTime(1000);
    // One asks the server; the other takes the listing that brings.
    expect(first.reloads + second.reloads).toBe(1);
    expect(elsewhere.reloads).toBe(0);
  });

  it("are left alone when the folder is being reloaded, or was already", () => {
    know(server("s1"), server("s2"));
    const idle = pane("a", "s1", "/srv");
    pane("b", "s2", "/srv", true);
    forgetListing("s1", "/srv");
    vi.advanceTimersByTime(1000);
    expect(idle.reloads).toBe(0);

    const later = pane("c", "s1", "/var");
    forgetListing("s1", "/var");
    remember(SERVER, "/var");
    vi.advanceTimersByTime(1000);
    expect(later.reloads).toBe(0);
  });

  it("are left alone when a folder too large to remember was listed since", () => {
    know(server("s1"));
    const huge = pane("a", "s1", "/srv/huge");
    forgetListing("s1", "/srv/huge");
    const names = Array.from(
      { length: LISTING_CACHE_LIMITS.entries + 1 },
      (_, index) => `${index}`,
    );
    expect(cache.put(SERVER, listing("/srv/huge", names))).toBe(false);
    vi.advanceTimersByTime(1000);
    expect(huge.reloads).toBe(0);
  });

  it("reload when they are inside a folder that was moved or deleted", () => {
    know(server("s1"));
    const inside = pane("a", "s1", "/srv/old/deep");
    const beside = pane("b", "s1", "/srv/older");
    forgetListing("s1", "/srv", ["/srv/old"]);
    vi.advanceTimersByTime(1000);
    expect(inside.reloads).toBe(1);
    expect(beside.reloads).toBe(0);
  });

  it("are left alone on another account, and with nothing on show", () => {
    know(server("s1"), server("root", { username: "root" }));
    const other = pane("a", "root", "/srv");
    const empty = pane("b", "s1", null);
    forgetListing("s1", "/srv");
    vi.advanceTimersByTime(1000);
    expect(other.reloads).toBe(0);
    expect(empty.reloads).toBe(0);
  });
});

describe("sessions that close", () => {
  it("drops a vault's decrypted listings the moment it locks", async () => {
    know(server("s1"), vault("v1", "s1", "/srv/vault"));
    remember(SERVER, "/srv", "/srv/vault");
    remember(VAULT, "/srv/vault", "/srv/vault/letters");
    await useSessionStore.getState().disconnect("v1");
    expect(cache.has(VAULT, "/srv/vault")).toBe(false);
    expect(cache.has(VAULT, "/srv/vault/letters")).toBe(false);
    expect(cache.has(SERVER, "/srv/vault")).toBe(true);
    expect(cache.has(SERVER, "/srv")).toBe(true);
  });

  it("drops an unsaved connection's listings with its last session, vaults included", async () => {
    know(server("s1"), server("s2"), vault("v1", "s1", "/srv/vault"));
    remember(SERVER, "/srv");
    remember(VAULT, "/srv/vault");
    await useSessionStore.getState().disconnect("s1");
    // Another tab is still on the server; the vault closed with its own.
    expect(cache.has(SERVER, "/srv")).toBe(true);
    expect(cache.has(VAULT, "/srv/vault")).toBe(false);
    await useSessionStore.getState().disconnect("s2");
    expect(cache.folderCount).toBe(0);
  });

  it("keeps a saved connection's listings for a while after it closes", async () => {
    know(server("s1", { savedConnectionId: "saved-1" }));
    remember(SERVER, "/srv");
    await useSessionStore.getState().disconnect("s1");
    expect(cache.has(SERVER, "/srv")).toBe(true);
    vi.advanceTimersByTime(SAVED_RETENTION_MILLIS - 1000);
    expect(cache.has(SERVER, "/srv")).toBe(true);
    vi.advanceTimersByTime(2000);
    expect(cache.has(SERVER, "/srv")).toBe(false);
  });

  it("keeps them when the connection is opened again in time", async () => {
    know(server("s1", { savedConnectionId: "saved-1" }));
    remember(SERVER, "/srv");
    await useSessionStore.getState().disconnect("s1");
    know(server("s9", { savedConnectionId: "saved-1" }));
    vi.advanceTimersByTime(SAVED_RETENTION_MILLIS * 2);
    expect(cache.has(SERVER, "/srv")).toBe(true);
  });

  it("keeps a server's listings when its session moves to another window", () => {
    know(server("s1"), vault("v1", "s1", "/srv/vault"));
    remember(SERVER, "/srv");
    remember(VAULT, "/srv/vault");
    useSessionStore.getState().release("s1");
    expect(cache.has(SERVER, "/srv")).toBe(true);
    // The vault's go: this window has no session left that may show them.
    expect(cache.has(VAULT, "/srv/vault")).toBe(false);
  });

  it("keeps them over a reconnect, which opens another session on the connection", async () => {
    know(server("s1"));
    remember(SERVER, "/srv");
    invoke.mockImplementation(async (command: string) =>
      command === "reconnect" ? server("s2") : undefined,
    );
    await useSessionStore.getState().reconnect("s1");
    expect(Object.keys(useSessionStore.getState().sessions)).toEqual(["s2"]);
    expect(cache.has(SERVER, "/srv")).toBe(true);
  });
});

describe("the setting", () => {
  const source = remoteSource(server("setting"), profile);

  it("remembers what the panes are told, and gives it back", () => {
    const answer = paneListings.asking(source, "/srv", "pane");
    expect(paneListings.recall(source, "/srv")).toBeNull();
    answer(listing("/srv"));
    expect(paneListings.recall(source, "/srv")?.path).toBe("/srv");
    paneListings.forget(source, "/srv");
    expect(paneListings.recall(source, "/srv")).toBeNull();
  });

  it("remembers nothing while it is off", () => {
    setCachedListings(false);
    paneListings.asking(source, "/srv", "pane")(listing("/srv"));
    expect(cache.folderCount).toBe(0);
    remember(SERVER, "/srv");
    expect(paneListings.recall(source, "/srv")).toBeNull();
  });

  it("forgets everything when it is turned off", () => {
    const stop = startSharingListings();
    remember(SERVER, "/srv", "/var");
    setCachedListings(false);
    expect(cache.folderCount).toBe(0);
    stop();
  });
});
