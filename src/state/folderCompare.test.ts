import { describe, expect, it, vi } from "vitest";
import type { CompareEntry } from "../lib/folderCompare";
import { DEFAULT_SETTINGS, type FolderCompareSettings } from "../lib/settings";
import type {
  DirListing,
  FileEntry,
  FolderCompareRequest,
  FolderComparison,
  FolderSummary,
  SessionInfo,
} from "../lib/types";
import {
  comparedListing,
  createFolderCompare,
  isCompareMessage,
  moveBetween,
  paneKey,
  snapshotOf,
  type BrowseMove,
  type ComparablePane,
  type CompareMessage,
  type FolderCompare,
  type PaneSnapshot,
  type ShownFolder,
} from "./folderCompare";

function file(name: string, size: number, modified: number): CompareEntry {
  return { name, shape: "file", size, modified, hidden: false };
}

function folder(name: string): CompareEntry {
  return { name, shape: "folder", size: 0, modified: 0, hidden: false };
}

function listed(
  path: string,
  entries: CompareEntry[],
  more: Partial<PaneSnapshot> = {},
): PaneSnapshot {
  const parent = path.slice(0, path.lastIndexOf("/")) || "/";
  return {
    location: { kind: "local" },
    pathStyle: "posix",
    ignoresCase: false,
    coarseTimes: false,
    outdated: false,
    showingResults: false,
    path,
    parent: path === "/" ? null : parent,
    entries,
    ...more,
  };
}

function pane(tabId: string, more: Partial<ComparablePane> = {}): ComparablePane {
  return { tabId, label: tabId, visible: true, activatedAt: 1, ready: true, ...more };
}

interface TestWindow {
  compare: FolderCompare;
  panes: ComparablePane[];
  options: FolderCompareSettings;
  sync: { compare: "sizeAndTime" | "sizeOnly"; timeToleranceSecs: number };
  requests: FolderCompareRequest[];
  cancelled: string[];
  /** Settles the deep comparison under way. */
  finish: (found: FolderComparison) => void;
  fail: (error: unknown) => void;
}

/** Windows whose messages reach each other a moment later and as JSON, as Tauri events do. */
function openWindows(...labels: string[]) {
  const opened = new Map<string, TestWindow>();
  const sent: CompareMessage[] = [];
  let ids = 0;
  for (const label of labels) {
    const testWindow: TestWindow = {
      compare: undefined as unknown as FolderCompare,
      panes: [],
      options: { ...DEFAULT_SETTINGS.interface.folderCompare },
      sync: { compare: "sizeAndTime", timeToleranceSecs: 2 },
      requests: [],
      cancelled: [],
      finish: () => undefined,
      fail: () => undefined,
    };
    testWindow.compare = createFolderCompare({
      window: label,
      send: (to, message) => {
        sent.push(message);
        const wire = JSON.stringify(message);
        for (const [other, target] of opened) {
          if (other === label || (to !== null && to !== other)) continue;
          queueMicrotask(() => target.compare.receive(JSON.parse(wire)));
        }
      },
      otherWindows: async () => [...opened.keys()].filter((other) => other !== label),
      panes: () => testWindow.panes,
      options: () => testWindow.options,
      syncDefaults: () => testWindow.sync,
      compareFolders: (request) => {
        testWindow.requests.push(request);
        return new Promise((resolve, reject) => {
          testWindow.finish = resolve;
          testWindow.fail = reject;
        });
      },
      cancel: async (requestId) => {
        testWindow.cancelled.push(requestId);
      },
      newId: () => `id-${(ids += 1)}`,
      seekMillis: 50,
    });
    opened.set(label, testWindow);
  }
  return { opened, sent, close: (label: string) => opened.delete(label) };
}

/** Lets messages between windows arrive, and what they set off in turn. */
async function settle() {
  for (let round = 0; round < 6; round += 1) await Promise.resolve();
}

function oneWindow(...panes: ComparablePane[]) {
  const { opened } = openWindows("main");
  const main = opened.get("main") as TestWindow;
  main.panes = panes;
  return main;
}

const HERE = [
  folder("docs"),
  file("same.txt", 10, 100),
  file("edited.txt", 12, 500),
  file("new.txt", 1, 1),
];
const THERE = [folder("docs"), file("same.txt", 10, 101), file("edited.txt", 10, 100)];

async function comparedPair() {
  const main = oneWindow(pane("left"), pane("right"));
  await main.compare.toggle("left");
  main.compare.publish("left", listed("/home/me/site", HERE));
  main.compare.publish("right", listed("/var/www", THERE));
  const pairId = main.compare.store.getState().pairOf.left;
  return { main, pairId, state: () => main.compare.store.getState() };
}

const states = (marks: ReadonlyMap<string, { state: string }> | undefined) =>
  Object.fromEntries([...(marks ?? [])].map(([name, mark]) => [name, mark.state]));

