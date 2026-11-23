// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FileSource } from "../lib/fileSource";
import type { AppError, DirListing, FileEntry } from "../lib/types";
import { listingShare, paneListings } from "../state/listings";
import { usePane, type PaneController, type PaneOptions } from "./usePane";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SCOPE = '["server","sftp","web.example.com",22,"deploy",null]';
const cache = listingShare.cache;

function file(path: string, size = 1): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind: "file",
    size,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function listing(path: string, names: string[]): DirListing {
  const parent = path === "/" ? null : path.slice(0, path.lastIndexOf("/")) || "/";
  return { path, parent, entries: names.map((name) => file(`${path}/${name}`)) };
}

interface Asked {
  path: string;
  resolve: (listing: DirListing) => void;
  reject: (error: AppError) => void;
}

/** A server whose folders are listed when the test says so, in the order it says. */
class FakeServer {
  /** Every folder asked for, in order, answered or not. */
  readonly asked: string[] = [];
  private readonly unanswered: Asked[] = [];

  /** A session on the server: sessions of one connection share `scope`. */
  source(sessionId: string, change: Partial<FileSource> = {}): FileSource {
    const unused = async () => {
      throw new Error("Not used by the pane");
    };
    return {
      key: `remote:${sessionId}`,
      kind: "remote",
      label: "web",
      pathStyle: "posix",
      listingScope: SCOPE,
      initialPath: async () => "/home",
      home: async () => "/home",
      list: (path) =>
        new Promise<DirListing>((resolve, reject) => {
          this.asked.push(path);
          this.unanswered.push({ path, resolve, reject });
        }),
      mkdir: unused,
      makeFile: unused,
      rename: unused,
      remove: unused,
      ...change,
    };
  }

  /** How many requests wait for an answer. */
  get waiting(): number {
    return this.unanswered.length;
  }

  /** The oldest request for the folder still waiting. */
  private take(path: string): Asked {
    const index = this.unanswered.findIndex((request) => request.path === path);
    if (index < 0) throw new Error(`Nobody waits for ${path}`);
    return this.unanswered.splice(index, 1)[0];
  }

  async answer(path: string, names: string[], listedAs = path): Promise<void> {
    const request = this.take(path);
    await act(async () => request.resolve(listing(listedAs, names)));
  }

  async fail(path: string, error: AppError): Promise<void> {
    const request = this.take(path);
    await act(async () => request.reject(error));
  }
}

interface Mounted {
  readonly pane: PaneController;
  rerender: (options: PaneOptions) => Promise<void>;
}

const mounted: (() => void)[] = [];

async function mount(source: FileSource, options: PaneOptions = {}): Promise<Mounted> {
  let latest: PaneController | null = null;
  function Probe({ paneOptions }: { paneOptions: PaneOptions }) {
    latest = usePane(source, paneOptions);
    return null;
  }
  const root = createRoot(document.createElement("div"));
  const render = (paneOptions: PaneOptions) =>
    act(async () => root.render(createElement(Probe, { paneOptions })));
  await render(options);
  mounted.push(() => act(() => root.unmount()));
  return {
    get pane() {
      if (!latest) throw new Error("The pane did not render");
      return latest;
    },
    rerender: render,
  };
}

const REMEMBERING: PaneOptions = { listings: paneListings };
const names = (pane: PaneController) => pane.listing?.entries.map((entry) => entry.name);

beforeEach(() => cache.clear());
afterEach(() => mounted.splice(0).forEach((unmount) => unmount()));

/** A pane on `/home` with `/home/site` seen before and remembered. */
async function paneThatVisited(server: FakeServer, options: PaneOptions = REMEMBERING) {
  const view = await mount(server.source("one"), options);
  await server.answer("/home", ["notes.txt", "site"]);
  await act(async () => void view.pane.navigate("/home/site"));
  await server.answer("/home/site", ["a.html", "b.html", "c.html"]);
  await act(async () => view.pane.goUp());
  await server.answer("/home", ["notes.txt", "site"]);
  return view;
}

