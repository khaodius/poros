import { beforeEach, describe, expect, it, vi } from "vitest";
import { listingScope } from "../lib/listingCache";
import { DEFAULT_SETTINGS } from "../lib/settings";
import { EMPTY_SNAPSHOT } from "../lib/transfers";
import type { ConnectProfile, DirListing, FileEntry, SessionInfo } from "../lib/types";
import { listingShare } from "./listings";
import { prefetchAround } from "./listingPrefetch";
import { useSessionStore } from "./sessionStore";
import { useSettingsStore } from "./settingsStore";
import { useTransferStore } from "./transferStore";

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

/** A folder whose entries are folders, so the one under the cursor is worth listing ahead. */
function folder(path: string, names: string[] = []): DirListing {
  const parent = path === "/" ? null : path.slice(0, path.lastIndexOf("/")) || "/";
  const entries = names.map((name): FileEntry => ({
    name,
    path: `${path}/${name}`,
    kind: "dir",
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  }));
  return { path, parent, entries };
}

interface Asked {
  path: string;
  resolve: (listing: DirListing) => void;
}

/** Every folder the server was asked for, answered when the test says so. */
let asked: Asked[] = [];

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function answer(index: number, listing: DirListing) {
  asked[index].resolve(listing);
  await settle();
}

beforeEach(() => {
  asked = [];
  invoke.mockReset();
  invoke.mockImplementation((command: string, args?: { path?: string }) => {
    if (command !== "remote_list") return Promise.resolve(undefined);
    return new Promise((resolve) => asked.push({ path: args?.path ?? "", resolve }));
  });
  cache.clear();
  useSettingsStore.setState({ settings: DEFAULT_SETTINGS });
  useTransferStore.setState({ snapshot: EMPTY_SNAPSHOT });
});

// Each test uses servers of its own: a server's folders are listed one at a time, for the
// whole run of the tests.
describe("prefetchAround", () => {
  it("lists the folder under the cursor, then the parent, and remembers both", async () => {
    know(server("s1"));
    const scope = listingScope(server("s1"), profile);
    prefetchAround("s1", folder("/srv/www", ["css"]), "/srv/www/css");
    expect(asked.map((request) => request.path)).toEqual(["/srv/www/css"]);
    await answer(0, folder("/srv/www/css"));
    expect(asked.map((request) => request.path)).toEqual(["/srv/www/css", "/srv"]);
    await answer(1, folder("/srv", ["www"]));
    expect(cache.has(scope, "/srv/www/css")).toBe(true);
    expect(cache.has(scope, "/srv")).toBe(true);
  });

  it("asks nothing for a vault that locked while its folders waited their turn", async () => {
    know(server("s2"), vault("v2", "s2", "/srv/vault"));
    prefetchAround("v2", folder("/srv/vault/letters", ["2026"]), "/srv/vault/letters/2026");
    // The parent waits behind the folder under the cursor, on the server's connection.
    expect(asked.map((request) => request.path)).toEqual(["/srv/vault/letters/2026"]);
    await useSessionStore.getState().disconnect("v2");
    await answer(0, folder("/srv/vault/letters/2026", ["tax-return.pdf"]));
    expect(asked).toHaveLength(1);
    expect(cache.folderCount).toBe(0);
  });

  it("asks nothing more once remembered listings are turned off", async () => {
    know(server("s3"));
    prefetchAround("s3", folder("/srv/www", ["css"]), "/srv/www/css");
    useSettingsStore.setState({
      settings: {
        ...DEFAULT_SETTINGS,
        interface: { ...DEFAULT_SETTINGS.interface, cachedListings: false },
      },
    });
    cache.clear();
    await answer(0, folder("/srv/www/css"));
    expect(asked).toHaveLength(1);
    expect(cache.folderCount).toBe(0);
  });

  it("waits on FTP while transfers run, and not on SFTP", () => {
    const counts = { ...EMPTY_SNAPSHOT.stats.counts, running: 1 };
    useTransferStore.setState({
      snapshot: { ...EMPTY_SNAPSHOT, stats: { ...EMPTY_SNAPSHOT.stats, counts } },
    });
    know(server("ftp-1", { protocol: "ftp" }), server("sftp-1"));
    prefetchAround("ftp-1", folder("/srv/www", ["css"]), "/srv/www/css");
    expect(asked).toHaveLength(0);
    prefetchAround("sftp-1", folder("/srv/www", ["css"]), "/srv/www/css");
    expect(asked).toHaveLength(1);
  });
});