describe("pairing two panes", () => {
  it("takes the other pane looked at last, one on screen before one in a hidden tab", async () => {
    const main = oneWindow(
      pane("self", { activatedAt: 50 }),
      pane("older", { activatedAt: 10 }),
      pane("newer", { activatedAt: 20 }),
      pane("hidden", { activatedAt: 40, visible: false }),
      pane("loading", { activatedAt: 45, ready: false }),
    );
    expect(await main.compare.toggle("self")).toBeNull();
    const { pairs, pairOf } = main.compare.store.getState();
    expect(pairs[pairOf.self]).toMatchObject({
      left: { window: "main", tabId: "self" },
      right: { window: "main", tabId: "newer" },
    });
    expect(pairOf.newer).toBe(pairOf.self);
  });

  it("falls back to a pane in a hidden tab, and says when there is none at all", async () => {
    const main = oneWindow(pane("self"), pane("hidden", { visible: false }));
    expect(await main.compare.toggle("self")).toBeNull();
    expect(main.compare.store.getState().pairOf.hidden).toBeDefined();

    const alone = oneWindow(pane("self"));
    expect(await alone.compare.toggle("self")).toMatch(/second folder pane/);
    expect(alone.compare.store.getState().pairs).toEqual({});
  });

  it("marks both panes once both show a folder", async () => {
    const main = oneWindow(pane("left"), pane("right"));
    await main.compare.toggle("left");
    const pairId = main.compare.store.getState().pairOf.left;
    expect(main.compare.store.getState().comparisons[pairId]).toBeNull();
    main.compare.publish("left", listed("/home/me/site", HERE));
    expect(main.compare.store.getState().comparisons[pairId]).toBeNull();
    main.compare.publish("right", listed("/var/www", THERE));
    const found = main.compare.store.getState().comparisons[pairId];
    expect(states(found?.left)).toEqual({
      docs: "both",
      "same.txt": "identical",
      "edited.txt": "newer",
      "new.txt": "onlyHere",
    });
    expect(states(found?.right)).toEqual({
      docs: "both",
      "same.txt": "identical",
      "edited.txt": "older",
    });
  });

  it("compares again when either pane shows something else, and not while one shows nothing", async () => {
    const { main, pairId, state } = await comparedPair();
    main.compare.publish("right", listed("/var/www", [...THERE, file("new.txt", 1, 1)]));
    expect(states(state().comparisons[pairId]?.left)["new.txt"]).toBe("identical");
    main.compare.publish("right", null);
    expect(state().comparisons[pairId]).toBeNull();
  });

  it("is turned off for both panes from either", async () => {
    const { main, state } = await comparedPair();
    expect(await main.compare.toggle("right")).toBeNull();
    expect(state()).toEqual({
      pairs: {},
      pairOf: {},
      snapshots: {},
      comparisons: {},
      deep: {},
      notes: {},
    });
  });

  it("ends when a compared pane goes away", async () => {
    const { main, state } = await comparedPair();
    main.compare.leave("right");
    expect(state().pairOf).toEqual({});
    expect(state().pairs).toEqual({});
  });

  it("moves a pane that is compared already to its new partner", async () => {
    const main = oneWindow(pane("one", { activatedAt: 3 }), pane("two", { activatedAt: 2 }));
    await main.compare.toggle("one");
    const first = main.compare.store.getState().pairOf.one;
    main.panes.push(pane("three", { activatedAt: 1 }));
    main.panes[1].activatedAt = 9;
    await main.compare.toggle("three");
    const { pairs, pairOf } = main.compare.store.getState();
    expect(pairs[first]).toBeUndefined();
    expect(pairOf.one).toBeUndefined();
    expect(pairOf.three).toBe(pairOf.two);
    expect(Object.keys(pairs)).toHaveLength(1);
  });

  it("follows the bar's options and the defaults in Settings > Sync", async () => {
    const { main, pairId, state } = await comparedPair();
    main.compare.rulesChanged();
    main.compare.publish("right", listed("/var/www", [file("edited.txt", 12, 100)]));
    expect(states(state().comparisons[pairId]?.left)["edited.txt"]).toBe("newer");
    main.options = { ...main.options, sizeRule: "sizeOnly" };
    main.compare.rulesChanged();
    expect(states(state().comparisons[pairId]?.left)["edited.txt"]).toBe("identical");
    main.options = { ...main.options, sizeRule: "sync" };
    main.compare.rulesChanged();
    expect(states(state().comparisons[pairId]?.left)["edited.txt"]).toBe("newer");
    main.sync = { compare: "sizeAndTime", timeToleranceSecs: 600 };
    main.compare.rulesChanged();
    expect(states(state().comparisons[pairId]?.left)["edited.txt"]).toBe("identical");
  });

  it("ignores case when either side does, and allows for a coarse listing's times", async () => {
    const main = oneWindow(pane("left"), pane("right"));
    await main.compare.toggle("left");
    const pairId = main.compare.store.getState().pairOf.left;
    main.compare.publish("left", listed("C:\\Site", [file("Index.HTML", 5, 1_700_000_075)]));
    main.compare.publish("right", listed("/www", [file("index.html", 5, 1_700_000_040)]));
    const found = () => states(main.compare.store.getState().comparisons[pairId]?.left);
    expect(found()).toEqual({ "Index.HTML": "onlyHere" });
    main.compare.publish(
      "left",
      listed("C:\\Site", [file("Index.HTML", 5, 1_700_000_075)], { ignoresCase: true }),
    );
    expect(found()).toEqual({ "Index.HTML": "newer" });
    main.compare.publish(
      "right",
      listed("/www", [file("index.html", 5, 1_700_000_040)], { coarseTimes: true }),
    );
    expect(found()).toEqual({ "Index.HTML": "identical" });
  });
});