describe("a folder seen before", () => {
  it("shows at once, marked as refreshing until the server answers", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    let opened: boolean | undefined;
    await act(async () => {
      void view.pane.navigate("/home/site").then((result) => {
        opened = result;
      });
    });
    // Nothing has come back from the server, and the folder is on show.
    expect(server.waiting).toBe(1);
    expect(view.pane.listing?.path).toBe("/home/site");
    expect(names(view.pane)).toEqual(["a.html", "b.html", "c.html"]);
    expect(view.pane.stale).toBe("refreshing");
    expect(view.pane.loading).toBe(true);
    expect(opened).toBe(true);

    await server.answer("/home/site", ["a.html", "c.html", "d.html"]);
    expect(names(view.pane)).toEqual(["a.html", "c.html", "d.html"]);
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(false);
    expect(view.pane.error).toBeNull();
  });

  it("changes nothing when the server lists what is on show", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    const shown = view.pane.listing;
    const visible = view.pane.visibleEntries;
    await server.answer("/home/site", ["a.html", "b.html", "c.html"]);
    expect(view.pane.listing).toBe(shown);
    expect(view.pane.visibleEntries).toBe(visible);
    expect(view.pane.stale).toBeNull();
  });

  it("keeps the selection and the cursor by path when the server's listing differs", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await act(async () => {
      view.pane.selectPaths(
        new Set(["/home/site/b.html", "/home/site/c.html"]),
        "/home/site/c.html",
        "/home/site/c.html",
      );
    });
    const row = () => view.pane.listing?.entries.find((entry) => entry.name === "c.html");
    const before = row();
    await server.answer("/home/site", ["0.html", "a.html", "c.html"]);
    expect([...view.pane.selection]).toEqual(["/home/site/c.html"]);
    expect(view.pane.cursorPath).toBe("/home/site/c.html");
    // A row that did not change is the object it was, so it is not drawn again.
    expect(row()).toBe(before);
  });

  it("does not put a late answer on the folder the user went to meanwhile", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    expect(view.pane.stale).toBe("refreshing");
    // Before the server answers for the remembered folder, the user is off to another.
    await act(async () => void view.pane.navigate("/srv"));
    expect(view.pane.listing?.path).toBe("/home/site");
    await server.answer("/srv", ["www"]);
    expect(view.pane.listing?.path).toBe("/srv");
    expect(view.pane.stale).toBeNull();

    await server.answer("/home/site", ["late.html"]);
    expect(view.pane.listing?.path).toBe("/srv");
    expect(names(view.pane)).toEqual(["www"]);
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(false);
    // The answer is still what is remembered of the folder it is for.
    expect(cache.get(SCOPE, "/home/site")?.entries.map((entry) => entry.name)).toEqual([
      "late.html",
    ]);
  });

  it("does not put it on another remembered folder either", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await act(async () => view.pane.goUp());
    // Both folders came from memory; the answers arrive the other way around.
    expect(view.pane.listing?.path).toBe("/home");
    await server.answer("/home", ["notes.txt", "site", "new.txt"]);
    await server.answer("/home/site", ["late.html"]);
    expect(view.pane.listing?.path).toBe("/home");
    expect(names(view.pane)).toEqual(["notes.txt", "site", "new.txt"]);
    expect(view.pane.stale).toBeNull();
  });

  it("confirms the folder on show even while the next one is on its way", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await act(async () => void view.pane.navigate("/srv"));
    await server.answer("/home/site", ["a.html"]);
    expect(view.pane.listing?.path).toBe("/home/site");
    expect(names(view.pane)).toEqual(["a.html"]);
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(true);
    await server.answer("/srv", ["www"]);
    expect(view.pane.listing?.path).toBe("/srv");
  });

  it("goes back and forward through remembered folders at once", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => view.pane.goBack());
    expect(view.pane.listing?.path).toBe("/home/site");
    expect(view.pane.canGoForward).toBe(true);
    expect(view.pane.stale).toBe("refreshing");
    await act(async () => view.pane.goBack());
    expect(view.pane.listing?.path).toBe("/home");
    // Coming back out of a folder puts the cursor on it.
    expect(view.pane.cursorPath).toBe("/home/site");
    expect(view.pane.canGoBack).toBe(false);
  });

  it("keeps a row being renamed that the server no longer lists, until that is over", async () => {
    const server = new FakeServer();
    const held = new Set(["/home/site/b.html"]);
    const view = await paneThatVisited(server, { ...REMEMBERING, heldPaths: held });
    await act(async () => void view.pane.navigate("/home/site"));
    await server.answer("/home/site", ["a.html", "c.html"]);
    expect(names(view.pane)).toEqual(["a.html", "c.html", "b.html"]);
    await view.rerender(REMEMBERING);
    expect(names(view.pane)).toEqual(["a.html", "c.html"]);
  });

  it("is not shown from memory where the pane remembers nothing", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server, {});
    await act(async () => void view.pane.navigate("/home/site"));
    expect(view.pane.listing?.path).toBe("/home");
    expect(view.pane.stale).toBeNull();
    await server.answer("/home/site", ["a.html"]);
    expect(view.pane.listing?.path).toBe("/home/site");
  });
});

