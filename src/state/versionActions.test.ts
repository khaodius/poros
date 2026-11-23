// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listingScope } from "../lib/listingCache";
import type { DirListing, FileEntry, ObjectVersion, SessionInfo } from "../lib/types";
import { listingShare } from "./listings";
import { useSessionStore } from "./sessionStore";
import { deleteVersion, resolveDeleted, restoreVersion } from "./versionActions";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const info: SessionInfo = {
  id: "s1",
  label: "s3.example.com",
  host: "s3.example.com",
  port: 443,
  username: "key",
  home: "/",
  initialPath: "/bucket",
  protocol: "s3",
};
const scope = listingScope(info, null);
const cache = listingShare.cache;

function remember(path: string, names: string[]): void {
  const entries = names.map((name): FileEntry => ({
    name,
    path: `${path}/${name}`,
    kind: "file",
    size: 1,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  }));
  const listing: DirListing = { path, parent: "/", entries };
  listingShare.store(scope, listing, cache.begin(scope, path));
}

const version: ObjectVersion = {
  versionId: "v1",
  kind: "version",
  latest: false,
  modified: 1_700_000_000,
  size: 4,
};
const file = { sessionId: "s1", path: "/bucket/reports/a.txt", name: "a.txt" };

beforeEach(() => {
  cache.clear();
  useSessionStore.setState({ sessions: { s1: { info, profile: null, status: "connected" } } });
  invoke.mockReset();
  invoke.mockImplementation(async (command: string) =>
    command === "versions_deleted" ? { files: 3, failed: 0, failures: [] } : null,
  );
  remember("/bucket/reports", ["a.txt"]);
  remember("/bucket/reports/old", ["kept.txt"]);
  remember("/bucket/other", ["b.txt"]);
});

describe("what changes a bucket", () => {
  it("forgets the listing of the folder a version was restored or removed in", async () => {
    await restoreVersion(file, version, "seconds");
    expect(cache.has(scope, "/bucket/reports")).toBe(false);
    expect(cache.has(scope, "/bucket/other")).toBe(true);

    remember("/bucket/reports", ["a.txt"]);
    await deleteVersion(file, version, "seconds");
    expect(cache.has(scope, "/bucket/reports")).toBe(false);
  });

  it("forgets the listings inside deleted folders it brings back or removes", async () => {
    const deleted: FileEntry = {
      name: "old",
      path: "/bucket/reports/old",
      kind: "dir",
      size: 0,
      modified: null,
      permissions: null,
      owner: null,
      group: null,
      hidden: false,
    };
    await resolveDeleted("restore", {
      sessionId: "s1",
      folder: "/bucket/reports",
      entries: [deleted],
    });
    expect(invoke).toHaveBeenCalledWith(
      "versions_deleted",
      expect.objectContaining({
        request: expect.objectContaining({ action: "restore" }),
      }),
    );
    expect(cache.has(scope, "/bucket/reports")).toBe(false);
    expect(cache.has(scope, "/bucket/reports/old")).toBe(false);
    expect(cache.has(scope, "/bucket/other")).toBe(true);
  });
});