describe("what a pane gives the comparison", () => {
  const info = (more: Partial<SessionInfo> = {}): SessionInfo => ({
    id: "session-1",
    label: "web.example.com",
    host: "web.example.com",
    port: 21,
    username: "me",
    home: "/",
    initialPath: "/",
    protocol: "ftp",
    ...more,
  });
  const row: FileEntry = {
    name: "index.html",
    path: "/www/index.html",
    kind: "file",
    size: 5,
    modified: 1_700_000_040,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
  const shown = (more: Partial<ShownFolder> = {}): ShownFolder => ({
    listing: { path: "/www", parent: "/", entries: [row] },
    location: { kind: "remote", sessionId: "session-1" },
    pathStyle: "posix",
    info: info(),
    lost: false,
    covered: false,
    outdated: false,
    showingResults: false,
    ...more,
  });

  it("takes what the backend says of the session", () => {
    expect(snapshotOf(shown({ info: info({ coarseTimes: true }) }))).toEqual({
      location: { kind: "remote", sessionId: "session-1" },
      pathStyle: "posix",
      protocol: "ftp",
      ignoresCase: false,
      coarseTimes: true,
      outdated: false,
      showingResults: false,
      path: "/www",
      parent: "/",
      entries: [
        { name: "index.html", shape: "file", size: 5, modified: 1_700_000_040, hidden: false },
      ],
    });
    // Times on whole minutes are not a sign of coarse ones: only the backend can tell.
    expect(snapshotOf(shown())?.coarseTimes).toBe(false);
    expect(snapshotOf(shown({ info: info({ protocol: "oneDrive" }) }))?.ignoresCase).toBe(true);
    const local = snapshotOf(shown({ location: { kind: "local" }, info: undefined }));
    expect(local).toMatchObject({ protocol: undefined, coarseTimes: false, ignoresCase: false });
  });

  it("has nothing to compare while the server is out of reach or something covers the rows", () => {
    expect(snapshotOf(shown({ lost: true }))).toBeNull();
    expect(snapshotOf(shown({ covered: true }))).toBeNull();
    expect(snapshotOf(shown({ listing: null }))).toBeNull();
  });

  it("compares a listing that could not be refreshed, and again once the folder is listed", async () => {
    expect(snapshotOf(shown({ outdated: true }))?.outdated).toBe(true);
    const { main, pairId, state } = await comparedPair();
    const right = paneKey(state().pairs[pairId].right);
    const remembered = [...THERE, file("gone.txt", 1, 1)];
    main.compare.publish("right", listed("/var/www", remembered, { outdated: true }));
    expect(state().snapshots[right]?.outdated).toBe(true);
    expect(states(state().comparisons[pairId]?.right)["gone.txt"]).toBe("onlyHere");
    // The folder listed again: what it holds now decides.
    main.compare.publish("right", listed("/var/www", THERE));
    expect(state().snapshots[right]?.outdated).toBe(false);
    expect(states(state().comparisons[pairId]?.right)).toEqual({
      docs: "both",
      "same.txt": "identical",
      "edited.txt": "older",
    });
  });

  it("compares the folder behind search results, and pauses while they are on show", async () => {
    const entry = (path: string, kind: FileEntry["kind"], size: number, modified: number) => ({
      ...row,
      name: path.slice(path.lastIndexOf("/") + 1),
      path,
      kind,
      size,
      modified,
    });
    const folder: DirListing = {
      path: "/var/www",
      parent: "/var",
      entries: [
        entry("/var/www/docs", "dir", 0, 0),
        entry("/var/www/same.txt", "file", 10, 101),
        entry("/var/www/edited.txt", "file", 10, 100),
      ],
    };
    // As a pane shows them: found in and below the folder, under its path and with no parent.
    const results: DirListing = {
      path: "/var/www",
      parent: null,
      entries: [entry("/var/www/docs/api/readme.md", "file", 3, 5)],
    };
    const right = (showingResults: boolean) =>
      snapshotOf(
        shown({
          listing: comparedListing({
            listing: showingResults ? results : folder,
            folder,
            showingResults,
          }),
          location: { kind: "local" },
          info: undefined,
          showingResults,
        }),
      );
    expect(right(true)).toEqual({ ...right(false), showingResults: true });
    expect(right(true)).toMatchObject({ path: "/var/www", parent: "/var" });
    expect(right(true)?.entries.map((item) => item.name)).toEqual([
      "docs",
      "same.txt",
      "edited.txt",
    ]);

    const { main, pairId, state } = await comparedPair();
    main.compare.publish("right", right(false));
    const started = main.compare.startDeep("left");
    main.finish({
      folders: [summary("docs", { identical: 1, newerLeft: 1 })],
      truncated: false,
      leftEntries: 3,
      rightEntries: 2,
    });
    await started;
    expect(state().comparisons[pairId]?.left.get("docs")?.state).toBe("differsInside");
    // Results open: neither pane is marked, and what was found below the folder is kept.
    main.compare.publish("right", right(true));
    expect(state().comparisons[pairId]).toBeNull();
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    void main.compare.startDeep("left");
    expect(main.requests).toHaveLength(1);
    // Results closed: the marks are back, the ones from below the folder too.
    main.compare.publish("right", right(false));
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    expect(states(state().comparisons[pairId]?.left)).toEqual({
      docs: "differsInside",
      "same.txt": "identical",
      "edited.txt": "newer",
      "new.txt": "onlyHere",
    });
  });

  it("takes the marks off both panes when a connection is lost", async () => {
    const { main, pairId, state } = await comparedPair();
    expect(state().comparisons[pairId]).not.toBeNull();
    main.compare.publish("right", snapshotOf(shown({ lost: true })));
    expect(state().comparisons[pairId]).toBeNull();
    main.compare.publish("right", snapshotOf(shown()));
    expect(states(state().comparisons[pairId]?.right)).toEqual({ "index.html": "onlyHere" });
  });
});

describe("panes in two windows", () => {
  async function twoWindows() {
    const { opened, sent, close } = openWindows("main", "workspace-1");
    const main = opened.get("main") as TestWindow;
    const other = opened.get("workspace-1") as TestWindow;
    main.panes = [pane("local", { activatedAt: 30 }), pane("spare", { activatedAt: 5 })];
    other.panes = [pane("server", { activatedAt: 20, label: "web.example.com" })];
    expect(await main.compare.toggle("local")).toBeNull();
    await settle();
    const pairId = main.compare.store.getState().pairOf.local;
    return { main, other, sent, close, pairId };
  }

  it("pairs with the pane looked at last wherever it is, and shares the listings", async () => {
    const { main, other, pairId } = await twoWindows();
    const pair = main.compare.store.getState().pairs[pairId];
    expect(pair.right).toEqual({
      window: "workspace-1",
      tabId: "server",
      label: "web.example.com",
    });
    expect(other.compare.store.getState().pairOf.server).toBe(pairId);

    main.compare.publish("local", listed("/home/me/site", HERE));
    other.compare.publish("server", listed("/var/www", THERE));
    await settle();
    const here = main.compare.store.getState();
    const there = other.compare.store.getState();
    expect(states(here.comparisons[pairId]?.left)["edited.txt"]).toBe("newer");
    expect(states(there.comparisons[pairId]?.right)["edited.txt"]).toBe("older");
    // Each window marks the rows of its own pane only.
    expect(here.comparisons[pairId]?.right.size).toBe(0);
    expect(there.comparisons[pairId]?.left.size).toBe(0);
    expect(here.snapshots[paneKey(pair.right)]?.path).toBe("/var/www");
  });

  it("prefers a pane of this window that was looked at later", async () => {
    const { opened } = openWindows("main", "workspace-1");
    const main = opened.get("main") as TestWindow;
    main.panes = [pane("local", { activatedAt: 30 }), pane("spare", { activatedAt: 25 })];
    (opened.get("workspace-1") as TestWindow).panes = [pane("server", { activatedAt: 20 })];
    await main.compare.toggle("local");
    const { pairs, pairOf } = main.compare.store.getState();
    expect(pairs[pairOf.local].right).toMatchObject({ window: "main", tabId: "spare" });
  });

  it("does not wait long for a window that does not answer", async () => {
    vi.useFakeTimers();
    try {
      const silent = createFolderCompare({
        window: "main",
        send: () => undefined,
        otherWindows: async () => ["workspace-9"],
        panes: () => [pane("self"), pane("spare")],
        options: () => DEFAULT_SETTINGS.interface.folderCompare,
        syncDefaults: () => ({ compare: "sizeAndTime", timeToleranceSecs: 2 }),
        compareFolders: () => new Promise(() => undefined),
        cancel: async () => undefined,
        newId: () => "id",
        seekMillis: 50,
      });
      const toggled = silent.toggle("self");
      await vi.advanceTimersByTimeAsync(60);
      expect(await toggled).toBeNull();
      expect(silent.store.getState().pairOf.spare).toBeDefined();
    } finally {
      vi.useRealTimers();
    }
  });

  it("is turned off in both windows from either", async () => {
    const { main, other } = await twoWindows();
    await other.compare.toggle("server");
    await settle();
    expect(main.compare.store.getState().pairs).toEqual({});
    expect(other.compare.store.getState().pairs).toEqual({});
    expect(main.compare.store.getState().snapshots).toEqual({});
  });

  it("ends when the other pane's window is gone", async () => {
    const { main, close } = await twoWindows();
    await main.compare.dropClosedWindows();
    expect(Object.keys(main.compare.store.getState().pairs)).toHaveLength(1);
    close("workspace-1");
    await main.compare.dropClosedWindows();
    expect(main.compare.store.getState().pairs).toEqual({});
  });

  it("tells the other window when its window closes", async () => {
    const { main, other } = await twoWindows();
    other.compare.leaveAll();
    await settle();
    expect(main.compare.store.getState().pairOf).toEqual({});
  });

  it("refuses a pairing with a pane that is no longer there", async () => {
    const { opened, sent } = openWindows("main", "workspace-1");
    const other = opened.get("workspace-1") as TestWindow;
    other.panes = [pane("server")];
    other.compare.receive({
      type: "link",
      from: "main",
      pair: {
        id: "pair-1",
        left: { window: "main", tabId: "local", label: "Local" },
        right: { window: "workspace-1", tabId: "closed", label: "web.example.com" },
      },
    });
    expect(other.compare.store.getState().pairs).toEqual({});
    expect(sent).toEqual([{ type: "unlink", from: "workspace-1", pairId: "pair-1" }]);
  });

  it("takes nothing from a window that is not the partner's, nor anything misshapen", async () => {
    const { main, pairId } = await twoWindows();
    main.compare.receive({ type: "unlink", from: "workspace-7", pairId });
    main.compare.receive({ type: "unlink", pairId });
    main.compare.receive({ type: "explode", from: "workspace-1", pairId });
    main.compare.receive("unlink");
    main.compare.receive(null);
    main.compare.receive({
      type: "snapshot",
      from: "workspace-1",
      pairId,
      tabId: "server",
      snapshot: { path: 7 },
    });
    const state = main.compare.store.getState();
    expect(Object.keys(state.pairs)).toEqual([pairId]);
    expect(state.snapshots[paneKey(state.pairs[pairId].right)]).toBeNull();
    expect(
      isCompareMessage({ type: "link", from: "a", pair: { id: "x", left: {}, right: {} } }),
    ).toBe(false);
    expect(isCompareMessage({ type: "seek", from: "a", seekId: "s" })).toBe(true);
  });

  it("leaves out what is misshapen in a listing or a deep comparison from the other window", async () => {
    const { main, pairId } = await twoWindows();
    main.compare.publish("local", listed("/home/me/site", HERE));
    const right = paneKey(main.compare.store.getState().pairs[pairId].right);
    const snapshot = (value: unknown) => {
      main.compare.receive({
        type: "snapshot",
        from: "workspace-1",
        pairId,
        tabId: "server",
        snapshot: value,
      });
      return main.compare.store.getState().snapshots[right];
    };
    const good = listed("/var/www", THERE, { location: { kind: "remote", sessionId: "s-1" } });
    expect(snapshot(good)).toEqual(good);
    expect(snapshot({ ...good, outdated: true })?.outdated).toBe(true);
    // Not a listing at all.
    expect(snapshot({ ...good, location: { kind: "remote" } })).toBeNull();
    expect(snapshot({ ...good, location: "local" })).toBeNull();
    expect(snapshot({ ...good, pathStyle: "dos" })).toBeNull();
    expect(snapshot({ ...good, entries: "docs" })).toBeNull();
    expect(snapshot([good])).toBeNull();
    // Flags that are not true are false, and entries that are not entries are left out.
    expect(
      snapshot({
        ...good,
        ignoresCase: "yes",
        coarseTimes: 1,
        outdated: "no",
        showingResults: "yes",
        parent: 7,
        entries: [
          { name: "a.txt", shape: "file", size: 1, modified: null, hidden: "no" },
          { name: "b.txt", shape: "file", size: "1", modified: 5 },
          { name: "c.txt", shape: "file", size: 1, modified: "5" },
          { name: "d.txt", shape: "file", size: 1 },
          { name: 4, shape: "file", size: 1, modified: 5 },
          { name: "e", shape: "pipe", size: 1, modified: 5 },
          null,
          "f.txt",
        ],
      }),
    ).toEqual({
      ...good,
      ignoresCase: false,
      coarseTimes: false,
      showingResults: false,
      parent: null,
      entries: [{ name: "a.txt", shape: "file", size: 1, modified: null, hidden: false }],
    });
    expect(states(main.compare.store.getState().comparisons[pairId]?.left)).toMatchObject({
      docs: "onlyHere",
    });

    const deep = (value: unknown) => {
      main.compare.receive({ type: "deep", from: "workspace-1", pairId, deep: value });
      return main.compare.store.getState().deep[pairId];
    };
    const done = {
      status: "done",
      leftRoot: "/home/me/site",
      rightRoot: "/var/www",
      ignoreCase: false,
      folders: [summary("docs", { identical: 1 })],
      truncated: false,
      options: "",
    };
    expect(deep(done)).toMatchObject({ status: "done" });
    expect(deep({ ...done, folders: { docs: summary("docs") } })).toBeUndefined();
    expect(deep({ ...done, folders: [summary("docs"), { identical: 1 }] })).toBeUndefined();
    expect(deep({ ...done, folders: [summary("docs"), null] })).toBeUndefined();
    expect(deep({ ...done, folders: [{ ...summary("docs"), path: 3 }] })).toBeUndefined();
    expect(deep({ ...done, leftRoot: null })).toBeUndefined();
    expect(deep({ status: "running" })).toBeUndefined();
    expect(deep({ status: "paused" })).toBeUndefined();
    // Counts that are not numbers count nothing.
    expect(deep({ ...done, folders: [{ path: "docs", identical: "4", different: 2 }] })).toEqual({
      status: "done",
      comparison: {
        leftRoot: "/home/me/site",
        rightRoot: "/var/www",
        ignoreCase: false,
        folders: new Map([["docs", summary("docs", { different: 2 })]]),
        truncated: false,
      },
      options: "",
    });
  });

  it("shows a deep comparison started in one window in the other", async () => {
    const { main, other, pairId } = await twoWindows();
    main.compare.publish("local", listed("/home/me/site", HERE));
    other.compare.publish("server", listed("/var/www", THERE));
    await settle();
    void other.compare.startDeep("server");
    await settle();
    const running = main.compare.store.getState().deep[pairId];
    expect(running).toMatchObject({ status: "running" });
    const requestId = other.requests[0].requestId;
    main.compare.progress({
      requestId,
      stage: "listing",
      localEntries: 12,
      remoteEntries: 30,
      compared: 0,
      toCompare: 0,
    });
    expect(main.compare.store.getState().deep[pairId]).toMatchObject({
      leftEntries: 12,
      rightEntries: 30,
    });
    // Stopped from the window that did not start it.
    main.compare.stopDeep("local");
    expect(main.cancelled).toEqual([requestId]);
    other.finish({
      folders: [summary("docs", { identical: 3 })],
      truncated: false,
      leftEntries: 4,
      rightEntries: 4,
    });
    await settle();
    expect(states(main.compare.store.getState().comparisons[pairId]?.left).docs).toBe("identical");
    expect(states(other.compare.store.getState().comparisons[pairId]?.right).docs).toBe(
      "identical",
    );
  });
});

function summary(path: string, more: Partial<FolderSummary> = {}): FolderSummary {
  return {
    path,
    identical: 0,
    newerLeft: 0,
    newerRight: 0,
    onlyLeft: 0,
    onlyRight: 0,
    different: 0,
    notCompared: 0,
    ...more,
  };
}

describe("comparing what is below the folders", () => {
  const found = (folders: FolderSummary[]): FolderComparison => ({
    folders,
    truncated: false,
    leftEntries: 9,
    rightEntries: 9,
  });

  it("asks for the two folders by the rules the rows are colored by", async () => {
    const main = oneWindow(pane("left"), pane("right"));
    main.options = { ...main.options, ignoreHourOffsets: true, includeHidden: false };
    main.sync = { compare: "sizeOnly", timeToleranceSecs: 5 };
    await main.compare.toggle("left");
    main.compare.publish("left", listed("C:\\Site", HERE, { ignoresCase: true }));
    main.compare.publish(
      "right",
      listed("/var/www", THERE, {
        location: { kind: "remote", sessionId: "session-1" },
        coarseTimes: true,
      }),
    );
    void main.compare.startDeep("right");
    expect(main.requests).toEqual([
      {
        requestId: expect.any(String),
        // The backend knows which listings give coarse times, so it is not told.
        left: { location: { kind: "local" }, path: "C:\\Site" },
        right: { location: { kind: "remote", sessionId: "session-1" }, path: "/var/www" },
        sizeOnly: true,
        timeToleranceSecs: 5,
        ignoreHourOffsets: true,
        includeHidden: false,
        ignoreCase: true,
      },
    ]);
    // One at a time.
    void main.compare.startDeep("left");
    expect(main.requests).toHaveLength(1);
  });

  it("marks the folders both sides have, wherever in the comparison the panes are", async () => {
    const { main, pairId, state } = await comparedPair();
    const started = main.compare.startDeep("left");
    main.finish(
      found([
        summary("docs", { identical: 2, newerLeft: 1 }),
        summary("docs/api", { identical: 5 }),
      ]),
    );
    await started;
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    expect(state().comparisons[pairId]?.left.get("docs")).toEqual({
      state: "differsInside",
      title: "Differs inside: 1 newer here, 2 identical",
    });
    expect(state().comparisons[pairId]?.right.get("docs")?.title).toBe(
      "Differs inside: 1 older here, 2 identical",
    );
    // Both panes one folder down: the comparison still covers what they show.
    main.compare.publish("left", listed("/home/me/site/docs", [folder("api")]));
    main.compare.publish("right", listed("/var/www/docs", [folder("api")]));
    expect(states(state().comparisons[pairId]?.left)).toEqual({ api: "identical" });
    // Not the same folder of it on both sides: nothing is claimed.
    main.compare.publish("right", listed("/var/www", [folder("api")]));
    expect(states(state().comparisons[pairId]?.left)).toEqual({ api: "both" });
  });

  it("drops what it found when a folder it covers changes or the options do", async () => {
    const { main, pairId, state } = await comparedPair();
    const compareDeep = async () => {
      const started = main.compare.startDeep("left");
      main.finish(found([summary("docs", { identical: 2 })]));
      await started;
      expect(state().comparisons[pairId]?.left.get("docs")?.state).toBe("identical");
    };
    await compareDeep();
    // The same listing loaded again keeps it.
    main.compare.publish("right", listed("/var/www", [...THERE]));
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    main.compare.publish("right", listed("/var/www", [...THERE, file("late.txt", 1, 1)]));
    expect(state().deep[pairId]).toBeUndefined();
    expect(state().comparisons[pairId]?.left.get("docs")?.state).toBe("both");

    main.compare.rulesChanged();
    await compareDeep();
    main.options = { ...main.options, hideIdentical: true, browseTogether: true };
    main.compare.rulesChanged();
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    main.options = { ...main.options, includeHidden: false };
    main.compare.rulesChanged();
    expect(state().deep[pairId]).toBeUndefined();
  });

  it("drops what it found, or stops looking, when a pane has nothing on show", async () => {
    const { main, pairId, state } = await comparedPair();
    const started = main.compare.startDeep("left");
    main.finish(found([summary("docs", { identical: 2 })]));
    await started;
    expect(state().deep[pairId]).toMatchObject({ status: "done" });
    // A vault locks, say, or the connection is lost.
    main.compare.publish("right", null);
    expect(state().deep[pairId]).toBeUndefined();
    // Shown again, the folder has to be compared again for its marks.
    main.compare.publish("right", listed("/var/www", THERE));
    expect(state().comparisons[pairId]?.left.get("docs")?.state).toBe("both");

    const running = main.compare.startDeep("left");
    const requestId = main.requests[1].requestId;
    main.compare.publish("left", null);
    expect(main.cancelled).toEqual([requestId]);
    expect(state().deep[pairId]).toBeUndefined();
    // What it finds afterwards is not shown.
    main.finish(found([summary("docs", { identical: 2 })]));
    await running;
    expect(state().deep[pairId]).toBeUndefined();
  });

  it("can be stopped, and says when it failed", async () => {
    const { main, pairId, state } = await comparedPair();
    let started = main.compare.startDeep("left");
    main.compare.stopDeep("right");
    expect(main.cancelled).toEqual([main.requests[0].requestId]);
    main.fail({ kind: "cancelled", message: "Cancelled" });
    await started;
    expect(state().deep[pairId]).toBeUndefined();

    started = main.compare.startDeep("left");
    main.fail({ kind: "permissionDenied", message: "Permission denied: /var/www/private" });
    await started;
    expect(state().deep[pairId]).toEqual({
      status: "failed",
      message: "Permission denied: /var/www/private",
    });
    main.compare.dismissDeep("left");
    expect(state().deep[pairId]).toBeUndefined();
  });

  it("stops with the comparison, and ignores what arrives afterwards", async () => {
    const { main, state } = await comparedPair();
    const started = main.compare.startDeep("left");
    await main.compare.toggle("left");
    expect(main.cancelled).toHaveLength(1);
    main.finish(found([summary("docs")]));
    await started;
    expect(state().deep).toEqual({});
  });

  it("does not show what was found by options changed while it ran", async () => {
    const { main, pairId, state } = await comparedPair();
    main.compare.rulesChanged();
    const started = main.compare.startDeep("left");
    main.options = { ...main.options, sizeRule: "sizeOnly" };
    main.compare.rulesChanged();
    main.finish(found([summary("docs", { identical: 1 })]));
    await started;
    expect(state().deep[pairId]).toBeUndefined();
  });
});

describe("browsing two panes together", () => {
  /** A pane that goes where it is told and shows `folders` there. */
  function follower(main: TestWindow, tabId: string, root: string, folders: string[]) {
    let path = root;
    const moves: BrowseMove[] = [];
    const show = (next: string) => {
      path = next;
      main.compare.publish(tabId, listed(path, folders.map(folder)));
    };
    main.compare.attach(tabId, {
      follow: (move) => {
        moves.push(move);
        if (move.kind === "up") {
          const parent = path.slice(0, path.lastIndexOf("/")) || "/";
          return path === "/" ? null : { path: parent, done: Promise.resolve(true) };
        }
        if (!folders.includes(move.name)) return null;
        return { path: `${path}/${move.name}`, done: Promise.resolve(true) };
      },
    });
    return { moves, show, arrive: (next: string) => show(next), path: () => path };
  }

  async function browsing() {
    const main = oneWindow(pane("left"), pane("right", { label: "web.example.com" }));
    main.options = { ...main.options, browseTogether: true };
    await main.compare.toggle("left");
    const pairId = main.compare.store.getState().pairOf.left;
    const left = follower(main, "left", "/home/me/site", ["css", "img"]);
    const right = follower(main, "right", "/var/www", ["css"]);
    left.show("/home/me/site");
    right.show("/var/www");
    return { main, pairId, left, right, state: () => main.compare.store.getState() };
  }

  it("reads a move from where a pane was and where it is", () => {
    const from = listed("/var/www", []);
    expect(moveBetween(from, listed("/var/www/css", []))).toEqual({ kind: "enter", name: "css" });
    expect(moveBetween(from, listed("/var", []))).toEqual({ kind: "up" });
    expect(moveBetween(from, listed("/etc/nginx", []))).toBeNull();
    expect(moveBetween(listed("/", []), listed("/srv", []))).toEqual({
      kind: "enter",
      name: "srv",
    });
  });

  it("enters the folder of the same name in the other pane, and goes up with it", async () => {
    const { pairId, left, right, state } = await browsing();
    left.show("/home/me/site/css");
    expect(right.moves).toEqual([{ kind: "enter", name: "css" }]);
    // Nothing is compared until the other pane has got there.
    expect(state().comparisons[pairId]).toBeNull();
    right.arrive("/var/www/css");
    expect(state().comparisons[pairId]).not.toBeNull();
    // The pane that followed does not send the first one on again.
    expect(left.moves).toEqual([]);

    right.show("/var/www");
    expect(left.moves).toEqual([{ kind: "up" }]);
    left.arrive("/home/me/site");
    expect(right.moves).toHaveLength(1);
    expect(state().notes[pairId]).toBeUndefined();
  });

  it("stays put and says so when the other pane has no such folder", async () => {
    const { pairId, left, right, state } = await browsing();
    left.show("/home/me/site/img");
    expect(right.path()).toBe("/var/www");
    expect(state().notes[pairId]).toBe("No folder named img in web.example.com");
    expect(state().comparisons[pairId]).not.toBeNull();
    // The pane that went on alone comes back: the other has not moved, and they are in step.
    left.show("/home/me/site");
    expect(state().notes[pairId]).toBeUndefined();
    expect(right.moves).toEqual([{ kind: "enter", name: "img" }]);
    left.show("/home/me/site/css");
    expect(right.moves).toHaveLength(2);
  });

  it("leaves the other pane where it is while one is further down alone", async () => {
    const { pairId, left, right, state } = await browsing();
    left.show("/home/me/site/img");
    left.show("/home/me/site/img/icons");
    left.show("/home/me/site/img");
    expect(right.moves).toHaveLength(1);
    expect(state().notes[pairId]).toBe("No folder named img in web.example.com");
    left.show("/home/me/site");
    expect(state().notes[pairId]).toBeUndefined();
    expect(right.path()).toBe("/var/www");
    // A jump elsewhere puts them in step again as well.
    left.show("/home/me/site/img");
    left.show("/srv");
    expect(state().notes[pairId]).toBeUndefined();
  });

  it("leaves a pane showing search results where it is", async () => {
    const { main, pairId, left, right, state } = await browsing();
    main.compare.publish(
      "right",
      listed("/var/www", ["css"].map(folder), { showingResults: true }),
    );
    left.show("/home/me/site/css");
    expect(right.moves).toEqual([]);
    expect(state().notes[pairId]).toBe("web.example.com shows search results");
    // Back where the two were, they are in step again.
    left.show("/home/me/site");
    expect(state().notes[pairId]).toBeUndefined();
  });

  it("does nothing while it is off, or for a jump elsewhere", async () => {
    const { main, left, right } = await browsing();
    left.show("/srv/other");
    expect(right.moves).toEqual([]);
    main.options = { ...main.options, browseTogether: false };
    left.show("/srv/other/css");
    expect(right.moves).toEqual([]);
  });

  it("keeps up with a pane that moves on before the other has arrived", async () => {
    const { pairId, left, right, state } = await browsing();
    left.show("/home/me/site/css");
    left.show("/home/me/site");
    expect(right.moves).toEqual([{ kind: "enter", name: "css" }]);
    right.arrive("/var/www/css");
    expect(right.moves).toEqual([{ kind: "enter", name: "css" }, { kind: "up" }]);
    expect(state().comparisons[pairId]).toBeNull();
    right.arrive("/var/www");
    expect(state().comparisons[pairId]).not.toBeNull();
    expect(left.moves).toEqual([]);
  });

  it("says when the other pane could not open the folder", async () => {
    const { main, pairId, state } = await browsing();
    main.compare.attach("right", {
      follow: () => ({ path: "/var/www/css", done: Promise.resolve(false) }),
    });
    main.compare.publish("left", listed("/home/me/site/css", []));
    await settle();
    expect(state().notes[pairId]).toBe("web.example.com could not open /var/www/css");
    expect(state().comparisons[pairId]).not.toBeNull();
  });

  it("works between two windows", async () => {
    const { opened } = openWindows("main", "workspace-1");
    const main = opened.get("main") as TestWindow;
    const other = opened.get("workspace-1") as TestWindow;
    main.panes = [pane("local")];
    other.panes = [pane("server", { label: "web.example.com" })];
    main.options = other.options = { ...main.options, browseTogether: true };
    await main.compare.toggle("local");
    await settle();
    const pairId = main.compare.store.getState().pairOf.local;
    const local = follower(main, "local", "/home/me/site", ["css", "img"]);
    const server = follower(other, "server", "/var/www", ["css"]);
    local.show("/home/me/site");
    server.show("/var/www");
    await settle();

    local.show("/home/me/site/css");
    await settle();
    expect(server.moves).toEqual([{ kind: "enter", name: "css" }]);
    expect(main.compare.store.getState().comparisons[pairId]).toBeNull();
    server.arrive("/var/www/css");
    await settle();
    expect(main.compare.store.getState().comparisons[pairId]).not.toBeNull();
    expect(other.compare.store.getState().comparisons[pairId]).not.toBeNull();
    expect(local.moves).toEqual([]);

    local.show("/home/me/site/css/img");
    await settle();
    const note = "No folder named img in web.example.com";
    expect(main.compare.store.getState().notes[pairId]).toBe(note);
    expect(other.compare.store.getState().notes[pairId]).toBe(note);
    expect(main.compare.store.getState().comparisons[pairId]).not.toBeNull();
  });
});