describe("a reload", () => {
  it("asks the server and shows nothing remembered meanwhile", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    expect(cache.has(SCOPE, "/home")).toBe(true);
    const shown = view.pane.listing;
    await act(async () => void view.pane.refresh());
    expect(server.waiting).toBe(1);
    expect(view.pane.listing).toBe(shown);
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(true);
    await server.answer("/home", ["notes.txt", "site", "fresh.txt"]);
    expect(names(view.pane)).toEqual(["notes.txt", "site", "fresh.txt"]);
  });

  it("is not dropped while a remembered folder waits for the server", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    // A file was just put there: the reload that follows must not be lost.
    await act(async () => void view.pane.refresh("/home/site/new.html"));
    expect(server.waiting).toBe(2);
    await server.answer("/home/site", ["a.html", "b.html", "c.html"]);
    await server.answer("/home/site", ["a.html", "b.html", "c.html", "new.html"]);
    expect(names(view.pane)).toEqual(["a.html", "b.html", "c.html", "new.html"]);
    expect(view.pane.cursorPath).toBe("/home/site/new.html");
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(false);
  });

  it("still waits for a folder that is on its way and not on show yet", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/srv"));
    let reloaded: boolean | undefined;
    await act(async () => {
      void view.pane.refresh().then((result) => {
        reloaded = result;
      });
    });
    expect(reloaded).toBe(false);
    expect(server.waiting).toBe(1);
    await server.answer("/srv", ["www"]);
    expect(view.pane.listing?.path).toBe("/srv");
  });
});

describe("two panes on one folder", () => {
  it("both show what the server tells one of them", async () => {
    const server = new FakeServer();
    const first = await mount(server.source("one"), REMEMBERING);
    await server.answer("/home", ["notes.txt"]);
    // Another tab on the same connection, with a session of its own.
    const second = await mount(server.source("two"), REMEMBERING);
    expect(names(second.pane)).toEqual(["notes.txt"]);
    await server.answer("/home", ["notes.txt"]);

    await act(async () => void first.pane.refresh());
    await server.answer("/home", ["notes.txt", "upload.zip"]);
    expect(names(first.pane)).toEqual(["notes.txt", "upload.zip"]);
    expect(names(second.pane)).toEqual(["notes.txt", "upload.zip"]);
    // The second pane did not ask the server again for it.
    expect(server.asked).toHaveLength(3);
    expect(server.waiting).toBe(0);
  });

  it("leaves a pane on another folder or another connection alone", async () => {
    const server = new FakeServer();
    const first = await mount(server.source("one"), REMEMBERING);
    await server.answer("/home", ["notes.txt"]);
    const elsewhere = await mount(
      server.source("two", { initialPath: async () => "/srv" }),
      REMEMBERING,
    );
    await server.answer("/srv", ["www"]);
    const otherAccount = await mount(
      server.source("three", { listingScope: SCOPE.replace("deploy", "root") }),
      REMEMBERING,
    );
    await server.answer("/home", ["root-only.txt"]);

    await act(async () => void first.pane.refresh());
    await server.answer("/home", ["notes.txt", "upload.zip"]);
    expect(names(elsewhere.pane)).toEqual(["www"]);
    expect(names(otherAccount.pane)).toEqual(["root-only.txt"]);
  });
});

