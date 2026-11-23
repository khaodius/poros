import { describe, expect, it } from "vitest";
import { PrefetchLanes, mayPrefetch, prefetchTargets, type PrefetchJob } from "./listingPrefetch";
import type { DirListing, FileEntry } from "./types";

const ON = { cachedListings: true, prefetchListings: true, prefetchOnCloud: false };

function entry(path: string, kind: FileEntry["kind"], linkTarget?: FileEntry["linkTarget"]) {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind,
    linkTarget,
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  } satisfies FileEntry;
}

const listing: DirListing = {
  path: "/srv/www",
  parent: "/srv",
  entries: [
    entry("/srv/www/css", "dir"),
    entry("/srv/www/index.html", "file"),
    entry("/srv/www/current", "symlink", "dir"),
  ],
};

describe("mayPrefetch", () => {
  it("follows the two switches", () => {
    expect(mayPrefetch("sftp", ON, false)).toBe(true);
    expect(mayPrefetch("sftp", { ...ON, prefetchListings: false }, false)).toBe(false);
    expect(mayPrefetch("sftp", { ...ON, cachedListings: false }, false)).toBe(false);
  });

  it("stays off on cloud storage until asked for there", () => {
    for (const protocol of ["s3", "googleDrive", "oneDrive", "backblazeB2"] as const) {
      expect(mayPrefetch(protocol, ON, false)).toBe(false);
      expect(mayPrefetch(protocol, { ...ON, prefetchOnCloud: true }, false)).toBe(true);
    }
  });

  it("waits on FTP while transfers run, where listings share their connection", () => {
    expect(mayPrefetch("ftp", ON, true)).toBe(false);
    expect(mayPrefetch("ftps", ON, true)).toBe(false);
    expect(mayPrefetch("ftp", ON, false)).toBe(true);
    expect(mayPrefetch("sftp", ON, true)).toBe(true);
  });
});

describe("prefetchTargets", () => {
  it("picks the folder under the cursor, then the parent", () => {
    expect(prefetchTargets(listing, "/srv/www/css")).toEqual(["/srv/www/css", "/srv"]);
    expect(prefetchTargets(listing, "/srv/www/current")).toEqual(["/srv/www/current", "/srv"]);
  });

  it("picks only the parent when the cursor is on a file or nowhere", () => {
    expect(prefetchTargets(listing, "/srv/www/index.html")).toEqual(["/srv"]);
    expect(prefetchTargets(listing, null)).toEqual(["/srv"]);
  });

  it("picks nothing at the top with the cursor on no folder", () => {
    expect(prefetchTargets({ path: "/", parent: null, entries: [] }, null)).toEqual([]);
  });
});

/** A job that runs until `finish` is called, and says when it started. */
function job(key: string, started: string[]): PrefetchJob & { finish: () => void } {
  let finish = () => {};
  const done = new Promise<void>((resolve) => {
    finish = resolve;
  });
  return {
    key,
    run: () => {
      started.push(key);
      return done;
    },
    finish: () => finish(),
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("PrefetchLanes", () => {
  it("runs one job per connection at a time", async () => {
    const lanes = new PrefetchLanes();
    const started: string[] = [];
    const first = job("a", started);
    const second = job("b", started);
    lanes.want("server", [first, second]);
    expect(started).toEqual(["a"]);
    expect(lanes.isBusy("server")).toBe(true);
    first.finish();
    await settle();
    expect(started).toEqual(["a", "b"]);
    second.finish();
    await settle();
    expect(lanes.isBusy("server")).toBe(false);
  });

  it("runs the jobs of different connections side by side", () => {
    const lanes = new PrefetchLanes();
    const started: string[] = [];
    lanes.want("one", [job("a", started)]);
    lanes.want("two", [job("b", started)]);
    expect(started).toEqual(["a", "b"]);
  });

  it("drops what waits for what is wanted next, as the cursor has moved on", async () => {
    const lanes = new PrefetchLanes();
    const started: string[] = [];
    const running = job("a", started);
    lanes.want("server", [running, job("b", started)]);
    const wantedNow = job("c", started);
    lanes.want("server", [wantedNow]);
    running.finish();
    await settle();
    expect(started).toEqual(["a", "c"]);
    wantedNow.finish();
    await settle();
    expect(started).toEqual(["a", "c"]);
  });

  it("does not ask twice for the folder being listed", async () => {
    const lanes = new PrefetchLanes();
    const started: string[] = [];
    const running = job("a", started);
    lanes.want("server", [running]);
    lanes.want("server", [job("a", started)]);
    running.finish();
    await settle();
    expect(started).toEqual(["a"]);
  });

  it("goes on after a job that failed, and drops what waits when cleared", async () => {
    const lanes = new PrefetchLanes();
    const started: string[] = [];
    const failing: PrefetchJob = { key: "a", run: () => Promise.reject(new Error("refused")) };
    lanes.want("server", [failing, job("b", started)]);
    await settle();
    expect(started).toEqual(["b"]);

    const waiting = job("d", started);
    lanes.want("other", [job("c", started), waiting]);
    lanes.clear("other");
    expect(started).toEqual(["b", "c"]);
  });
});
