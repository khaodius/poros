import { afterEach, describe, expect, it } from "vitest";
import type { FileEntry } from "../lib/types";
import { dropAction } from "./fileOperations";
import { registerPane, type PaneHandle } from "./paneRegistry";

function entry(path: string, kind: FileEntry["kind"] = "file"): FileEntry {
  return {
    name: path.slice(path.lastIndexOf("/") + 1),
    path,
    kind,
    size: 0,
    modified: null,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

const unregister: (() => void)[] = [];

function pane(tabId: string, kind: PaneHandle["kind"], path: string, sessionId?: string) {
  unregister.push(
    registerPane({
      tabId,
      kind,
      sessionId,
      label: tabId,
      path: () => path,
      refresh: () => undefined,
      selected: () => [],
      visible: true,
      activatedAt: 0,
    }),
  );
}

afterEach(() => unregister.splice(0).forEach((remove) => remove()));

const onPane = (tabId: string, folder: string | null) => ({
  kind: "pane" as const,
  tabId,
  folder,
});

describe("dropAction", () => {
  it("moves within a server, or copies with the copy key", () => {
    pane("server", "remote", "/srv/www", "s1");
    const files = [entry("/srv/www/index.html")];
    expect(dropAction("server", files, onPane("server", "/srv/www/assets"), false)).toMatchObject({
      kind: "place",
      mode: "move",
      folder: "/srv/www/assets",
    });
    expect(dropAction("server", files, onPane("server", "/srv/www/assets"), true)).toMatchObject({
      kind: "place",
      mode: "copy",
    });
  });

  it("does nothing where the files already are", () => {
    pane("server", "remote", "/srv/www", "s1");
    pane("same", "remote", "/srv/www", "s1");
    const files = [entry("/srv/www/index.html")];
    expect(dropAction("server", files, onPane("server", null), false)).toBeNull();
    expect(dropAction("server", files, onPane("same", null), false)).toBeNull();
    expect(dropAction("server", files, onPane("same", null), true)).toMatchObject({
      kind: "place",
      mode: "copy",
    });
  });

  it("keeps folders out of themselves", () => {
    pane("server", "remote", "/srv", "s1");
    pane("inside", "remote", "/srv/www/assets", "s1");
    const folder = [entry("/srv/www", "dir")];
    expect(dropAction("server", folder, onPane("server", "/srv/www"), false)).toBeNull();
    expect(dropAction("server", folder, onPane("inside", null), false)).toMatchObject({
      kind: "blocked",
    });
  });

  it("transfers between this computer and a server", () => {
    pane("local", "local", "/home/me");
    pane("server", "remote", "/srv/www", "s1");
    const files = [entry("/home/me/notes.txt")];
    expect(dropAction("local", files, onPane("server", null), false)).toEqual({
      kind: "transfer",
      direction: "upload",
      folder: "/srv/www",
    });
  });

  it("refuses to go between two servers", () => {
    pane("first", "remote", "/srv", "s1");
    pane("second", "remote", "/srv", "s2");
    expect(dropAction("first", [entry("/srv/a.txt")], onPane("second", null), false)).toMatchObject(
      { kind: "blocked" },
    );
  });
});