describe("a refresh that fails", () => {
  const gone: AppError = { kind: "notFound", message: "No such folder" };
  const dropped: AppError = { kind: "disconnected", message: "The connection was lost" };

  it("takes a remembered listing off show when its folder cannot be listed", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await server.fail("/home/site", gone);
    expect(view.pane.listing).toBeNull();
    expect(view.pane.error).toEqual(gone);
    expect(view.pane.failedPath).toBe("/home/site");
    expect(view.pane.stale).toBeNull();
    expect(view.pane.loading).toBe(false);
    expect([...view.pane.selection]).toEqual([]);
    // It is not remembered any longer, and neither is anything inside it.
    expect(cache.has(SCOPE, "/home/site")).toBe(false);
    expect(view.pane.canGoBack).toBe(true);

    // Trying again asks for the same folder.
    await act(async () => void view.pane.refresh());
    await server.answer("/home/site", ["back.html"]);
    expect(names(view.pane)).toEqual(["back.html"]);
    expect(view.pane.failedPath).toBeNull();
    expect(view.pane.error).toBeNull();
  });

  it("goes up from a folder that could no longer be listed", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await server.fail("/home/site", gone);
    expect(view.pane.canGoUp).toBe(true);
    await act(async () => view.pane.goUp());
    expect(view.pane.listing?.path).toBe("/home");
    expect(view.pane.failedPath).toBeNull();
    expect(view.pane.error).toBeNull();
  });

  it("does so for a reload of a listing the server gave before", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.refresh());
    await server.fail("/home", { kind: "permissionDenied", message: "Permission denied" });
    expect(view.pane.listing).toBeNull();
    expect(view.pane.failedPath).toBe("/home");
    expect(view.pane.error?.kind).toBe("permissionDenied");
    expect(cache.has(SCOPE, "/home")).toBe(false);
  });

  it("keeps the listing up, marked as out of date, when the server cannot be reached", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await server.fail("/home/site", dropped);
    expect(names(view.pane)).toEqual(["a.html", "b.html", "c.html"]);
    expect(view.pane.stale).toBe("outdated");
    expect(view.pane.error).toEqual(dropped);
    expect(view.pane.failedPath).toBeNull();
    expect(view.pane.loading).toBe(false);
    // Nothing says the folder changed, so it is still remembered for when the server is back.
    expect(cache.has(SCOPE, "/home/site")).toBe(true);

    await act(async () => void view.pane.refresh());
    await server.answer("/home/site", ["a.html"]);
    expect(view.pane.stale).toBeNull();
    expect(view.pane.error).toBeNull();
  });

  const passing: AppError[] = [
    { kind: "cloud", message: "SlowDown: Please reduce your request rate." },
    { kind: "ftp", message: "451 Requested action aborted: local error in processing" },
  ];

  it.each(passing)(
    "keeps a remembered listing up, marked as out of date, after a passing $kind error",
    async (failure) => {
      const server = new FakeServer();
      const view = await paneThatVisited(server);
      await act(async () => void view.pane.navigate("/home/site"));
      await server.fail("/home/site", failure);
      expect(names(view.pane)).toEqual(["a.html", "b.html", "c.html"]);
      expect(view.pane.stale).toBe("outdated");
      expect(view.pane.error).toEqual(failure);
      expect(view.pane.failedPath).toBeNull();
      expect(cache.has(SCOPE, "/home/site")).toBe(true);
    },
  );

  it.each(passing)(
    "keeps a reloaded listing up, with its selection, after a passing $kind error",
    async (failure) => {
      const server = new FakeServer();
      const view = await paneThatVisited(server);
      await act(async () => view.pane.selectOnly("/home/notes.txt"));
      const shown = view.pane.listing;
      await act(async () => void view.pane.refresh());
      await server.fail("/home", failure);
      expect(view.pane.listing).toBe(shown);
      expect([...view.pane.selection]).toEqual(["/home/notes.txt"]);
      expect(view.pane.stale).toBe("outdated");
      expect(view.pane.error).toEqual(failure);
      expect(view.pane.failedPath).toBeNull();
      expect(cache.has(SCOPE, "/home")).toBe(true);
    },
  );

  it("opens the usual first folder when the remembered one to start in is gone", async () => {
    const server = new FakeServer();
    cache.put(SCOPE, listing("/home/old", ["was-here.txt"]));
    const source = server.source("one", {
      initialPath: async () => "/home/old",
      fallbackPath: async () => "/home",
    });
    const view = await mount(source, REMEMBERING);
    expect(names(view.pane)).toEqual(["was-here.txt"]);
    await server.fail("/home/old", gone);
    await server.answer("/home", ["notes.txt"]);
    expect(view.pane.listing?.path).toBe("/home");
    expect(view.pane.failedPath).toBeNull();
    expect(view.pane.error).toBeNull();
    expect(view.pane.canGoBack).toBe(false);
  });

  it("forgets a folder that cannot be listed though the pane has left it", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/home/site"));
    await act(async () => view.pane.goUp());
    await server.fail("/home/site", gone);
    expect(view.pane.listing?.path).toBe("/home");
    expect(view.pane.error).toBeNull();
    expect(view.pane.stale).toBe("refreshing");
    expect(cache.has(SCOPE, "/home/site")).toBe(false);
    expect(cache.has(SCOPE, "/home")).toBe(true);
  });

  it("leaves the folder on show alone when another one cannot be opened", async () => {
    const server = new FakeServer();
    const view = await paneThatVisited(server);
    await act(async () => void view.pane.navigate("/nowhere"));
    await server.fail("/nowhere", gone);
    expect(view.pane.listing?.path).toBe("/home");
    expect(view.pane.error).toEqual(gone);
    expect(view.pane.failedPath).toBeNull();
    expect(view.pane.stale).toBeNull();
  });

  it("keeps a listing up as before where the pane remembers nothing", async () => {
    const server = new FakeServer();
    const view = await mount(server.source("one"), {});
    await server.answer("/home", ["notes.txt"]);
    await act(async () => void view.pane.refresh());
    await server.fail("/home", gone);
    expect(names(view.pane)).toEqual(["notes.txt"]);
    expect(view.pane.error).toEqual(gone);
    expect(view.pane.failedPath).toBeNull();
  });
});

describe("search results in a pane that remembers listings", () => {
  const found: DirListing = {
    path: "/home",
    parent: null,
    entries: [file("/home/site/a.html"), file("/home/site/old/a.html")],
  };

  /** A pane on `/home` with `/home/site` remembered, showing results found in `/home`. */
  async function paneWithResults(server: FakeServer) {
    const view = await paneThatVisited(server);
    await view.rerender({ ...REMEMBERING, results: found });
    await act(async () => view.pane.showResults("/home"));
    return view;
  }

  it("are never remembered as the listing of the folder they were found in", async () => {
    const server = new FakeServer();
    const view = await paneWithResults(server);
    expect(view.pane.showingResults).toBe(true);
    expect(view.pane.listing?.entries.map((entry) => entry.path)).toEqual([
      "/home/site/a.html",
      "/home/site/old/a.html",
    ]);
    expect(cache.get(SCOPE, "/home")?.entries.map((entry) => entry.name)).toEqual([
      "notes.txt",
      "site",
    ]);
    // A reload lists the folder behind them, which is remembered as the folder's; they stay.
    await act(async () => void view.pane.refresh());
    await server.answer("/home", ["notes.txt", "site", "todo.txt"]);
    expect(view.pane.showingResults).toBe(true);
    expect(view.pane.listing).toBe(found);
    expect(view.pane.stale).toBeNull();
    expect(cache.get(SCOPE, "/home")?.entries.map((entry) => entry.name)).toEqual([
      "notes.txt",
      "site",
      "todo.txt",
    ]);
  });

  it("go back to the folder they were found in, shown at once from memory", async () => {
    const server = new FakeServer();
    const view = await paneWithResults(server);
    await act(async () => view.pane.goBack());
    expect(view.pane.showingResults).toBe(false);
    expect(view.pane.listing?.path).toBe("/home");
    expect(names(view.pane)).toEqual(["notes.txt", "site"]);
    expect(view.pane.stale).toBe("refreshing");
    await server.answer("/home", ["notes.txt", "site", "todo.txt"]);
    expect(names(view.pane)).toEqual(["notes.txt", "site", "todo.txt"]);
    expect(view.pane.stale).toBeNull();
    // And forward to them again, as they were.
    await act(async () => view.pane.goForward());
    expect(view.pane.showingResults).toBe(true);
    expect(view.pane.listing).toBe(found);
  });

  it("are not replaced by a listing of their folder another pane remembers", async () => {
    const server = new FakeServer();
    const view = await paneWithResults(server);
    const other = await mount(server.source("two"), REMEMBERING);
    await server.answer("/home", ["notes.txt", "site", "new.txt"]);
    expect(names(other.pane)).toEqual(["notes.txt", "site", "new.txt"]);
    expect(view.pane.listing).toBe(found);
    // Closing them shows the folder as the other pane last listed it.
    await act(async () => view.pane.closeResults());
    expect(view.pane.showingResults).toBe(false);
    expect(names(view.pane)).toEqual(["notes.txt", "site", "new.txt"]);
  });
});
